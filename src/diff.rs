use std::collections::HashSet;
use std::io::Read;
use std::path::Path;

use crate::budget::{estimate_tokens, PromptBudget};
use crate::error::GcmError;
use crate::git::{ChangedFile, Repo};
use crate::provider::grouping_user_content;

/// One changed path in the grouping inventory (CLO-797): the path the model
/// groups by, its porcelain `XY` code, and the source path of a rename or copy.
#[derive(serde::Serialize)]
struct InventoryEntry<'a> {
    path: &'a str,
    xy: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    old_path: Option<&'a str>,
}

/// JSON-encode the changed files as one inventory, `[{path, xy, old_path?}]`,
/// so the prompt carries each path once (CLO-797) and a path containing a
/// newline (or any character) stays a single discrete element - newline-joining
/// would split it and the model would group phantom paths (CLO-487).
pub fn files_json(changed: &[ChangedFile]) -> String {
    let entries: Vec<InventoryEntry> = changed
        .iter()
        .map(|c| InventoryEntry {
            path: &c.path,
            xy: format!("{}{}", c.x as char, c.y as char),
            old_path: c.orig_path.as_deref(),
        })
        .collect();
    serde_json::to_string(&entries).unwrap_or_else(|_| "[]".to_string())
}

/// Untracked-expansion caps (FR-57): bound both file count and total bytes so an
/// un-ignored directory of thousands of files cannot freeze the CLI.
const MAX_UNTRACKED_FILES: usize = 50;
const MAX_UNTRACKED_BYTES: usize = 256 * 1024;
/// Per-file read cap for an individual untracked file (mirrors bash `head -c 8192`).
const PER_FILE_BYTES: usize = 8192;

/// How [`append_untracked`] renders the untracked files it does not read
/// because a FR-57 cap was reached.
#[derive(Clone, Copy, PartialEq, Eq)]
enum UntrackedRender {
    /// One `--- /dev/null` stub per skipped file (the message paths).
    PerFileStubs,
    /// One trailer counting the skipped files (the grouping path, whose
    /// inventory already names every path - CLO-797).
    SummaryTrailer,
}

/// Per-provider diff budget (CLO-489, FR-13a): the coarse total-body safeguard
/// and the per-tracked-file truncation cap. Each provider declares its own (a
/// smaller-context provider gets a tighter total); env overrides
/// `GCM_DIFF_TOTAL_BYTES` / `GCM_DIFF_PER_FILE_BYTES` apply across providers.
pub struct DiffBudget {
    /// Coarse final safeguard on the whole assembled body - the per-provider
    /// total-input cap. The message paths tail-chop at it; the grouping path
    /// treats it as a ceiling its ladder keeps under (CLO-797).
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

impl GatheredDiff {
    /// Per-section byte counts, largest concern first (CLO-798). Logged at debug
    /// so a prompt-size regression is diagnosable without a packet capture.
    pub fn section_sizes(&self) -> Vec<(&'static str, usize)> {
        vec![("stat", self.stat.len()), ("body", self.body.len())]
    }

    /// Total bytes of prompt text. Approximate by design: it counts the sections
    /// gcm assembles, not the serialized request body a provider finally sees.
    pub fn approx_bytes(&self) -> usize {
        self.section_sizes().iter().map(|(_, n)| n).sum()
    }
}

/// The richer context handed to the provider for grouping (CLO-487): the
/// changed-file inventory (a JSON array, so newline-containing paths stay
/// discrete), the diff `--stat`, and the per-file-truncated full diff. Distinct
/// from [`GatheredDiff`] to keep the tracer's single-message concerns separate.
pub struct GroupingContext {
    /// JSON array of `{path, xy, old_path?}` - the one path inventory the model
    /// groups by (CLO-797).
    pub files: String,
    pub stat: String,
    pub body: String,
}

impl GroupingContext {
    /// Per-section byte counts (CLO-798), in prompt order. `body` is the section
    /// that grows without bound on a large change set, so seeing the split is
    /// what makes an oversized grouping prompt diagnosable.
    pub fn section_sizes(&self) -> Vec<(&'static str, usize)> {
        vec![
            ("files", self.files.len()),
            ("stat", self.stat.len()),
            ("body", self.body.len()),
        ]
    }

