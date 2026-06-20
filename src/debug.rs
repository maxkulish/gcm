//! Minimal debug logging gated by the `GCM_DEBUG` env var.
//!
//! This is deliberately NOT a logging framework (no levels, targets, or
//! structured output) - that is CLO-493 (FR-38). It exists so the typed provider
//! errors and retry attempts (CLO-488) are visible when diagnosing: the task's
//! acceptance criterion is "the error type is visible in debug logs."

/// Whether debug logging is on: `GCM_DEBUG` set to anything other than empty or
/// `0`.
pub fn enabled() -> bool {
    debug_flag(std::env::var("GCM_DEBUG").ok().as_deref())
}

/// Pure predicate behind [`enabled`] - testable without mutating the process env.
fn debug_flag(value: Option<&str>) -> bool {
    matches!(value, Some(v) if !v.is_empty() && v != "0")
}

/// Print a `gcm: [debug] ...` line to stderr when [`enabled`].
pub fn log(msg: &str) {
    if enabled() {
        eprintln!("gcm: [debug] {msg}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flag_on_for_nonempty_nonzero() {
        assert!(debug_flag(Some("1")));
        assert!(debug_flag(Some("true")));
        assert!(debug_flag(Some("yes")));
    }

    #[test]
    fn flag_off_for_unset_empty_or_zero() {
        assert!(!debug_flag(None));
        assert!(!debug_flag(Some("")));
        assert!(!debug_flag(Some("0")));
    }
}
