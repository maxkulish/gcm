use std::collections::HashSet;
use std::io::Read;
use std::path::Path;

use crate::error::GcmError;
use crate::git::{ChangedFile, Repo};

/// JSON-encode the changed-file paths as an array of strings so a path
/// containing a newline (or any character) stays a single discrete element in
/// the grouping prompt - newline-joining would split such a path into multiple
/// lines and the model would group phantom paths (CLO-487 path-agreement).
fn file_list_json(changed: &[ChangedFile]) -> String {
    let paths: Vec<&str> = changed.iter().map(|c| c.path.as_str()).collect();
    serde_json::to_string(&paths).unwrap_or_else(|_| "[]".to_string())
}

/// JSON-encode the porcelain status as an array of `"XY path"` strings (also
/// newline-safe, same rationale as [`file_list_json`]).
fn status_json(changed: &[ChangedFile]) -> String {
    let rows: Vec<String> = changed
        .iter()
        .map(|c| format!("{}{} {}", c.x as char, c.y as char, c.path))
        .collect();
    serde_json::to_string(&rows).unwrap_or_else(|_| "[]".to_string())
}

/// Untracked-expansion caps (FR-57): bound both file count and total bytes so an
/// un-ignored directory of thousands of files cannot freeze the CLI.
const MAX_UNTRACKED_FILES: usize = 50;
const MAX_UNTRACKED_BYTES: usize = 256 * 1024;
/// Per-file read cap for an individual untracked file (mirrors bash `head -c 8192`).
const PER_FILE_BYTES: usize = 8192;

/// Per-provider diff budget (CLO-489, FR-13a): the coarse total-body safeguard
/// and the per-tracked-file truncation cap. Each provider declares its own (a
/// smaller-context model like `gpt-4o-mini` gets a tighter total); env overrides
/// `GCM_DIFF_TOTAL_BYTES` / `GCM_DIFF_PER_FILE_BYTES` apply across providers.
pub struct DiffBudget {
    /// Coarse final safeguard on the whole assembled body. Applied on **every**
    /// path (single-commit `gather`, per-group `gather_for_files`, and grouping
    /// `gather_for_grouping`) - this is the per-provider total-input cap.
    pub total_bytes: usize,
    /// Per-file cap for a tracked diff section in the **grouping** prompt only:
    /// each file's section is truncated independently with a `[diff omitted: N
    /// bytes]` placeholder rather than tail-chopping the whole body (CLO-487
    /// FR-15, so every changed file stays present for the model to group). The
    /// single-commit and per-group-message paths intentionally do NOT apply
    /// per-file truncation (they rely on `total_bytes`), preserving the CLO-486
    /// tracer's full-diff behavior (O3 parity).
    pub per_file_bytes: usize,
}

impl DiffBudget {
    /// Standard defaults (today's Groq values) - behavioral parity (O3).
    pub const STANDARD_TOTAL: usize = 350_000;
    pub const STANDARD_PER_FILE: usize = 8192;

    /// Build a budget from per-provider defaults, applying the cross-provider env
    /// overrides when set to a non-empty, positive integer.
    pub fn resolve(default_total: usize, default_per_file: usize) -> Self {
        Self::from_overrides(
            env_usize("GCM_DIFF_TOTAL_BYTES"),
            env_usize("GCM_DIFF_PER_FILE_BYTES"),
            default_total,
            default_per_file,
        )
    }

    /// Pure budget assembly (env reads factored out for unit testing): an override
    /// wins when present, else the provider default.
    fn from_overrides(
        total: Option<usize>,
        per_file: Option<usize>,
        default_total: usize,
        default_per_file: usize,
    ) -> Self {
        DiffBudget {
            total_bytes: total.unwrap_or(default_total),
            per_file_bytes: per_file.unwrap_or(default_per_file),
        }
    }

    /// The standard (Groq/Google) budget with env overrides applied.
    pub fn standard() -> Self {
        Self::resolve(Self::STANDARD_TOTAL, Self::STANDARD_PER_FILE)
    }
}

/// Read a non-empty, parseable, positive `usize` env var, else `None`.
fn env_usize(name: &str) -> Option<usize> {
    std::env::var(name)
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|&n| n > 0)
}

/// The diff context handed to the provider.
pub struct GatheredDiff {
    pub stat: String,
    pub body: String,
}