    /// Total bytes of prompt text. Approximate by design: it counts the sections
    /// gcm assembles, not the serialized request body a provider finally sees.
    pub fn approx_bytes(&self) -> usize {
        self.section_sizes().iter().map(|(_, n)| n).sum()
    }
}

/// Render `bytes` for a human-facing status line (CLO-798): `820 B`, `48 KB`,
/// `1.4 MB`. Deliberately coarse - the point is an order of magnitude, and exact
/// counts stay in the debug lines.
pub fn human_bytes(bytes: usize) -> String {
    const KB: usize = 1024;
    const MB: usize = KB * 1024;
    if bytes < KB {
        format!("{bytes} B")
    } else if bytes < MB {
        format!("{} KB", bytes / KB)
    } else {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    }
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
    append_untracked(
        repo,
        &mut body,
        Some(&allow),
        UntrackedRender::PerFileStubs,
        PER_FILE_BYTES,
    )?;
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

/// Which rung of the sacrifice ladder produced a grouping context (CLO-797
/// D3). Each rung gives up more of the diff body; the inventory and the stat
/// are never cut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupingRung {
    /// Today's per-file cap, and the prompt fits.
    Full,
    /// Every file's diff cut to a lower per-file cap.
    Shrunk { per_file_bytes: usize },
    /// Diff headers only, each body replaced by its omitted byte count.
    HeadersOnly,
    /// Inventory and stat only.
    PathsOnly,
    /// Even the inventory and stat exceed the budget; the context is the
    /// paths-only one, and the pre-flight check turns it into an error.
    CannotFit,
}

/// A grouping context with the rung that produced it, its estimated size in
/// tokens, and how many files lost diff content to the ladder.
pub struct GroupingGather {
    pub ctx: GroupingContext,
    pub rung: GroupingRung,
    pub estimate: usize,
    pub reduced_files: usize,
}

/// The lowest per-file cap the shrink rung tries before falling back to
/// headers only.
const MIN_SHRUNK_PER_FILE: usize = 256;
const BODY_OMITTED: &str = "[diff omitted to fit the model's context window - see the stat]\n";

/// The pieces of a grouping prompt that every rung shares.
struct GroupingParts<'a> {
    repo: &'a Repo,
    allow: HashSet<String>,
    files: String,
    stat: String,
    tracked: String,
}

impl<'a> GroupingParts<'a> {
    fn gather(repo: &'a Repo, changed: &[ChangedFile]) -> Result<Self, GcmError> {
        let paths: Vec<&str> = changed.iter().map(|f| f.path.as_str()).collect();
        Ok(GroupingParts {
            repo,
            allow: changed.iter().map(|f| f.path.clone()).collect(),
            files: files_json(changed),
            stat: repo.diff_stat_for(&paths)?,
            tracked: elide_binary_diff(&repo.diff_full_for(&paths)?),
        })
    }

    /// Render the context with each file's diff (tracked and untracked) cut to
    /// `per_file` bytes; `None` drops the body. Also returns how many untracked
    /// files the cap truncated.
    fn render(&self, per_file: Option<usize>) -> Result<(GroupingContext, usize), GcmError> {
        let (body, untracked_cut) = match per_file {
            None => (BODY_OMITTED.to_string(), 0),
            Some(cap) => {
                let mut body = truncate_per_file(&self.tracked, cap);
                let cut = append_untracked(
                    self.repo,
                    &mut body,
                    Some(&self.allow),
                    UntrackedRender::SummaryTrailer,
                    cap.min(PER_FILE_BYTES),
                )?;
                (body, cut)
            }
        };
        let ctx = GroupingContext {
            files: self.files.clone(),
            stat: self.stat.clone(),
            body,
        };
        Ok((ctx, untracked_cut))
    }
}

