//! Build the provider `ResolveContext` for `gcm resolve` (CLO-531, ST8).
//!
//! This module is the bridge between the parsed conflict file and the provider
//! trait: it extracts style context (indentation, nearby symbols) and maps the
//! internal `Hunk` shape to the provider-local `ConflictHunk`.

use crate::provider::{ConflictHunk, ResolveContext};

use super::markers::{ConflictFile, Hunk};

/// Build a `ResolveContext` for a single conflicted file. Sends only the hard
/// hunks to the provider (trivial hunks are resolved deterministically upstream).
#[allow(dead_code)]
pub fn build_resolve_context(
    path: String,
    file: &ConflictFile,
    hard_hunk_indices: &[usize],
) -> ResolveContext {
    let style_context = extract_style_context(file);
    let hunks: Vec<ConflictHunk> = hard_hunk_indices
        .iter()
        .map(|i| to_provider_hunk(&file.hunks[*i]))
        .collect();
    ResolveContext {
        path,
        hunks,
        style_context,
        temperature: 0.1,
    }
}

#[allow(dead_code)]
fn to_provider_hunk(hunk: &Hunk) -> ConflictHunk {
    ConflictHunk {
        base: hunk.base.clone(),
        ours: hunk.ours.clone(),
        theirs: hunk.theirs.clone(),
    }
}

/// Extract a short style context: non-conflict lines plus a note on dominant
/// indentation so the provider preserves formatting.
pub(crate) fn extract_style_context(file: &ConflictFile) -> String {
    let mut ctx = String::new();
    if !file.context_lines.is_empty() {
        let joined = file.context_lines.join("\n");
        ctx.push_str("File context:\n");
        ctx.push_str(&joined);
        ctx.push('\n');
    }
    if let Some(indent) = dominant_indent(
        &file
            .hunks
            .iter()
            .map(|h| h.ours.clone())
            .collect::<Vec<_>>(),
    ) {
        ctx.push_str(&format!(
            "\nDominant indentation: {} spaces per level\n",
            indent
        ));
    }
    ctx
}

fn dominant_indent(texts: &[String]) -> Option<usize> {
    let mut counts: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    for text in texts {
        for line in text.lines() {
            let spaces = line.chars().take_while(|c| *c == ' ').count();
            if spaces > 0 {
                *counts.entry(spaces).or_insert(0) += 1;
            }
        }
    }
    counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(indent, _)| indent)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file() -> ConflictFile {
        ConflictFile {
            path: "src/lib.rs".to_string(),
            hunks: vec![Hunk {
                start_line: 3,
                end_line: 8,
                base: Some("base\n".to_string()),
                ours: "    ours\n".to_string(),
                theirs: "    theirs\n".to_string(),
            }],
            context_lines: vec!["fn main() {".to_string(), "    // setup".to_string()],
        }
    }

    #[test]
    fn build_resolve_context_maps_hard_hunks() {
        let ctx = build_resolve_context("src/lib.rs".to_string(), &file(), &[0]);
        assert_eq!(ctx.path, "src/lib.rs");
        assert_eq!(ctx.hunks.len(), 1);
        assert_eq!(ctx.hunks[0].ours, "    ours\n");
        assert_eq!(ctx.hunks[0].base, Some("base\n".to_string()));
    }

    #[test]
    fn style_context_includes_dominant_indent() {
        let ctx = build_resolve_context("src/lib.rs".to_string(), &file(), &[0]);
        assert!(ctx.style_context.contains("Dominant indentation: 4 spaces"));
    }

    #[test]
    fn dominant_indent_detects_four_spaces() {
        assert_eq!(dominant_indent(&["    a\n    b\n".to_string()]), Some(4));
    }

    #[test]
    fn dominant_indent_none_when_no_leading_spaces() {
        assert_eq!(dominant_indent(&["a\nb\n".to_string()]), None);
    }
}