/// The richer context handed to the provider for grouping (CLO-487): the file
/// list and porcelain status (both JSON arrays, so newline-containing paths stay
/// discrete), the diff `--stat`, and the per-file-truncated full diff. Distinct
/// from [`GatheredDiff`] to keep the tracer's single-message concerns separate.
pub struct GroupingContext {
    /// JSON array of the exact changed paths (the model groups by these).
    pub file_list: String,
    /// JSON array of `"XY path"` porcelain status rows.
    pub status: String,
    pub stat: String,
    pub body: String,
}

/// Build the single-message diff for **one commit group** (CLO-491, FR-45): the
/// tracked diff and stat scoped to the group's paths, plus the group's own
/// untracked files (filtered, so other groups' untracked content never leaks
/// into this message). Used to regenerate a message-only call for an advanced
/// group on a cache hit. Unborn-safe: with no `HEAD` the tracked diff is empty
/// and all content arrives through the filtered untracked path.
pub fn gather_for_files(
    repo: &Repo,
    files: &[&ChangedFile],
    budget: &DiffBudget,
) -> Result<GatheredDiff, GcmError> {
    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    let stat = repo.diff_stat_for(&paths)?;
    let tracked = repo.diff_full_for(&paths)?;
    let mut body = elide_binary_diff(&tracked);
    let allow: HashSet<String> = files.iter().map(|f| f.path.clone()).collect();
    append_untracked(repo, &mut body, Some(&allow))?;
    cap_total(&mut body, budget.total_bytes);
    Ok(GatheredDiff { stat, body })
}

/// Build the single-commit diff for a filtered changed-file set (CLO-490). This
/// is the `--all`/fallback equivalent of [`gather_for_files`], but it takes the
/// owned changed slice the main flow already filtered.
pub fn gather_for_changed(
    repo: &Repo,
    changed: &[ChangedFile],
    budget: &DiffBudget,
) -> Result<GatheredDiff, GcmError> {
    let files: Vec<&ChangedFile> = changed.iter().collect();
    gather_for_files(repo, &files, budget)
}

/// Build the grouping context (CLO-487): the file list and porcelain status are
/// derived from the already-gathered `changed` set (so they stay byte-identical
/// to the paths used for validation and staging), the diff `--stat` is the
/// prompt header, and the body is the tracked diff truncated **per file** with
/// `[diff omitted: N bytes]` placeholders, plus untracked content (FR-57 caps),
/// under the `MAX_TOTAL_BYTES` final safeguard.
pub fn gather_for_grouping(
    repo: &Repo,
    changed: &[ChangedFile],
    budget: &DiffBudget,
) -> Result<GroupingContext, GcmError> {
    let file_list = file_list_json(changed);
    let status = status_json(changed);

    let paths: Vec<&str> = changed.iter().map(|f| f.path.as_str()).collect();
    let stat = repo.diff_stat_for(&paths)?;
    let tracked = repo.diff_full_for(&paths)?;
    let mut body = truncate_per_file(&elide_binary_diff(&tracked), budget.per_file_bytes);
    let allow: HashSet<String> = changed.iter().map(|f| f.path.clone()).collect();
    append_untracked(repo, &mut body, Some(&allow))?;
    cap_total(&mut body, budget.total_bytes);

    Ok(GroupingContext {
        file_list,
        status,
        stat,
        body,
    })
}