/// Build the grouping context (CLO-487, CLO-797): the file inventory is derived
/// from the already-gathered `changed` set (so it stays byte-identical to the
/// paths used for validation and staging), the diff `--stat` is the prompt
/// header, and the body is the tracked diff truncated **per file** with
/// `[diff omitted: N bytes]` placeholders, plus untracked content (FR-57 caps).
///
/// The rendered prompt must fit `grouping.available()` tokens and the body
/// `diff_budget.total_bytes`. When it does not, the ladder lowers the per-file
/// cap, then keeps headers only, then drops the body. No file is cut mid-hunk.
pub fn gather_for_grouping(
    repo: &Repo,
    changed: &[ChangedFile],
    diff_budget: &DiffBudget,
    grouping: &PromptBudget,
) -> Result<GroupingGather, GcmError> {
    let parts = GroupingParts::gather(repo, changed)?;
    let available = grouping.available();
    let measure = |ctx: &GroupingContext| estimate_tokens(&grouping_user_content(ctx));
    let fits = |ctx: &GroupingContext, estimate: usize| {
        estimate <= available && ctx.body.len() <= diff_budget.total_bytes
    };

    let mut caps = vec![(GroupingRung::Full, Some(diff_budget.per_file_bytes))];
    let mut cap = diff_budget.per_file_bytes / 2;
    while cap >= MIN_SHRUNK_PER_FILE {
        caps.push((
            GroupingRung::Shrunk {
                per_file_bytes: cap,
            },
            Some(cap),
        ));
        cap /= 2;
    }
    caps.push((GroupingRung::HeadersOnly, Some(0)));
    for (rung, per_file) in caps {
        let (ctx, untracked_cut) = parts.render(per_file)?;
        let estimate = measure(&ctx);
        if fits(&ctx, estimate) {
            let reduced_files = match rung {
                GroupingRung::Full => 0,
                GroupingRung::Shrunk { per_file_bytes } => {
                    sections_over(&parts.tracked, per_file_bytes) + untracked_cut
                }
                _ => changed.len(),
            };
            return Ok(GroupingGather {
                ctx,
                rung,
                estimate,
                reduced_files,
            });
        }
    }
    // The terminal decision is whether the inventory and stat fit. The omitted
    // marker is kept when there is room for it and dropped when there is not.
    let (mut ctx, _) = parts.render(None)?;
    let mut estimate = measure(&ctx);
    if !fits(&ctx, estimate) {
        ctx.body.clear();
        estimate = measure(&ctx);
    }
    let rung = if estimate <= available {
        GroupingRung::PathsOnly
    } else {
        GroupingRung::CannotFit
    };
    Ok(GroupingGather {
        ctx,
        rung,
        estimate,
        reduced_files: changed.len(),
    })
}

/// How many `diff --git` sections of `diff` are longer than `cap`, i.e. how
/// many files [`truncate_per_file`] cuts at that cap.
fn sections_over(diff: &str, cap: usize) -> usize {
    let mut over = 0;
    let mut len = 0;
    for line in diff.split_inclusive('\n') {
        if line.starts_with("diff --git ") && len > 0 {
            over += usize::from(len > cap);
            len = 0;
        }
        len += line.len();
    }
    over + usize::from(len > cap)
}

