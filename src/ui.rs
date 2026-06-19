use std::io::{IsTerminal, Write};
use std::process::{Command, Stdio};

use crate::error::GcmError;

/// Result of the confirmation step.
pub enum Decision {
    /// Commit with this (possibly edited) message.
    Commit(String),
    /// User declined; abort cleanly (exit 0, FR-9).
    Abort,
}

/// Render the message and ask the user to confirm (FR-5). With `auto_yes` the
/// prompt is skipped and the message is accepted as-is. Assumes the caller has
/// already enforced the non-TTY guard for the prompting path.
pub fn confirm(message: &str, auto_yes: bool) -> Result<Decision, GcmError> {
    print_message(message);

    if auto_yes {
        return Ok(Decision::Commit(message.to_string()));
    }

    print!("Commit with this message? [Y/n/e(dit)] ");
    std::io::stdout().flush().ok();

    let mut response = String::new();
    if std::io::stdin().read_line(&mut response).is_err() {
        return Err(GcmError::NonInteractive);
    }

    match response.trim() {
        "n" | "N" => Ok(Decision::Abort),
        "e" | "E" => {
            let edited = edit_in_editor(message)?;
            let edited = edited.trim().to_string();
            if edited.is_empty() {
                Err(GcmError::EmptyMessage)
            } else {
                Ok(Decision::Commit(edited))
            }
        }
        _ => Ok(Decision::Commit(message.to_string())),
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
fn edit_in_editor(message: &str) -> Result<String, GcmError> {
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

    let status = Command::new(&editor)
        .arg(tmp.path())
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|e| GcmError::Editor(format!("could not launch '{editor}': {e}")))?;
    if !status.success() {
        return Err(GcmError::Editor(format!("'{editor}' exited with an error")));
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