/// Append untracked, non-gitignored file content to `body`, bounded by the
/// FR-57 file-count and byte caps. Shared by [`gather`] and
/// [`gather_for_grouping`] (which pass `None` = every untracked file) and
/// [`gather_for_files`] (which passes `Some(allow)` to restrict to one group's
/// paths, so a single group's message diff is not polluted by other groups'
/// untracked files - CLO-491).
fn append_untracked(
    repo: &Repo,
    body: &mut String,
    allow: Option<&HashSet<String>>,
) -> Result<(), GcmError> {
    let mut untracked = repo.untracked_files()?;
    untracked.sort();

    // Every (allow-listed) untracked path counts toward the file-count cap -
    // binary and unreadable files included - so a directory of thousands of
    // files (of any kind) cannot force thousands of reads. Once either cap is
    // reached, every remaining file is listed by name only, with no read at all
    // (FR-57).
    let mut files_done = 0usize;
    let mut bytes_used = 0usize;
    for path in &untracked {
        // Filter to the allow-list (if any) before the caps, so excluded paths
        // neither consume the budget nor reach the prompt.
        if allow.is_some_and(|a| !a.contains(path)) {
            continue;
        }
        if files_done >= MAX_UNTRACKED_FILES || bytes_used >= MAX_UNTRACKED_BYTES {
            body.push_str(&format!(
                "\n--- /dev/null\n+++ b/{path}\n[content omitted: untracked cap reached]\n"
            ));
            continue;
        }
        let full = repo.root().join(path);
        // Only read regular files. `symlink_metadata` does not follow symlinks,
        // so we never read a symlink's target (which could leak content from
        // outside the repo) and never block on a FIFO/device/socket.
        let is_regular = std::fs::symlink_metadata(&full)
            .map(|m| m.file_type().is_file())
            .unwrap_or(false);
        if !is_regular {
            body.push_str(&format!(
                "\n--- /dev/null\n+++ b/{path}\n[omitted: not a regular file]\n"
            ));
            files_done += 1;
            continue;
        }
        // Read at most a per-file slice bounded by the remaining byte budget, so
        // a single huge file is never loaded into memory in full.
        let budget = (MAX_UNTRACKED_BYTES - bytes_used).min(PER_FILE_BYTES);
        match read_capped(&full, budget) {
            Ok((content, more)) if looks_binary(&content) => {
                body.push_str(&format!("\n--- /dev/null\n+++ b/{path}\n+[binary file]\n"));
                let _ = more;
            }
            Ok((content, more)) => {
                let text = String::from_utf8_lossy(&content);
                body.push_str(&format!("\n--- /dev/null\n+++ b/{path}\n"));
                for line in text.lines() {
                    body.push('+');
                    body.push_str(line);
                    body.push('\n');
                }
                if more {
                    body.push_str("+[truncated]\n");
                }
                bytes_used += content.len();
            }
            Err(_) => {
                // Unreadable (perm, race, symlink loop) - note by name, never block.
                body.push_str(&format!(
                    "\n--- /dev/null\n+++ b/{path}\n[omitted: unreadable]\n"
                ));
            }
        }
        files_done += 1;
    }
    Ok(())
}

/// Coarse final safeguard on the whole assembled body (FR-57), truncating on a
/// char boundary so a multibyte char split at the cap does not panic.
fn cap_total(body: &mut String, total_bytes: usize) {
    if body.len() > total_bytes {
        let mut end = total_bytes;
        while end > 0 && !body.is_char_boundary(end) {
            end -= 1;
        }
        body.truncate(end);
        body.push_str("\n... (diff truncated)\n");
    }
}

/// Truncate a tracked diff **per file**: split on `diff --git ` boundaries and,
/// for any section longer than `cap`, keep the file's header and replace its
/// hunk body with `[diff omitted: N bytes]` (N = omitted bytes). This keeps
/// every changed file present in the prompt instead of tail-chopping the whole
/// body and severing the last file mid-hunk (CLO-487 FR-15).
fn truncate_per_file(diff: &str, cap: usize) -> String {
    let mut out = String::new();
    let mut section = String::new();
    for line in diff.split_inclusive('\n') {
        if line.starts_with("diff --git ") && !section.is_empty() {
            push_capped_section(&section, cap, &mut out);
            section.clear();
        }
        section.push_str(line);
    }
    if !section.is_empty() {
        push_capped_section(&section, cap, &mut out);
    }
    out
}

fn push_capped_section(section: &str, cap: usize, out: &mut String) {
    if section.len() <= cap {
        out.push_str(section);
        return;
    }
    // Keep the header (lines up to the first hunk `@@`); if there is no hunk
    // marker, keep just the first line. Replace the rest with a byte-count
    // placeholder.
    let mut header_end = None;
    let mut idx = 0;
    let mut first_line_end = section.len();
    for (i, line) in section.split_inclusive('\n').enumerate() {
        if i == 0 {
            first_line_end = line.len();
        }
        if line.starts_with("@@") {
            header_end = Some(idx);
            break;
        }
        idx += line.len();
    }
    let header = &section[..header_end.unwrap_or(first_line_end)];
    let omitted = section.len() - header.len();
    out.push_str(header);
    if !header.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(&format!("[diff omitted: {omitted} bytes]\n"));
}

/// Read at most `cap` bytes from a file without loading it fully into memory.
/// Returns the bytes and whether the file had more content beyond `cap`.
fn read_capped(path: &Path, cap: usize) -> std::io::Result<(Vec<u8>, bool)> {
    let file = std::fs::File::open(path)?;
    // Read one extra byte so we can tell whether the file exceeded the cap.
    let mut buf = Vec::new();
    file.take(cap as u64 + 1).read_to_end(&mut buf)?;
    let more = buf.len() > cap;
    buf.truncate(cap);
    Ok((buf, more))
}

