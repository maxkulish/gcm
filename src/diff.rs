use std::io::Read;
use std::path::Path;

use crate::error::GcmError;
use crate::git::Repo;

/// Untracked-expansion caps (FR-57): bound both file count and total bytes so an
/// un-ignored directory of thousands of files cannot freeze the CLI.
const MAX_UNTRACKED_FILES: usize = 50;
const MAX_UNTRACKED_BYTES: usize = 256 * 1024;
/// Per-file read cap for an individual untracked file (mirrors bash `head -c 8192`).
const PER_FILE_BYTES: usize = 8192;
/// Coarse final safeguard on the whole assembled body.
const MAX_TOTAL_BYTES: usize = 350_000;

/// The diff context handed to the provider.
pub struct GatheredDiff {
    pub stat: String,
    pub body: String,
}

/// Build the prompt diff: tracked changes (binary-elided) plus untracked,
/// non-gitignored file content, bounded by the FR-57 caps. Reads only the
/// working tree; nothing is staged (FR-47).
pub fn gather(repo: &Repo) -> Result<GatheredDiff, GcmError> {
    let stat = repo.diff_stat()?;
    let tracked = repo.diff_full()?;
    let mut body = elide_binary_diff(&tracked);

    let mut untracked = repo.untracked_files()?;
    untracked.sort();

    // Every untracked path counts toward the file-count cap - binary and
    // unreadable files included - so a directory of thousands of files (of any
    // kind) cannot force thousands of reads. Once either cap is reached, every
    // remaining file is listed by name only, with no read at all (FR-57).
    let mut files_done = 0usize;
    let mut bytes_used = 0usize;
    for path in &untracked {
        if files_done >= MAX_UNTRACKED_FILES || bytes_used >= MAX_UNTRACKED_BYTES {
            body.push_str(&format!(
                "\n--- /dev/null\n+++ b/{path}\n[content omitted: untracked cap reached]\n"
            ));
            continue;
        }
        let full = repo.root().join(path);
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

    if body.len() > MAX_TOTAL_BYTES {
        body.truncate(MAX_TOTAL_BYTES);
        body.push_str("\n... (diff truncated)\n");
    }

    Ok(GatheredDiff { stat, body })
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
    let total = sample.len();
    let non_text = sample
        .bytes()
        .filter(|&c| c != b'\t' && c != b'\n' && c != b'\r' && !(0x20..=0x7e).contains(&c))
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
}
