//! Minimal logging for gcm (CLO-493 / FR-38).
//!
//! `GCM_LOG_LEVEL` governs the active level (`off|error|warn|info|debug|trace`).
//! If `GCM_LOG_LEVEL` is unset, the legacy `GCM_DEBUG` flag provides backward
//! compatibility: any non-empty, non-`0` value enables debug-level output.
//! All log lines go to stderr.
//!
//! The default level is `Warn` (CLO-798): retry notices and other operational
//! warnings have to reach the user without opting in, or a stalled run stays as
//! silent as it was before. `GCM_LOG_LEVEL=off` is the opt-out.
//!
//! This module is compiled **once**, into the library, and the binary reaches it
//! through `gcm::debug` (`src/main.rs`). That matters for [`progress`]: its state
//! is a process-wide static, and a second copy of this file would give the
//! binary's progress ticker and the library's log calls two different statics.

use std::str::FromStr;

/// Available log levels, ordered from least to most verbose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Off = 0,
    Error = 1,
    Warn = 2,
    Info = 3,
    Debug = 4,
    Trace = 5,
}

impl FromStr for Level {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "off" => Ok(Level::Off),
            "error" => Ok(Level::Error),
            "warn" => Ok(Level::Warn),
            "info" => Ok(Level::Info),
            "debug" => Ok(Level::Debug),
            "trace" => Ok(Level::Trace),
            other => Err(format!("unknown log level '{other}'")),
        }
    }
}

/// The level in force when nothing selects one (CLO-798). Warn, not Off: the
/// retry notice at `provider::http::retry_with` is the only signal a user gets
/// during a multi-minute retry storm, and an opt-in signal is no signal.
pub const DEFAULT_LEVEL: Level = Level::Warn;

/// The effective log level: `GCM_LOG_LEVEL` wins over the legacy `GCM_DEBUG`
/// flag. Default is [`DEFAULT_LEVEL`]. An unparseable `GCM_LOG_LEVEL` falls back
/// to the default rather than to `Off`, so a typo cannot silence warnings.
pub fn log_level() -> Level {
    resolve_level(
        std::env::var("GCM_LOG_LEVEL").ok().as_deref(),
        std::env::var("GCM_DEBUG").ok().as_deref(),
    )
}

/// Pure core of [`log_level`], so the precedence table is unit-testable without
/// mutating process environment from a parallel test.
fn resolve_level(log_level_var: Option<&str>, debug_var: Option<&str>) -> Level {
    if let Some(v) = log_level_var {
        if !v.trim().is_empty() {
            return v.parse().unwrap_or(DEFAULT_LEVEL);
        }
    }
    if debug_flag(debug_var) {
        return Level::Debug;
    }
    DEFAULT_LEVEL
}

/// Pure predicate behind the legacy `GCM_DEBUG` behaviour.
fn debug_flag(value: Option<&str>) -> bool {
    matches!(value, Some(v) if !v.is_empty() && v != "0")
}

/// Whether a message at `level` would be emitted right now.
pub fn enabled(level: Level) -> bool {
    log_level() >= level
}

/// Emit a single log line to stderr if `level` is enabled. Routed through
/// [`progress::emit_line`] so a line never lands on top of a live progress
/// ticker (CLO-798).
#[macro_export]
macro_rules! log {
    ($level:expr, $($arg:tt)*) => {
        if $crate::debug::enabled($level) {
            $crate::debug::progress::emit_line(&format!(
                "gcm: [{}] {}",
                $level.as_str(),
                format_args!($($arg)*)
            ));
        }
    };
}

/// Convenience macro for debug-level messages. Backwards-compatible with the
/// pre-CLO-493 `GCM_DEBUG=1` callers used in CLO-488.
#[macro_export]
macro_rules! debug_log {
    ($($arg:tt)*) => {
        $crate::log!($crate::debug::Level::Debug, $($arg)*)
    };
}

/// Convenience macro for warn-level messages (CLO-798). Visible by default; see
/// [`DEFAULT_LEVEL`].
#[macro_export]
macro_rules! warn_log {
    ($($arg:tt)*) => {
        $crate::log!($crate::debug::Level::Warn, $($arg)*)
    };
}

/// Coordination between the in-flight progress ticker (`ui::CallProgress`, in
/// the binary) and every other writer to stderr (CLO-798).
///
/// The ticker redraws one line in place with a leading `\r`. Any other write
/// that arrives mid-call would otherwise land on top of that partially drawn
/// line. Writers therefore go through [`emit_line`], which erases a live ticker
/// line first and then writes its own message whole.
///
/// The live-line width is a single process-wide static. This module must have
/// exactly one instance in the process, which is why `src/debug.rs` is compiled
/// into the library only and the binary reaches it as `gcm::debug::progress`.
pub mod progress {
    use std::io::{IsTerminal, Write};
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Column width of the ticker line currently drawn on stderr; 0 when none is
    /// live. Only meaningful on a TTY - the non-TTY ticker writes whole lines and
    /// so never needs erasing.
    static LIVE_WIDTH: AtomicUsize = AtomicUsize::new(0);

    /// Record that a ticker line of `width` columns is drawn on stderr.
    pub fn set_live(width: usize) {
        LIVE_WIDTH.store(width, Ordering::SeqCst);
    }

    /// Record that no ticker line is drawn.
    pub fn clear_live() {
        LIVE_WIDTH.store(0, Ordering::SeqCst);
    }

