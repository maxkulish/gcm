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
/// line. Writers therefore go through [`progress::emit_line`], which erases a live ticker
/// line first and then writes its own message whole.
///
/// The live-line width is a single process-wide static. This module must have
/// exactly one instance in the process, which is why `src/debug.rs` is compiled
/// into the library only and the binary reaches it as `gcm::debug::progress`.
pub mod progress {
    use std::io::{IsTerminal, Write};
    use std::sync::Mutex;

    /// Whether a ticker frame is currently drawn on stderr, and therefore has to
    /// be erased before anything else is written there.
    ///
    /// This is state **and** the write it describes. Holding them apart - an
    /// atomic flag plus an unsynchronized write - leaves an interleaving where a
    /// logger reads "nothing is drawn", the ticker then draws, and the logger's
    /// line lands on top of the frame. So every write to stderr on this path goes
    /// through [`RENDERER`], which mutates the flag and writes the bytes under one
    /// lock. The lock is held only for the duration of one small write.
    #[derive(Default)]
    pub(in crate::debug) struct Renderer {
        live: bool,
    }

    /// Return to column 0 and erase to end of line. Preferred over blanking with
    /// the recorded width, which corrupts on a terminal resized narrower during
    /// the call.
    const CLEAR: &str = "\r\x1b[K";

    impl Renderer {
        /// The exact bytes that put `msg` on its own clean line, and the state
        /// that leaves behind. Pure, so the interleaving is testable without a
        /// terminal or a second thread.
        fn line(&mut self, msg: &str, tty: bool) -> String {
            let prefix = if self.live && tty { CLEAR } else { "" };
            self.live = false;
            format!("{prefix}{msg}\n")
        }

        /// The bytes for one in-place ticker frame. Off a TTY the caller uses
        /// [`Renderer::line`] instead, so this never emits an escape there.
        fn frame(&mut self, msg: &str) -> String {
            self.live = true;
            format!("{CLEAR}{msg}")
        }

        /// The bytes that remove a drawn frame, if any.
        fn clear(&mut self) -> String {
            if !std::mem::take(&mut self.live) {
                return String::new();
            }
            CLEAR.to_string()
        }
    }

    static RENDERER: Mutex<Renderer> = Mutex::new(Renderer { live: false });

    fn with_renderer(f: impl FnOnce(&mut Renderer, bool) -> String) {
        let mut err = std::io::stderr();
        let tty = err.is_terminal();
        let mut guard = RENDERER.lock().unwrap_or_else(|e| e.into_inner());
        let out = f(&mut guard, tty);
        if out.is_empty() {
            return;
        }
        let _ = err.write_all(out.as_bytes());
        let _ = err.flush();
    }

    /// Write `msg` to stderr as a whole line, erasing a live ticker frame first.
    /// The frame is marked gone: the ticker redraws below the message.
    pub fn emit_line(msg: &str) {
        with_renderer(|r, tty| r.line(msg, tty));
    }

    /// Draw one in-place ticker frame (TTY only; the caller uses [`emit_line`]
    /// off a TTY). A frame carries no newline, hence the flush inside.
    pub fn draw_frame(msg: &str) {
        with_renderer(|r, _| r.frame(msg));
    }

    /// Erase a drawn frame and leave the cursor at column 0. Idempotent.
    pub fn clear_live() {
        with_renderer(|r, _| r.clear());
    }

    #[cfg(test)]
    pub(super) fn test_renderer() -> Renderer {
        Renderer::default()
    }

    #[cfg(test)]
    impl Renderer {
        pub(super) fn t_line(&mut self, msg: &str, tty: bool) -> String {
            self.line(msg, tty)
        }
        pub(super) fn t_frame(&mut self, msg: &str) -> String {
            self.frame(msg)
        }
        pub(super) fn t_clear(&mut self) -> String {
            self.clear()
        }
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

    /// CLO-798 AC-10: the clear and the message are one transaction. The
    /// renderer owns both the "a frame is drawn" flag and the bytes that act on
    /// it, so the sequence below is the only one a concurrent writer can observe -
    /// an atomic flag read separately from the write would allow a log line to
    /// land on top of a frame drawn between the read and the write.
    #[test]
    fn emit_line_clears_progress_first() {
        let mut r = progress::test_renderer();
        // Nothing drawn: a plain line, no control characters at all.
        assert_eq!(
            r.t_line("gcm: [warn] retrying", true),
            "gcm: [warn] retrying\n"
        );
        // A frame, then a log line: the frame is erased in the same write.
        assert_eq!(
            r.t_frame("gcm: | grouping... 7s"),
            "\r\x1b[Kgcm: | grouping... 7s"
        );
        assert_eq!(
            r.t_line("gcm: [warn] retrying", true),
            "\r\x1b[Kgcm: [warn] retrying\n"
        );
        // The frame is gone, so a second log line does not erase again.
        assert_eq!(
            r.t_line("gcm: [warn] retrying", true),
            "gcm: [warn] retrying\n"
        );
        // The ticker resumes below the message.
        assert_eq!(
            r.t_frame("gcm: / grouping... 8s"),
            "\r\x1b[Kgcm: / grouping... 8s"
        );
        assert_eq!(r.t_clear(), "\r\x1b[K");
        assert_eq!(r.t_clear(), "", "clearing twice writes nothing");
    }

    /// CLO-798 AC-8: off a TTY the renderer never emits `\r` or an escape, even
    /// with a frame recorded - the non-TTY ticker writes whole lines, so there is
    /// nothing to erase and a redirected stderr stays plain text.
    #[test]
    fn non_tty_output_carries_no_control_characters() {
        let mut r = progress::test_renderer();
        r.t_frame("gcm: | grouping... 7s");
        let out = r.t_line("gcm: [warn] retrying", false);
        assert_eq!(out, "gcm: [warn] retrying\n");
        assert!(!out.contains('\x1b'), "escape leaked off-TTY: {out:?}");
        assert!(!out.contains('\r'), "CR leaked off-TTY: {out:?}");
    }
}
