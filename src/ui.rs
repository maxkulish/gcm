use std::io::{BufRead, IsTerminal, Write};
use std::process::{Command, Stdio};

use crate::error::GcmError;

/// A parsed answer to a `[y/N/e(dit)]` confirmation prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptChoice {
    Yes,
    No,
    Edit,
}

/// Parse one line of prompt input. Case-insensitive `y`/`yes`, `n`/`no`,
/// `e`/`edit`; bare Enter is the default No. Anything else returns `None` so
/// the caller can reprompt. Accepting is only ever explicit.
fn parse_choice(input: &str) -> Option<PromptChoice> {
    match input.trim().to_ascii_lowercase().as_str() {
        "y" | "yes" => Some(PromptChoice::Yes),
        "n" | "no" | "" => Some(PromptChoice::No),
        "e" | "edit" => Some(PromptChoice::Edit),
        _ => None,
    }
}

/// Bounded reprompt budget: after this many unrecognized answers the prompt
/// resolves to No rather than looping forever on garbage input.
const PROMPT_ATTEMPTS: usize = 3;

/// Ask `prompt` on stderr and read the answer from `reader`. No is the
/// default (bare Enter) and the EOF answer, so closed, exhausted, or Ctrl-D
/// input can never accept; unrecognized input reprompts up to
/// [`PROMPT_ATTEMPTS`] times and then resolves to No.
fn prompt_choice_from(reader: &mut impl BufRead, prompt: &str) -> Result<PromptChoice, GcmError> {
    for _ in 0..PROMPT_ATTEMPTS {
        eprint!("{prompt}");
        std::io::stderr().flush().ok();
        let mut response = String::new();
        let read = reader
            .read_line(&mut response)
            .map_err(|_| GcmError::NonInteractive)?;
        if read == 0 {
            return Ok(PromptChoice::No);
        }
        match parse_choice(&response) {
            Some(choice) => return Ok(choice),
            None => eprintln!("Please answer y(es), n(o), or e(dit)."),
        }
    }
    Ok(PromptChoice::No)
}

/// Prompt on the real stdin.
fn prompt_choice(prompt: &str) -> Result<PromptChoice, GcmError> {
    let stdin = std::io::stdin();
    let mut lock = stdin.lock();
    prompt_choice_from(&mut lock, prompt)
}

/// Result of the confirmation step.
pub enum Decision {
    /// Commit with this (possibly edited) message.
    Commit(String),
    /// User declined; abort cleanly (exit 0, FR-9).
    Abort,
}

/// Result of the per-file confirmation step for `gcm resolve`.
pub enum FileDecision {
    /// Write the resolved file to the working tree.
    Accept,
    /// Leave the file conflicted and continue to the next file.
    Skip,
    /// Open the resolved content in $EDITOR, then write the edited version.
    Edit,
}

/// Render the resolved file and ask whether to keep it. In `json_mode` the
/// preview renders on stderr so stdout stays a pure JSON envelope - the user
/// still sees what they are confirming, since Yes now carries commit
/// authority (never confirm blind).
pub fn confirm_file(
    path: &str,
    resolved_text: &str,
    json_mode: bool,
) -> Result<FileDecision, GcmError> {
    if json_mode {
        let mut err = std::io::stderr();
        write_file_preview(&mut err, path, resolved_text).ok();
    } else {
        let mut out = std::io::stdout();
        write_file_preview(&mut out, path, resolved_text).ok();
    }

    match prompt_choice(&format!("Keep resolution for {path}? [y/N/e(dit)] "))? {
        PromptChoice::Yes => Ok(FileDecision::Accept),
        PromptChoice::No => Ok(FileDecision::Skip),
        PromptChoice::Edit => Ok(FileDecision::Edit),
    }
}

fn write_file_preview(w: &mut impl Write, path: &str, resolved_text: &str) -> std::io::Result<()> {
    writeln!(w)?;
    writeln!(w, "Resolved {path} (preview):")?;
    writeln!(w, "-----------------------------")?;
    let lines: Vec<&str> = resolved_text.lines().collect();
    let max_preview = 40;
    for line in lines.iter().take(max_preview) {
        writeln!(w, "{line}")?;
    }
    if lines.len() > max_preview {
        writeln!(w, "... {} more lines", lines.len() - max_preview)?;
    }
    writeln!(w, "-----------------------------")?;
    writeln!(w)
}