    /// The width currently registered, 0 when nothing is live.
    pub fn live_width() -> usize {
        LIVE_WIDTH.load(Ordering::SeqCst)
    }

    /// Pure: the exact bytes that put `msg` on its own clean line.
    ///
    /// With nothing live, or off a TTY, that is just the message and a newline -
    /// which is why a redirected stderr never receives a `\r` or an escape
    /// sequence (CLO-798 AC-8).
    ///
    /// With a ticker line drawn on a TTY, the cursor returns to column 0 and the
    /// rest of the line is erased with `ESC[K`. Blanking with `live_width` spaces
    /// instead would look equivalent but corrupts on a terminal resized narrower
    /// mid-call, since the recorded width no longer matches the real line.
    /// `live_width` is therefore only consulted as "is a line drawn".
    pub fn line_with_clear(msg: &str, live_width: usize, tty: bool) -> String {
        if live_width == 0 || !tty {
            return format!("{msg}\n");
        }
        format!("\r\x1b[K{msg}\n")
    }

    /// Write `msg` to stderr as a whole line, erasing a live ticker line first.
    /// The ticker is marked not-live: its next tick redraws below the message.
    pub fn emit_line(msg: &str) {
        let mut err = std::io::stderr();
        let out = line_with_clear(msg, live_width(), err.is_terminal());
        let _ = err.write_all(out.as_bytes());
        let _ = err.flush();
        clear_live();
    }
}

impl Level {
    /// Lower-case label used in the log prefix.
    pub fn as_str(self) -> &'static str {
        match self {
            Level::Off => "off",
            Level::Error => "error",
            Level::Warn => "warn",
            Level::Info => "info",
            Level::Debug => "debug",
            Level::Trace => "trace",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_parsing() {
        assert_eq!("off".parse::<Level>().unwrap(), Level::Off);
        assert_eq!("WARN".parse::<Level>().unwrap(), Level::Warn);
        assert_eq!("  debug ".parse::<Level>().unwrap(), Level::Debug);
        assert!("bogus".parse::<Level>().is_err());
    }

    #[test]
    fn ordering() {
        assert!(Level::Debug > Level::Warn);
        assert!(Level::Off < Level::Error);
    }

    #[test]
    fn legacy_debug_flag_unchanged() {
        assert!(debug_flag(Some("1")));
        assert!(debug_flag(Some("true")));
        assert!(debug_flag(Some("yes")));
        assert!(!debug_flag(None));
        assert!(!debug_flag(Some("")));
        assert!(!debug_flag(Some("0")));
    }

    /// CLO-798 AC-3: warn-level output has to be on when nothing selects a level,
    /// and a typo must not silence it. `off`/`error` remain the way to opt out.
    #[test]
    fn log_level_defaults_to_warn() {
        let warn_enabled =
            |lvl: Option<&str>, dbg: Option<&str>| resolve_level(lvl, dbg) >= Level::Warn;
        assert!(warn_enabled(None, None), "unset must emit warnings");
        assert!(!warn_enabled(Some("off"), None), "off opts out");
        assert!(!warn_enabled(Some("error"), None), "error opts out");
        assert!(warn_enabled(Some("warn"), None));
        assert!(
            warn_enabled(Some("bogus"), None),
            "a typo falls back to warn"
        );
        assert!(warn_enabled(Some("  "), None), "blank is treated as unset");
        assert_eq!(resolve_level(None, Some("1")), Level::Debug);
        assert_eq!(resolve_level(None, Some("0")), DEFAULT_LEVEL);
        // GCM_LOG_LEVEL still wins over the legacy flag.
        assert_eq!(resolve_level(Some("off"), Some("1")), Level::Off);
    }

    /// CLO-798 AC-10: a log line written while the ticker is live erases the
    /// ticker line first, then writes its own message whole.
    #[test]
    fn emit_line_clears_progress_first() {
        // Nothing live: a plain line, no control characters at all.
        assert_eq!(
            progress::line_with_clear("gcm: [warn] retrying", 0, true),
            "gcm: [warn] retrying\n"
        );
        // Live ticker on a TTY: return to column 0, erase the rest of the line,
        // then the message. Erasing rather than space-padding keeps this correct
        // when the terminal was resized narrower during the call.
        assert_eq!(
            progress::line_with_clear("gcm: [warn] retrying", 4, true),
            "\r\x1b[Kgcm: [warn] retrying\n"
        );
        // The recorded width is only a liveness flag: a different width produces
        // the same bytes, so a stale width cannot corrupt the line.
        assert_eq!(
            progress::line_with_clear("gcm: [warn] retrying", 120, true),
            progress::line_with_clear("gcm: [warn] retrying", 4, true)
        );
        // Not a TTY: never emit `\r` or an escape, even with a width registered
        // (AC-8).
        for width in [0, 4, 120] {
            let out = progress::line_with_clear("gcm: [warn] retrying", width, false);
            assert_eq!(out, "gcm: [warn] retrying\n");
            assert!(!out.contains('\x1b'), "escape leaked off-TTY: {out:?}");
            assert!(!out.contains('\r'), "CR leaked off-TTY: {out:?}");
        }
        // Nothing live means a plain line even on a TTY.
        assert!(!progress::line_with_clear("m", 0, true).contains('\x1b'));
    }

    #[test]
    fn progress_width_registration_roundtrips() {
        progress::clear_live();
        assert_eq!(progress::live_width(), 0);
        progress::set_live(12);
        assert_eq!(progress::live_width(), 12);
        progress::clear_live();
        assert_eq!(progress::live_width(), 0);
    }
}
