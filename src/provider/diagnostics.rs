//! gcm-authored prose for the provider failures that the transport's own
//! `Display` describes badly or not at all (CLO-798).
//!
//! This module is **binary-side**. `src/provider/facade.rs` re-exports the
//! provider types from the `gcm` library crate, so binary and library are two
//! separate crates and a `pub(crate)` helper in the library cannot be reached
//! from here. Making it `pub` there instead would enlarge the public surface for
//! a consumer that does not exist.
//!
//! Two failures are rewritten:
//!
//! * A **context-window rejection** arrives as `BadRequest`, whose `Display`
//!   ends "Likely an unsupported model/parameter or a gcm bug; please report it".
//!   That is the wrong advice for the most common 400 gcm produces, and the
//!   recovery depends on which of the four calls was in flight.
//! * A **timeout** arrives as a bare "API request timed out" naming neither the
//!   budget that expired nor the variable that changes it. The budget is known
//!   here and not in the error.

use gcm::provider::http;

use crate::diff::human_bytes;
use crate::provider::{ErrorKind, ProviderError};

/// Which of the four provider calls is in flight. Fixes both the status line's
/// operation label and the advice a context-window rejection gets: telling a
/// user to retry with `--all` on the path that already is `--all` sends them in
/// a circle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    /// `generate_plan` from `build_plan`.
    Grouping,
    /// `generate_message` regenerating a cached group's message.
    GroupMessage,
    /// `generate_message` on the `--all` / merge / tracer path.
    SingleMessage,
    /// `generate_message` reached after a grouping failure.
    FallbackMessage,
}

impl Operation {
    /// The label shared by the status line, the debug prompt-size line and the
    /// diagnostics.
    pub fn label(self) -> &'static str {
        match self {
            Operation::Grouping => "grouping",
            Operation::GroupMessage => "group message",
            Operation::SingleMessage => "single-commit message",
            Operation::FallbackMessage => "fallback message",
        }
    }

    /// What the user can actually do about an oversized prompt on this path.
    ///
    /// Staging is deliberately not suggested: `gather_for_grouping` builds its
    /// file set from every change in the worktree - staged, unstaged and
    /// untracked alike (`src/diff.rs:204`) - so staging a subset sends exactly
    /// the same bytes and the call fails again. Lowering the budget or removing
    /// paths from the set is what actually shrinks the request.
    fn oversize_advice(self) -> &'static str {
        match self {
            // The grouping prompt carries a file list, a status block and a
            // per-file-truncated diff; the single-message prompt carries only the
            // stat and the diff, so --all really is the smaller request.
            Operation::Grouping => {
                "Retry with --all, which sends a smaller prompt, or lower \
                 GCM_DIFF_TOTAL_BYTES to send less of the diff."
            }
            // Not --all: that would widen the request back to the whole worktree.
            Operation::GroupMessage => {
                "Lower GCM_DIFF_TOTAL_BYTES to send less of the diff, or exclude \
                 generated paths with .gcmignore."
            }
            Operation::SingleMessage | Operation::FallbackMessage => {
                "Everything is already in one message, so lower GCM_DIFF_TOTAL_BYTES \
                 to send less of the diff, exclude generated paths with .gcmignore, \
                 or commit the unrelated work separately first."
            }
        }
    }
}

/// What the call was carrying, for the failure prose.
#[derive(Debug, Clone, Copy)]
pub struct CallShape {
    pub operation: Operation,
    pub files: usize,
    pub prompt_bytes: usize,
}

/// A gcm-authored replacement for `err`'s own prose, or `None` when the
/// provider's message is already the best one available.
///
/// The caller uses the returned string as the user-facing message. Neither the
/// envelope shape nor `error.code`/`fallback.raw_code` is affected: those are
/// derived from the untouched `ErrorKind`.
pub fn describe(shape: CallShape, err: &ProviderError) -> Option<String> {
    if let Some(detail) = context_window_detail(&err.kind) {
        return Some(context_window_message(shape, err.provider, detail));
    }
    if is_timeout(&err.kind) {
        return Some(timeout_message(shape, err.provider, http::timeout_secs()));
    }
    None
}

/// The provider's own detail text when the transport tagged this 400 as a
/// context-window rejection (`http::CONTEXT_WINDOW_MARKER`, prepended before the
/// 200-char truncation so it survives it).
fn context_window_detail(kind: &ErrorKind) -> Option<&str> {
    match kind {
        ErrorKind::BadRequest { detail: Some(d) } => {
            d.strip_prefix(http::CONTEXT_WINDOW_MARKER).map(str::trim)
        }
        _ => None,
    }
}

/// Both timeout phases (AC-4): no response headers within the budget, which the
/// transport classifies as `Timeout`, and a stall while reading the body of a
/// 2xx, which it classifies as `Transport` with "timeout" in the text.
fn is_timeout(kind: &ErrorKind) -> bool {
    match kind {
        ErrorKind::Timeout => true,
        ErrorKind::Transport(msg) => msg.to_lowercase().contains("timeout"),
        _ => false,
    }
}