/// Render the message and ask the user to confirm (FR-5). With `auto_yes` the
/// prompt is skipped and the message is accepted as-is. With `quiet` the
/// message preview is not printed to stdout (used in `--json` mode where stdout
/// must contain only the machine envelope). Assumes the caller has already
/// enforced the non-TTY guard for the prompting path.
pub fn confirm(message: &str, auto_yes: bool, quiet: bool) -> Result<Decision, GcmError> {
    if !quiet {
        print_message(message);
    }

    if auto_yes {
        return Ok(Decision::Commit(message.to_string()));
    }

    match prompt_choice("Commit with this message? [y/N/e(dit)] ")? {
        PromptChoice::Yes => Ok(Decision::Commit(message.to_string())),
        PromptChoice::No => Ok(Decision::Abort),
        PromptChoice::Edit => {
            let edited = edit_in_editor(message)?;
            let edited = edited.trim().to_string();
            if edited.is_empty() {
                Err(GcmError::EmptyMessage)
            } else {
                Ok(Decision::Commit(edited))
            }
        }
    }
}

fn print_message(message: &str) {
    println!();
    println!("Commit message:");
    println!("-----------------------------");
    println!("{message}");
    println!("-----------------------------");
    println!();
}

/// Open `$EDITOR` (default `vim`) on the message via a temp file, inheriting the
/// terminal so the editor is usable; read the edited text back. The temp file is
/// removed on every exit path (tempfile crate / Drop, AC-7).
pub(crate) fn edit_in_editor(message: &str) -> Result<String, GcmError> {
    let editor = std::env::var("EDITOR")
        .ok()
        .filter(|e| !e.trim().is_empty())
        .unwrap_or_else(|| "vim".to_string());

    let mut tmp = tempfile::Builder::new()
        .prefix("gcm-commit-")
        .suffix(".txt")
        .tempfile()
        .map_err(|e| GcmError::Editor(format!("could not create temp file: {e}")))?;
    tmp.write_all(message.as_bytes())
        .map_err(|e| GcmError::Editor(format!("could not write temp file: {e}")))?;
    tmp.flush().ok();

    // Launch through the shell, exactly as git does for core.editor, so the
    // $EDITOR string is parsed by the user's shell - handling arguments
    // (`code --wait`), quotes, and space-containing executable paths (a macOS
    // app bundle). The file path is passed as a separate argv ($1) and
    // referenced as "$1", so it is never word-split or re-expanded. (sh is
    // always present on the supported platforms, macOS + Linux.)
    let status = Command::new("sh")
        .arg("-c")
        .arg(format!("{editor} \"$1\""))
        .arg("gcm")
        .arg(tmp.path())
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|e| GcmError::Editor(format!("could not launch editor '{editor}': {e}")))?;
    if !status.success() {
        return Err(GcmError::Editor(format!(
            "editor '{editor}' exited with an error"
        )));
    }

    std::fs::read_to_string(tmp.path())
        .map_err(|e| GcmError::Editor(format!("could not read edited message: {e}")))
    // `tmp` drops here, deleting the file on success or via `?` early-return.
}

/// Non-TTY guard (ADR-001 #10, AC-11): true when we would need to prompt but
/// cannot, so the caller can error instead of hanging on a closed stdin.
pub fn needs_terminal_but_absent(auto_yes: bool, dry_run: bool) -> bool {
    !auto_yes && !dry_run && !std::io::stdin().is_terminal()
}

/// FR-46 warning text shown before a pre-existing curated index is reset. Pure
/// (returns the string) so the wording is unit-testable; the caller prints it to
/// stderr. gcm groups whole files, so it resets the index and re-stages by group;
/// partial (hunk-level) `git add -p` staging is not preserved in v1, so the hunks
/// the user excluded would be committed. Consistent with the static `--help`
/// disclosure (`EGRESS_DISCLOSURE`, `src/cli.rs`).
pub fn curated_index_warning(staged: usize, partial: usize) -> String {
    // Always name both counts (FR-46): the staged total and how many of those are
    // partially staged (the data-loss case). A `0 partially` makes explicit that
    // no hunk-level work is at risk while still flagging that the curated index
    // (which files, what grouping) is overridden.
    format!(
        "gcm: warning: {staged} file(s) already staged ({partial} partially via `git add -p`) - gcm will reset the curated index and re-stage by group.\n\
         gcm: warning: hunk-level staging is not preserved in v1; excluded hunks would be committed."
    )
}