/// Heuristic: is this byte sample binary? NUL bytes or invalid UTF-8 (beyond a
/// possible multibyte char split at the sample boundary) mean binary. UTF-8 text
/// (including non-ASCII) is preserved (FR-32, NUL-misclassification guard).
fn looks_binary(bytes: &[u8]) -> bool {
    let sample = &bytes[..bytes.len().min(8192)];
    if sample.contains(&0) {
        return true;
    }
    match std::str::from_utf8(sample) {
        Ok(_) => false,
        // A trailing multibyte char split by the 8192-byte window is fine (<=3 bytes).
        Err(e) => e.valid_up_to() < sample.len().saturating_sub(3),
    }
}

/// Per-file binary elision for a tracked diff (port of git-commit-ai.sh:87-119):
/// if a file's hunk body is mostly non-text, keep the `diff --git` header and
/// replace the body with a placeholder; otherwise strip stray NULs and keep it.
fn elide_binary_diff(diff: &str) -> String {
    let mut out = String::new();
    let mut buf = String::new();
    for line in diff.split_inclusive('\n') {
        if line.starts_with("diff --git ") {
            flush_section(&buf, &mut out);
            buf.clear();
        }
        buf.push_str(line);
    }
    flush_section(&buf, &mut out);
    out
}

fn flush_section(section: &str, out: &mut String) {
    if section.is_empty() {
        return;
    }
    let mut header = String::new();
    let mut body = String::new();
    let mut in_body = false;
    for line in section.split_inclusive('\n') {
        if !in_body && line.starts_with("@@") {
            in_body = true;
        }
        if in_body {
            body.push_str(line);
        } else {
            header.push_str(line);
        }
    }

    let mut sample = String::new();
    for line in body.lines() {
        let stripped = line
            .strip_prefix(|c| c == '+' || c == '-' || c == ' ')
            .unwrap_or(line);
        sample.push_str(stripped);
    }
    // Count NUL, the UTF-8 replacement char (U+FFFD, what lossy decoding turns
    // raw binary bytes into), and control chars as "non-text". Valid non-ASCII
    // text (Cyrillic, CJK, etc.) is NOT counted, so it is never wrongly elided.
    let total = sample.chars().count();
    let non_text = sample
        .chars()
        .filter(|&c| {
            c == '\u{0}'
                || c == '\u{FFFD}'
                || (c.is_control() && c != '\t' && c != '\n' && c != '\r')
        })
        .count();

    if total > 200 && (non_text as f64) / (total as f64) > 0.10 {
        out.push_str(&header);
        if !header.ends_with('\n') {
            out.push('\n');
        }
        let lines = body.lines().count();
        out.push_str(&format!(
            "Binary files differ (body elided: {total} bytes, {lines} diff lines)\n"
        ));
    } else {
        out.push_str(&section.replace('\0', ""));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_budget_override_wins_else_default() {
        // An env override (Some) wins; a default fills in for None (CLO-489 AC-8).
        let b = DiffBudget::from_overrides(Some(1000), None, 350_000, 8192);
        assert_eq!(b.total_bytes, 1000);
        assert_eq!(b.per_file_bytes, 8192);
        let d = DiffBudget::from_overrides(None, None, 256_000, 8192);
        assert_eq!(d.total_bytes, 256_000);
        assert_eq!(d.per_file_bytes, 8192);
    }

    #[test]
    fn ascii_text_is_not_binary() {
        assert!(!looks_binary(b"fn main() {}\n"));
    }

    #[test]
    fn utf8_unicode_text_is_not_binary() {
        assert!(!looks_binary("файл: привет мир\n".as_bytes()));
    }

    #[test]
    fn nul_bytes_are_binary() {
        // A file git's 8000-byte heuristic could misclassify as text but which
        // carries NUL bytes must be treated as binary (Novel #9).
        let mut data = b"looks like text for a while ".repeat(4);
        data.push(0);
        data.extend_from_slice(b"more");
        assert!(looks_binary(&data));
    }

    #[test]
    fn invalid_utf8_is_binary() {
        assert!(looks_binary(&[0xff, 0xfe, 0xfd, 0x00, 0x01, 0x02]));
    }

    #[test]
    fn text_diff_is_preserved() {
        let diff = "diff --git a/x.rs b/x.rs\n--- a/x.rs\n+++ b/x.rs\n@@ -1 +1 @@\n-old\n+new\n";
        let out = elide_binary_diff(diff);
        assert!(out.contains("+new"));
        assert!(!out.contains("body elided"));
    }

    #[test]
    fn cyrillic_text_diff_is_not_elided() {
        // Valid non-ASCII (UTF-8) text must not be misclassified as binary even
        // though every Cyrillic byte is > 0x7e.
        let mut diff = String::from("diff --git a/doc.txt b/doc.txt\n@@ -0,0 +1 @@\n");
        for _ in 0..50 {
            diff.push_str("+Добавлен новый раздел документации про настройку\n");
        }
        let out = elide_binary_diff(&diff);
        assert!(out.contains("Добавлен"), "Cyrillic text preserved");
        assert!(!out.contains("body elided"), "valid UTF-8 not elided");
    }

    #[test]
    fn binary_diff_body_is_elided() {
        let mut diff = String::from("diff --git a/img.png b/img.png\n@@ -0,0 +1 @@\n");
        // A long, mostly-non-text body.
        for _ in 0..50 {
            diff.push('+');
            diff.push_str("\u{0}\u{1}\u{2}\u{3}\u{4}\u{5}\u{6}\u{7}\u{8}\u{e}\n");
        }
        let out = elide_binary_diff(&diff);
        assert!(out.contains("diff --git a/img.png b/img.png"));
        assert!(out.contains("body elided"));
        assert!(!out.contains('\u{0}'));
    }

    #[test]
    fn read_capped_bounds_large_files() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(&vec![b'a'; 100_000]).unwrap();
        f.flush().unwrap();
        let (buf, more) = read_capped(f.path(), 8192).unwrap();
        assert_eq!(
            buf.len(),
            8192,
            "read is bounded to the cap, not the file size"
        );
        assert!(more, "more flag set when the file exceeds the cap");
    }

    #[test]
    fn read_capped_small_file_has_no_more() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(b"short").unwrap();
        f.flush().unwrap();
        let (buf, more) = read_capped(f.path(), 8192).unwrap();
        assert_eq!(buf, b"short");
        assert!(!more);
    }

    #[test]
    fn small_diff_section_is_unchanged() {
        let diff = "diff --git a/x.rs b/x.rs\n--- a/x.rs\n+++ b/x.rs\n@@ -1 +1 @@\n-old\n+new\n";
        assert_eq!(truncate_per_file(diff, 8192), diff);
    }

    #[test]
    fn large_diff_section_keeps_header_and_omits_body() {
        let mut diff =
            String::from("diff --git a/big.rs b/big.rs\n--- a/big.rs\n+++ b/big.rs\n@@ -1 +1 @@\n");
        for _ in 0..500 {
            diff.push_str("+a line of content that makes this section big\n");
        }
        let out = truncate_per_file(&diff, 200);
        assert!(
            out.contains("diff --git a/big.rs b/big.rs"),
            "diff header kept"
        );
        assert!(out.contains("+++ b/big.rs"), "file header kept");
        assert!(out.contains("[diff omitted:"), "placeholder present");
        assert!(!out.contains("a line of content"), "huge body dropped");
        assert!(out.len() < 300, "section is now small");
    }

    #[test]
    fn file_list_json_keeps_a_newline_path_as_one_element() {
        // A path containing a newline must stay a single discrete element, or
        // the model would see (and group) phantom split paths (Codex finding).
        let changed = vec![
            ChangedFile {
                x: b' ',
                y: b'M',
                path: "weird\nname.txt".to_string(),
                orig_path: None,
            },
            ChangedFile {
                x: b'?',
                y: b'?',
                path: "normal.rs".to_string(),
                orig_path: None,
            },
        ];
        let parsed: Vec<String> = serde_json::from_str(&file_list_json(&changed)).unwrap();
        assert_eq!(
            parsed,
            vec!["weird\nname.txt".to_string(), "normal.rs".to_string()]
        );
        let rows: Vec<String> = serde_json::from_str(&status_json(&changed)).unwrap();
        assert_eq!(rows[0], " M weird\nname.txt");
    }

    #[test]
    fn truncates_per_file_so_a_small_file_after_a_huge_one_survives() {
        // Whole-body tail-chop would sever the trailing small file; per-file
        // truncation keeps it intact (the CLO-487 fix).
        let mut diff = String::from("diff --git a/big.rs b/big.rs\n@@ -1 +1 @@\n");
        for _ in 0..500 {
            diff.push_str("+filler filler filler filler filler\n");
        }
        diff.push_str(
            "diff --git a/small.rs b/small.rs\n--- a/small.rs\n+++ b/small.rs\n@@ -1 +1 @@\n+tiny\n",
        );
        let out = truncate_per_file(&diff, 200);
        assert!(out.contains("diff --git a/small.rs"), "small file present");
        assert!(out.contains("+tiny"), "small file body intact");
        assert!(out.contains("[diff omitted:"), "big file elided");
        assert!(!out.contains("filler filler"), "big file body dropped");
    }
}
