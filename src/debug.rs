//! Minimal logging for gcm (CLO-493 / FR-38).
//!
//! `GCM_LOG_LEVEL` governs the active level (`off|error|warn|info|debug|trace`).
//! If `GCM_LOG_LEVEL` is unset, the legacy `GCM_DEBUG` flag provides backward
//! compatibility: any non-empty, non-`0` value enables debug-level output.
//! All log lines go to stderr.

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

/// The effective log level: `GCM_LOG_LEVEL` wins over the legacy `GCM_DEBUG`
/// flag. Default is `Off`.
pub fn log_level() -> Level {
    if let Ok(v) = std::env::var("GCM_LOG_LEVEL") {
        if !v.trim().is_empty() {
            return v.parse().unwrap_or(Level::Off);
        }
    }
    if debug_flag(std::env::var("GCM_DEBUG").ok().as_deref()) {
        return Level::Debug;
    }
    Level::Off
}

/// Pure predicate behind the legacy `GCM_DEBUG` behaviour.
fn debug_flag(value: Option<&str>) -> bool {
    matches!(value, Some(v) if !v.is_empty() && v != "0")
}

/// Whether a message at `level` would be emitted right now.
pub fn enabled(level: Level) -> bool {
    log_level() >= level
}

/// Emit a single log line to stderr if `level` is enabled.
#[macro_export]
macro_rules! log {
    ($level:expr, $($arg:tt)*) => {
        if $crate::debug::enabled($level) {
            eprintln!("gcm: [{}] {}", $level.as_str(), format_args!($($arg)*));
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
}