/// Never says "a gcm bug; please report it": an oversized diff is the user's
/// input, and there is a real recovery for it.
///
/// Worded "too large to accept" rather than "exceeds the context window". The
/// transport's detector also matches per-string length limits and gateway
/// payload-size rejections, which are not the context window but do have the
/// same recovery; claiming the context window specifically would be wrong for
/// those. Output-budget rejections, where the recovery would differ, never reach
/// here - `http::is_context_window_body` vetoes them.
fn context_window_message(shape: CallShape, provider: &str, detail: &str) -> String {
    let mut msg = format!(
        "{provider} rejected the {} request: the prompt is too large for this model \
         to accept ({} file(s), ~{}). {}",
        shape.operation.label(),
        shape.files,
        human_bytes(shape.prompt_bytes),
        shape.operation.oversize_advice()
    );
    if !detail.is_empty() {
        msg.push_str(&format!(" ({provider} said: {detail})"));
    }
    msg
}

/// Names the budget that actually applied, not a constant the reader has to go
/// look up, plus the variable that changes it.
fn timeout_message(shape: CallShape, provider: &str, budget_secs: u64) -> String {
    format!(
        "{provider} did not answer the {} request within {budget_secs}s. Raise the \
         budget with GCM_HTTP_TIMEOUT_SECS=<seconds>, or switch to a faster model.",
        shape.operation.label()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shape(operation: Operation) -> CallShape {
        CallShape {
            operation,
            files: 12,
            prompt_bytes: 49_152,
        }
    }

    fn bad_request(detail: &str) -> ProviderError {
        ProviderError::new(
            "Groq",
            ErrorKind::BadRequest {
                detail: Some(detail.to_string()),
            },
        )
    }

    /// CLO-798 AC-5: the context-window message is gcm's own, names the cause and
    /// the size, and never blames gcm for the user's oversized diff.
    #[test]
    fn context_window_message_is_gcm_authored() {
        let marked = format!("{}context_length_exceeded", http::CONTEXT_WINDOW_MARKER);
        let msg = describe(shape(Operation::Grouping), &bad_request(&marked)).unwrap();
        assert!(msg.contains("too large for this model"), "{msg}");
        assert!(msg.contains("12 file(s)"), "{msg}");
        assert!(msg.contains("~48 KB"), "{msg}");
        assert!(msg.contains("context_length_exceeded"), "{msg}");
        assert!(!msg.contains("gcm bug"), "{msg}");
        assert!(!msg.contains("please report it"), "{msg}");
    }

    /// CLO-798 AC-5: the advice differs per call site. Recommending `--all` on a
    /// path that already sends everything offers no recovery.
    #[test]
    fn advice_is_operation_specific() {
        let marked = format!("{}too many tokens", http::CONTEXT_WINDOW_MARKER);
        let grouping = describe(shape(Operation::Grouping), &bad_request(&marked)).unwrap();
        assert!(grouping.contains("--all"), "{grouping}");

        for op in [
            Operation::SingleMessage,
            Operation::FallbackMessage,
            Operation::GroupMessage,
        ] {
            let msg = describe(shape(op), &bad_request(&marked)).unwrap();
            assert!(!msg.contains("--all"), "{op:?} still suggests --all: {msg}");
            assert!(msg.contains(op.label()), "{msg}");
        }
        // The advice has to name something that actually shrinks the request.
        // Staging never does: the prompt is built from the whole worktree
        // (`diff::gather_for_grouping`), so a staged subset sends the same bytes.
        for op in [
            Operation::Grouping,
            Operation::GroupMessage,
            Operation::SingleMessage,
            Operation::FallbackMessage,
        ] {
            let msg = describe(shape(op), &bad_request(&marked)).unwrap();
            assert!(
                !msg.to_lowercase().contains("stage"),
                "{op:?} suggests staging, which does not narrow the prompt: {msg}"
            );
            assert!(msg.contains("GCM_DIFF_TOTAL_BYTES"), "{msg}");
            assert!(
                !msg.contains("raise GCM_DIFF_TOTAL_BYTES"),
                "raising the budget makes an oversized prompt larger: {msg}"
            );
        }
    }

    /// An ordinary 400 keeps the provider's own message: gcm has nothing better
    /// to say about a genuinely malformed request.
    #[test]
    fn plain_bad_request_is_left_alone() {
        assert!(describe(
            shape(Operation::Grouping),
            &bad_request("unsupported parameter")
        )
        .is_none());
        assert!(describe(
            shape(Operation::Grouping),
            &ProviderError::new("Groq", ErrorKind::EmptyResponse)
        )
        .is_none());
    }

    /// CLO-798 AC-4: both timeout phases produce the budget-naming message. A
    /// body stall after 2xx headers is classified `Transport`, not `Timeout`.
    #[test]
    fn both_timeout_phases_name_the_budget() {
        let before_headers = ProviderError::new("Groq", ErrorKind::Timeout);
        let mid_body = ProviderError::new(
            "Groq",
            ErrorKind::Transport("timeout reading response body".to_string()),
        );
        for err in [before_headers, mid_body] {
            let msg = describe(shape(Operation::Grouping), &err).unwrap();
            assert!(msg.contains("GCM_HTTP_TIMEOUT_SECS"), "{msg}");
            assert!(msg.contains("grouping"), "{msg}");
        }
        // A transport failure that is not a timeout keeps its own message.
        let refused = ProviderError::new(
            "Groq",
            ErrorKind::Transport("connection refused".to_string()),
        );
        assert!(describe(shape(Operation::Grouping), &refused).is_none());
    }

    #[test]
    fn timeout_message_names_the_budget_it_is_given() {
        assert!(timeout_message(shape(Operation::Grouping), "Groq", 1).contains("within 1s"));
        assert!(timeout_message(shape(Operation::Grouping), "Groq", 60).contains("within 60s"));
    }
}