/// Append untracked, non-gitignored file content to `body`, bounded by the
/// FR-57 file-count and byte caps. Shared by [`gather`] and
/// [`gather_for_grouping`] (which pass `None` = every untracked file) and
/// [`gather_for_files`] (which passes `Some(allow)` to restrict to one group's
/// paths, so a single group's message diff is not polluted by other groups'
/// untracked files - CLO-491). `render` decides how files past the caps appear;
/// `per_file` bounds each file's read. Returns how many files that bound
/// truncated.
fn append_untracked(
    repo: &Repo,
    body: &mut String,
    allow: Option<&HashSet<String>>,
    render: UntrackedRender,
    per_file: usize,
) -> Result<usize, GcmError> {
    let mut untracked = repo.untracked_files()?;
    untracked.sort();

    // Every (allow-listed) untracked path counts toward the file-count cap -
    // binary and unreadable files included - so a directory of thousands of
    // files (of any kind) cannot force thousands of reads. Once either cap is
    // reached, every remaining file is listed by name only, with no read at all
    // (FR-57).
    let mut files_done = 0usize;
    let mut bytes_used = 0usize;
    let mut skipped = 0usize;
    let mut truncated = 0usize;
    for path in &untracked {
        // Filter to the allow-list (if any) before the caps, so excluded paths
        // neither consume the budget nor reach the prompt.
        if allow.is_some_and(|a| !a.contains(path)) {
            continue;
        }
        if files_done >= MAX_UNTRACKED_FILES || bytes_used >= MAX_UNTRACKED_BYTES {
            match render {
                UntrackedRender::PerFileStubs => body.push_str(&format!(
                    "\n--- /dev/null\n+++ b/{path}\n[content omitted: untracked cap reached]\n"
                )),
                UntrackedRender::SummaryTrailer => skipped += 1,
            }
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
        let budget = (MAX_UNTRACKED_BYTES - bytes_used).min(per_file);
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
                    truncated += 1;
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
    if skipped > 0 {
        body.push_str(&format!(
            "\n[content omitted: {skipped} further untracked files past the read cap - see the changed-files inventory]\n"
        ));
    }
    Ok(truncated)
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

    fn changed(x: u8, y: u8, path: &str, orig_path: Option<&str>) -> ChangedFile {
        ChangedFile {
            x,
            y,
            path: path.to_string(),
            orig_path: orig_path.map(str::to_string),
        }
    }

    fn inventory(changed: &[ChangedFile]) -> Vec<serde_json::Value> {
        serde_json::from_str(&files_json(changed)).unwrap()
    }

    #[test]
    fn inventory_has_one_entry_per_path_with_its_status() {
        let entries = inventory(&[
            changed(b' ', b'M', "a.rs", None),
            changed(b'?', b'?', "b.md", None),
        ]);
        assert_eq!(
            entries,
            vec![
                serde_json::json!({"path": "a.rs", "xy": " M"}),
                serde_json::json!({"path": "b.md", "xy": "??"}),
            ]
        );
    }

    #[test]
    fn inventory_rename_and_copy_carry_both_names() {
        let files = [
            changed(b'R', b' ', "new.rs", Some("old.rs")),
            changed(b'C', b' ', "copy.rs", Some("src.rs")),
        ];
        let entries = inventory(&files);
        assert_eq!(
            entries[0],
            serde_json::json!({"path": "new.rs", "xy": "R ", "old_path": "old.rs"})
        );
        assert_eq!(
            entries[1],
            serde_json::json!({"path": "copy.rs", "xy": "C ", "old_path": "src.rs"})
        );
        // Both names still reach staging, which reads the same ChangedFile.
        assert_eq!(files[0].stage_paths(), vec!["new.rs", "old.rs"]);
    }

    #[test]
    fn inventory_keeps_awkward_paths_as_one_element() {
        // A path containing a newline, a quote, a backslash or non-ASCII must
        // stay a single discrete element, or the model would see (and group)
        // phantom split paths (Codex finding, CLO-487).
        let paths = [
            "weird\nname.txt",
            "say \"hi\".md",
            "back\\slash.rs",
            "файл.txt",
        ];
        let files: Vec<ChangedFile> = paths.iter().map(|p| changed(b' ', b'M', p, None)).collect();
        let entries = inventory(&files);
        assert_eq!(entries.len(), paths.len());
        for (entry, path) in entries.iter().zip(paths) {
            assert_eq!(entry["path"], *path);
        }
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

    fn git(root: &Path, args: &[&str]) {
        let out = std::process::Command::new("git")
            .current_dir(root)
            .args(args)
            .output()
            .expect("run git");
        assert!(out.status.success(), "git {args:?}: {out:?}");
    }

    /// A repository whose message-path diff exercises every gathering branch:
    /// a text edit, a binary edit, a rename, and more untracked files than
    /// `MAX_UNTRACKED_FILES`. Repo-local config pins the diff format so a
    /// developer's global git config cannot change the output.
    fn message_fixture() -> (tempfile::TempDir, Repo) {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        git(root, &["init", "-q"]);
        for (k, v) in [
            ("user.email", "t@t"),
            ("user.name", "T"),
            ("diff.noprefix", "false"),
            ("diff.mnemonicPrefix", "false"),
            ("diff.renames", "true"),
            ("diff.algorithm", "myers"),
            ("core.autocrlf", "false"),
        ] {
            git(root, &["config", k, v]);
        }
        std::fs::write(root.join("a.rs"), "fn a() {}\n").unwrap();
        std::fs::write(root.join("old.md"), "# title\n\nbody text\n").unwrap();
        std::fs::write(root.join("img.bin"), [0u8, 1, 2, 3, 0, 5]).unwrap();
        git(root, &["add", "-A"]);
        git(root, &["commit", "-q", "-m", "init"]);

        let mut a = String::from("fn a() {}\n");
        for i in 0..40 {
            a.push_str(&format!("fn added_{i}() {{ let x = {i}; }}\n"));
        }
        std::fs::write(root.join("a.rs"), a).unwrap();
        git(root, &["mv", "old.md", "new.md"]);
        std::fs::write(root.join("img.bin"), [0u8, 9, 8, 7, 0, 6, 0]).unwrap();
        for i in 0..(MAX_UNTRACKED_FILES + 5) {
            std::fs::write(
                root.join(format!("u{i:02}.txt")),
                format!("untracked {i}\n"),
            )
            .unwrap();
        }
        let repo = Repo::at(root.to_path_buf());
        (dir, repo)
    }

    fn budget(available: usize) -> PromptBudget {
        PromptBudget {
            kind: crate::budget::BudgetKind::Grouping,
            window_tokens: available,
            envelope_allowance: 0,
            completion_reserve: 0,
        }
    }

    fn roomy() -> PromptBudget {
        budget(10_000_000)
    }

    /// A repository with `n` untracked files and no commits.
    fn untracked_fixture(n: usize) -> (tempfile::TempDir, Repo) {
        let dir = tempfile::tempdir().expect("tempdir");
        git(dir.path(), &["init", "-q"]);
        for i in 0..n {
            std::fs::write(dir.path().join(format!("u{i:02}.txt")), "x\n").unwrap();
        }
        let repo = Repo::at(dir.path().to_path_buf());
        (dir, repo)
    }

    #[test]
    fn untracked_trailer_replaces_stubs_in_grouping() {
        let (_dir, repo) = untracked_fixture(MAX_UNTRACKED_FILES + 10);
        let changed = repo.changed_files().unwrap();
        let ctx = gather_for_grouping(&repo, &changed, &DiffBudget::standard(), &roomy())
            .unwrap()
            .ctx;
        assert_eq!(
            ctx.body.matches("[content omitted:").count(),
            1,
            "one trailer: {}",
            ctx.body
        );
        assert!(ctx
            .body
            .contains("[content omitted: 10 further untracked files past the read cap"));
        assert!(
            !ctx.body.contains("untracked cap reached"),
            "no per-file stubs"
        );
        assert_eq!(
            ctx.body.matches("--- /dev/null").count(),
            MAX_UNTRACKED_FILES,
            "headers only for the files that were read"
        );
    }

    #[test]
    fn gather_for_files_keeps_stubs() {
        let (_dir, repo) = untracked_fixture(MAX_UNTRACKED_FILES + 10);
        let changed = repo.changed_files().unwrap();
        let body = gather_for_changed(&repo, &changed, &DiffBudget::standard())
            .unwrap()
            .body;
        assert_eq!(body.matches("untracked cap reached").count(), 10);
        assert!(!body.contains("further untracked files"), "no trailer");
    }

    /// Three files whose diffs fall between 4 and 8 KiB and one under 256 bytes, so every
    /// ladder rung renders a different prompt.
    fn ladder_fixture() -> (tempfile::TempDir, Repo, Vec<ChangedFile>) {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        git(root, &["init", "-q"]);
        git(root, &["config", "user.email", "t@t"]);
        git(root, &["config", "user.name", "T"]);
        for name in ["a.rs", "b.rs", "c.rs", "small.rs"] {
            std::fs::write(root.join(name), "start\n").unwrap();
        }
        git(root, &["add", "-A"]);
        git(root, &["commit", "-q", "-m", "init"]);
        for name in ["a.rs", "b.rs", "c.rs"] {
            let lines: String = (0..350).map(|i| format!("line {i} of {name}\n")).collect();
            std::fs::write(root.join(name), lines).unwrap();
        }
        std::fs::write(root.join("small.rs"), "start\none\ntwo\nthree\nfour\nend\n").unwrap();
        let repo = Repo::at(root.to_path_buf());
        let changed = repo.changed_files().unwrap();
        (dir, repo, changed)
    }

    /// The estimated size of the prompt one rung renders.
    fn rung_estimate(repo: &Repo, changed: &[ChangedFile], per_file: Option<usize>) -> usize {
        let parts = GroupingParts::gather(repo, changed).unwrap();
        estimate_tokens(&grouping_user_content(&parts.render(per_file).unwrap().0))
    }

    fn ladder(changed: &[ChangedFile], repo: &Repo, available: usize) -> GroupingGather {
        gather_for_grouping(repo, changed, &DiffBudget::standard(), &budget(available)).unwrap()
    }

    #[test]
    fn ladder_fits_untouched() {
        let (_dir, repo, changed) = ladder_fixture();
        let g = ladder(&changed, &repo, 10_000_000);
        assert_eq!(g.rung, GroupingRung::Full);
        assert_eq!(g.reduced_files, 0);
        assert!(g.ctx.body.contains("line 349 of c.rs"));
    }

    #[test]
    fn ladder_rung1_shrinks_the_per_file_cap() {
        let (_dir, repo, changed) = ladder_fixture();
        let fit = rung_estimate(&repo, &changed, Some(4096));
        assert!(fit < rung_estimate(&repo, &changed, Some(8192)));
        let g = ladder(&changed, &repo, fit);
        assert_eq!(
            g.rung,
            GroupingRung::Shrunk {
                per_file_bytes: 4096
            }
        );
        assert_eq!(g.reduced_files, 3, "the three large files are cut");
        assert!(g.estimate <= fit);
        assert!(g.ctx.body.contains("+end"), "the small file keeps its diff");
    }

    #[test]
    fn ladder_rung2_keeps_headers_only() {
        let (_dir, repo, changed) = ladder_fixture();
        let fit = rung_estimate(&repo, &changed, Some(0));
        assert!(fit < rung_estimate(&repo, &changed, Some(MIN_SHRUNK_PER_FILE)));
        let g = ladder(&changed, &repo, fit);
        assert_eq!(g.rung, GroupingRung::HeadersOnly);
        assert_eq!(g.reduced_files, changed.len());
        assert_eq!(g.ctx.body.matches("diff --git ").count(), 4);
        assert!(!g.ctx.body.contains("+end"));
    }

    #[test]
    fn ladder_rung3_drops_the_body() {
        let (_dir, repo, changed) = ladder_fixture();
        let fit = rung_estimate(&repo, &changed, None);
        let g = ladder(&changed, &repo, fit);
        assert_eq!(g.rung, GroupingRung::PathsOnly);
        assert_eq!(g.ctx.body, BODY_OMITTED);
        assert!(g.ctx.files.contains("small.rs"), "inventory never cut");
        assert!(g.ctx.stat.contains("a.rs"), "stat never cut");
    }

    /// The estimate of the inventory and stat alone, with an empty body.
    fn paths_and_stat_estimate(repo: &Repo, changed: &[ChangedFile]) -> usize {
        let parts = GroupingParts::gather(repo, changed).unwrap();
        let (mut ctx, _) = parts.render(None).unwrap();
        ctx.body.clear();
        estimate_tokens(&grouping_user_content(&ctx))
    }

    #[test]
    fn ladder_paths_only_drops_the_marker_when_only_paths_and_stat_fit() {
        let (_dir, repo, changed) = ladder_fixture();
        let exact = paths_and_stat_estimate(&repo, &changed);
        assert!(exact < rung_estimate(&repo, &changed, None));
        let g = ladder(&changed, &repo, exact);
        assert_eq!(g.rung, GroupingRung::PathsOnly);
        assert!(g.ctx.body.is_empty());
        assert_eq!(g.estimate, exact);
    }

    #[test]
    fn ladder_paths_only_drops_the_marker_past_the_byte_ceiling() {
        let (_dir, repo, changed) = ladder_fixture();
        let diff_budget = DiffBudget {
            total_bytes: 1,
            per_file_bytes: DiffBudget::STANDARD_PER_FILE,
        };
        let g = gather_for_grouping(&repo, &changed, &diff_budget, &roomy()).unwrap();
        assert_eq!(g.rung, GroupingRung::PathsOnly);
        assert!(g.ctx.body.is_empty());
    }

    #[test]
    fn ladder_reports_cannot_fit_when_the_inventory_overflows() {
        let (_dir, repo, changed) = ladder_fixture();
        let exact = paths_and_stat_estimate(&repo, &changed);
        let g = ladder(&changed, &repo, exact - 1);
        assert_eq!(g.rung, GroupingRung::CannotFit);
        assert!(g.estimate > exact - 1);
    }

    #[test]
    fn ladder_counts_truncated_untracked_files_as_reduced() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        git(root, &["init", "-q"]);
        for i in 0..5 {
            let lines: String = (0..200)
                .map(|l| format!("untracked {i} line {l}\n"))
                .collect();
            std::fs::write(root.join(format!("u{i}.rs")), lines).unwrap();
        }
        let repo = Repo::at(root.to_path_buf());
        let changed = repo.changed_files().unwrap();
        let fit = rung_estimate(&repo, &changed, Some(512));
        assert!(fit < rung_estimate(&repo, &changed, Some(1024)));
        let g = ladder(&changed, &repo, fit);
        assert_eq!(
            g.rung,
            GroupingRung::Shrunk {
                per_file_bytes: 512
            }
        );
        assert_eq!(g.reduced_files, 5, "every untracked file is cut");
    }

    #[test]
    fn ladder_byte_ceiling_shrinks_instead_of_tail_chopping() {
        let (_dir, repo, changed) = ladder_fixture();
        let shrunk = GroupingParts::gather(&repo, &changed)
            .unwrap()
            .render(Some(4096))
            .unwrap()
            .0;
        let diff_budget = DiffBudget {
            total_bytes: shrunk.body.len(),
            per_file_bytes: DiffBudget::STANDARD_PER_FILE,
        };
        let g = gather_for_grouping(&repo, &changed, &diff_budget, &roomy()).unwrap();
        assert_eq!(
            g.rung,
            GroupingRung::Shrunk {
                per_file_bytes: 4096
            }
        );
        assert!(!g.ctx.body.contains("(diff truncated)"), "no tail-chop");
        assert!(g.ctx.body.contains("+end"), "the last file is intact");
    }

    fn render_gathered(label: &str, g: &GatheredDiff) -> String {
        format!(
            "=== {label} stat ===\n{}=== {label} body ===\n{}",
            g.stat, g.body
        )
    }

    /// Single-commit and per-group message gathering stay byte-identical to the
    /// output captured before CLO-797 touched this file. Set
    /// `GCM_BLESS_MESSAGE_GOLDEN=1` to rewrite the golden after an intended change.
    #[test]
    fn message_gathering_unchanged() {
        let (_dir, repo) = message_fixture();
        let changed = repo.changed_files().unwrap();
        let standard = DiffBudget {
            total_bytes: DiffBudget::STANDARD_TOTAL,
            per_file_bytes: DiffBudget::STANDARD_PER_FILE,
        };
        let tight = DiffBudget {
            total_bytes: 600,
            per_file_bytes: DiffBudget::STANDARD_PER_FILE,
        };
        let subset: Vec<&ChangedFile> = changed
            .iter()
            .filter(|c| c.path == "a.rs" || c.path.starts_with("u0"))
            .collect();

        let mut actual = String::new();
        actual.push_str(&render_gathered(
            "gather_for_changed standard",
            &gather_for_changed(&repo, &changed, &standard).unwrap(),
        ));
        actual.push_str(&render_gathered(
            "gather_for_changed tight",
            &gather_for_changed(&repo, &changed, &tight).unwrap(),
        ));
        actual.push_str(&render_gathered(
            "gather_for_files subset",
            &gather_for_files(&repo, &subset, &standard).unwrap(),
        ));

        let golden =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("src/testdata/message_gathering.golden");
        if std::env::var_os("GCM_BLESS_MESSAGE_GOLDEN").is_some() {
            std::fs::create_dir_all(golden.parent().unwrap()).unwrap();
            std::fs::write(&golden, &actual).unwrap();
        }
        let expected = std::fs::read_to_string(&golden).expect("golden present");
        assert_eq!(actual, expected, "message gathering output changed");
    }
}
