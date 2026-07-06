//! Parse zdiff3 conflict markers into typed hunks (CLO-531, ST2).
//!
//! zdiff3 markers look like:
//!
//! ```text
//! <<<<<<< HEAD
//! ours content
//! ||||||| base commit
//! base content
//! =======
//! theirs content
//! >>>>>>> feature
//! ```
//!
//! Plain diff3 (without base) is accepted gracefully even though zdiff3 always
//! includes the base block.

/// One conflict hunk parsed from zdiff3 markers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    /// 1-based start line of the conflict in the working-tree file.
    pub start_line: usize,
    /// 1-based end line (inclusive).
    pub end_line: usize,
    /// The common ancestor text (zdiff3 `|||||||` block). `None` if absent.
    pub base: Option<String>,
    /// The "ours" / current branch text (`<<<<<<<` block).
    pub ours: String,
    /// The "their" / incoming branch text (`>>>>>>>` block).
    pub theirs: String,
}

/// A conflicted file parsed into its hunks.
#[derive(Debug, Clone)]
pub struct ConflictFile {
    pub path: String,
    pub hunks: Vec<Hunk>,
    /// Lines outside any conflict hunk (context, carried verbatim).
    pub context_lines: Vec<String>,
}

/// Parse a conflicted file into hunks. `path` is the repo-relative path; the
/// caller already read the file content. Lines are 1-based so that downstream
/// previews can show human-friendly line numbers.
pub fn parse(path: String, content: &str) -> ConflictFile {
    let lines: Vec<&str> = content.lines().collect();
    let mut hunks = Vec::new();
    let mut context_lines = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if let Some(_ours_label) = lines[i].strip_prefix("<<<<<<< ") {
            let start_line = i + 1;
            // Collect ours until base or separator.
            let mut ours_lines = Vec::new();
            let mut base_lines = Vec::new();
            let mut saw_base_marker = false;
            let mut theirs_lines = Vec::new();
            let mut state = ParseState::Ours;
            let mut j = i + 1;
            while j < lines.len() {
                let line = lines[j];
                match state {
                    ParseState::Ours => {
                        if line.starts_with("||||||| ") {
                            saw_base_marker = true;
                            state = ParseState::Base;
                        } else if line == "=======" {
                            state = ParseState::Theirs;
                        } else {
                            ours_lines.push(line);
                        }
                    }
                    ParseState::Base => {
                        if line == "=======" {
                            state = ParseState::Theirs;
                        } else {
                            base_lines.push(line);
                        }
                    }
                    ParseState::Theirs => {
                        if line.starts_with(">>>>>>> ") {
                            break;
                        } else {
                            theirs_lines.push(line);
                        }
                    }
                }
                j += 1;
            }
            let end_line = j + 1; // inclusive
            hunks.push(Hunk {
                start_line,
                end_line,
                base: if saw_base_marker {
                    Some(join_lines(&base_lines))
                } else {
                    None
                },
                ours: join_lines(&ours_lines),
                theirs: join_lines(&theirs_lines),
            });
            i = j + 1;
        } else {
            context_lines.push(lines[i].to_string());
            i += 1;
        }
    }
    ConflictFile {
        path,
        hunks,
        context_lines,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParseState {
    Ours,
    Base,
    Theirs,
}

fn join_lines(lines: &[&str]) -> String {
    if lines.is_empty() {
        return String::new();
    }
    let mut s = lines.join("\n");
    // Preserve a trailing newline if the original section had one. The section
    // text itself is stored without a trailing newline so replacement is easier;
    // the reconstruction step adds the final newline.
    s.push('\n');
    s
}

/// True if the text still contains conflict markers.
pub fn has_conflict_markers(text: &str) -> bool {
    text.lines().any(|l| {
        l.starts_with("<<<<<<< ")
            || l == "======="
            || l.starts_with(">>>>>>> ")
            || l.starts_with("||||||| ")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_single_zdiff3_hunk() {
        let content = "line 1\n<<<<<<< HEAD\nours\n||||||| base commit\nbase\n=======\ntheirs\n>>>>>>> feature\nline 2\n";
        let file = parse("f.txt".to_string(), content);
        assert_eq!(file.hunks.len(), 1);
        let h = &file.hunks[0];
        assert_eq!(h.start_line, 2);
        assert_eq!(h.end_line, 8);
        assert_eq!(h.ours, "ours\n");
        assert_eq!(h.base.as_deref(), Some("base\n"));
        assert_eq!(h.theirs, "theirs\n");
        assert_eq!(
            file.context_lines,
            vec!["line 1".to_string(), "line 2".to_string()]
        );
    }

    #[test]
    fn parse_multiple_hunks() {
        let content = "<<<<<<< HEAD\nours1\n||||||| base\nbase1\n=======\ntheirs1\n>>>>>>> a\n\n<<<<<<< HEAD\nours2\n||||||| base\nbase2\n=======\ntheirs2\n>>>>>>> b\n";
        let file = parse("f.txt".to_string(), content);
        assert_eq!(file.hunks.len(), 2);
        assert_eq!(file.hunks[0].ours, "ours1\n");
        assert_eq!(file.hunks[1].theirs, "theirs2\n");
    }

    #[test]
    fn parse_diff3_without_base() {
        let content = "<<<<<<< HEAD\nours\n=======\ntheirs\n>>>>>>> feature\n";
        let file = parse("f.txt".to_string(), content);
        assert_eq!(file.hunks.len(), 1);
        let h = &file.hunks[0];
        assert_eq!(h.ours, "ours\n");
        assert_eq!(h.base, None);
        assert_eq!(h.theirs, "theirs\n");
    }

    #[test]
    fn parse_no_conflicts() {
        let file = parse("f.txt".to_string(), "no conflicts here\n");
        assert!(file.hunks.is_empty());
        assert_eq!(file.context_lines, vec!["no conflicts here".to_string()]);
    }

    #[test]
    fn has_conflict_markers_detects_all_four() {
        assert!(has_conflict_markers("<<<<<<< HEAD\n"));
        assert!(has_conflict_markers("=======\n"));
        assert!(has_conflict_markers(">>>>>>> feature\n"));
        assert!(has_conflict_markers("||||||| base\n"));
        assert!(!has_conflict_markers("no conflicts"));
    }

    #[test]
    fn empty_sections_preserved() {
        let content = "<<<<<<< HEAD\n||||||| base\n=======\ntheirs\n>>>>>>> feature\n";
        let file = parse("f.txt".to_string(), content);
        let h = &file.hunks[0];
        assert_eq!(h.ours, "");
        assert_eq!(h.base, Some("".to_string()));
        assert_eq!(h.theirs, "theirs\n");
    }
}