/// `--dry-run` preview of the grouping plan: group 1's message plus a note of
/// how many files remain in later groups (committed on subsequent runs). Stages
/// and commits nothing (CLO-487 AC-8).
pub fn preview_plan(message: &str, group_count: usize, remaining: usize) {
    println!();
    println!("Group 1 commit message (dry run - nothing staged or committed):");
    println!("-----------------------------");
    println!("{message}");
    println!("-----------------------------");
    if group_count > 1 {
        println!(
            "{remaining} file(s) remain in {} more group(s); run gcm again to commit the next group.",
            group_count - 1
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn choose(input: &str) -> PromptChoice {
        prompt_choice_from(&mut Cursor::new(input.as_bytes()), "test? [y/N/e(dit)] ").unwrap()
    }

    #[test]
    fn parse_choice_accepts_only_explicit_yes() {
        for s in ["y", "Y", "yes", "YES", "Yes", " y ", "yes\n"] {
            assert_eq!(parse_choice(s), Some(PromptChoice::Yes), "input {s:?}");
        }
    }

    #[test]
    fn parse_choice_rejects_no_variants_and_empty() {
        for s in ["n", "N", "no", "NO", "No", "", "  ", "\n"] {
            assert_eq!(parse_choice(s), Some(PromptChoice::No), "input {s:?}");
        }
    }

    #[test]
    fn parse_choice_edit_variants() {
        for s in ["e", "E", "edit", "EDIT", "Edit"] {
            assert_eq!(parse_choice(s), Some(PromptChoice::Edit), "input {s:?}");
        }
    }

    #[test]
    fn parse_choice_unknown_input_is_none() {
        for s in ["ok", "q", "yess", "nope!", "ye s", "commit"] {
            assert_eq!(parse_choice(s), None, "input {s:?} must reprompt, not act");
        }
    }

    #[test]
    fn prompt_no_and_empty_reject() {
        assert_eq!(choose("no\n"), PromptChoice::No);
        assert_eq!(choose("No\n"), PromptChoice::No);
        assert_eq!(
            choose("\n"),
            PromptChoice::No,
            "bare Enter is the default No"
        );
    }

    #[test]
    fn prompt_eof_rejects() {
        assert_eq!(
            choose(""),
            PromptChoice::No,
            "EOF (Ctrl-D) can never accept"
        );
    }

    #[test]
    fn prompt_explicit_yes_and_edit() {
        assert_eq!(choose("yes\n"), PromptChoice::Yes);
        assert_eq!(choose("e\n"), PromptChoice::Edit);
    }

    #[test]
    fn prompt_unknown_input_reprompts_then_rejects() {
        // Three garbage answers exhaust the attempt budget -> No.
        assert_eq!(choose("q\nq\nq\n"), PromptChoice::No);
        // A recognized answer after garbage is honored.
        assert_eq!(choose("q\ny\n"), PromptChoice::Yes);
        assert_eq!(choose("what\nok\nn\n"), PromptChoice::No);
        // Garbage then EOF -> No.
        assert_eq!(choose("q\n"), PromptChoice::No);
    }

    #[test]
    fn curated_index_warning_has_required_substrings() {
        let w = curated_index_warning(3, 1);
        // AC-7 testable substrings.
        assert!(w.contains("curated index"), "names the curated index: {w}");
        assert!(w.contains("reset"), "says it will reset: {w}");
        assert!(
            w.contains("hunk-level staging is not preserved"),
            "states the v1 limitation: {w}"
        );
        assert!(
            w.contains("3 file(s) already staged"),
            "names the staged count: {w}"
        );
        // Specific substring, not a bare `contains('1')` - the text always carries
        // "v1", so a digit check would pass regardless of the partial count.
        assert!(
            w.contains("(1 partially"),
            "names the partial count specifically: {w}"
        );
    }

    #[test]
    fn curated_index_warning_always_names_both_counts() {
        let w = curated_index_warning(2, 0);
        assert!(w.contains("curated index"));
        assert!(w.contains("hunk-level staging is not preserved"));
        assert!(
            w.contains("2 file(s) already staged"),
            "names staged count: {w}"
        );
        assert!(
            w.contains("0 partially"),
            "names the partial count even when zero: {w}"
        );
    }
}
