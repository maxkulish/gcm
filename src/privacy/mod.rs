pub mod detect;
pub mod entropy;
pub mod rules;

use std::fmt;
use std::ops::Range;

pub const SECRET_SCAN_ENV: &str = "GCM_SECRET_SCAN";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[cfg_attr(feature = "clap", value(rename_all = "lower"))]
pub enum SecretScanMode {
    Off,
    Redact,
    Abort,
}

impl SecretScanMode {
    pub fn resolve_with<F>(explicit: Option<Self>, env_lookup: F) -> Result<Self, ScanError>
    where
        F: for<'a> FnOnce(&'a str) -> Option<String>,
    {
        let raw = match explicit {
            Some(mode) => return Ok(mode),
            None => match env_lookup(SECRET_SCAN_ENV) {
                Some(raw) => raw,
                None => return Ok(Self::Off),
            },
        };

        Self::parse(&raw)
    }

    pub fn parse(raw: &str) -> Result<Self, ScanError> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "" | "off" => Ok(Self::Off),
            "redact" => Ok(Self::Redact),
            "abort" => Ok(Self::Abort),
            other => Err(ScanError::InvalidMode {
                value: other.to_string(),
            }),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScanError {
    SecretDetected { count: usize },
    InvalidMode { value: String },
    RulePack { message: String },
}

impl fmt::Display for ScanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScanError::SecretDetected { count } => {
                write!(
                    f,
                    "secret scan detected {count} credential-like value(s); no provider request was sent."
                )
            }
            ScanError::InvalidMode { value } => {
                write!(
                    f,
                    "unknown {} value '{value}'. Use off, redact, or abort.",
                    SECRET_SCAN_ENV
                )
            }
            ScanError::RulePack { message } => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for ScanError {}

#[derive(Clone, Copy, Debug)]
pub struct Scanner<'e> {
    mode: SecretScanMode,
    engine: &'e rules::RuleEngine,
}

impl<'e> Scanner<'e> {
    pub fn new(mode: SecretScanMode, engine: &'e rules::RuleEngine) -> Self {
        Self { mode, engine }
    }

    pub fn vendored(mode: SecretScanMode) -> Result<Self, ScanError> {
        let engine = rules::vendored().map_err(|e| ScanError::RulePack { message: e })?;
        Ok(Self::new(mode, engine))
    }

    pub fn mode(&self) -> SecretScanMode {
        self.mode
    }

    pub fn engine(&self) -> &'e rules::RuleEngine {
        self.engine
    }

    pub fn scan(&self, text: String) -> Result<String, ScanError> {
        match self.mode {
            SecretScanMode::Off => Ok(text),
            SecretScanMode::Redact => Ok(detect::redact_secrets(&text, self.engine)),
            SecretScanMode::Abort => {
                let count = detect::secret_ranges(&text, self.engine).len();
                if count > 0 {
                    Err(ScanError::SecretDetected { count })
                } else {
                    Ok(text)
                }
            }
        }
    }

    pub fn ranges(&self, text: &str) -> Vec<Range<usize>> {
        detect::secret_ranges(text, self.engine)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_accepts_all_modes_case_insensitively() {
        assert_eq!(SecretScanMode::parse("off").unwrap(), SecretScanMode::Off);
        assert_eq!(SecretScanMode::parse("OFF").unwrap(), SecretScanMode::Off);
        assert_eq!(
            SecretScanMode::parse(" redact ").unwrap(),
            SecretScanMode::Redact
        );
        assert_eq!(
            SecretScanMode::parse("aBoRt").unwrap(),
            SecretScanMode::Abort
        );
        assert_eq!(SecretScanMode::parse("").unwrap(), SecretScanMode::Off);
    }

    #[test]
    fn parse_rejects_unknown_value() {
        let err = SecretScanMode::parse("panic").unwrap_err();
        assert!(matches!(err, ScanError::InvalidMode { .. }));
        assert_eq!(
            err.to_string(),
            "unknown GCM_SECRET_SCAN value 'panic'. Use off, redact, or abort."
        );
    }

    #[test]
    fn resolve_with_prefers_explicit_over_env() {
        let mode =
            SecretScanMode::resolve_with(Some(SecretScanMode::Off), |_| Some("abort".to_string()))
                .unwrap();
        assert_eq!(mode, SecretScanMode::Off);
    }

    #[test]
    fn resolve_with_reads_caller_env_map() {
        fn env_lookup(key: &str) -> Option<String> {
            if key == SECRET_SCAN_ENV {
                Some("abort".to_string())
            } else {
                None
            }
        }

        let mode = SecretScanMode::resolve_with(None, env_lookup).unwrap();
        assert_eq!(mode, SecretScanMode::Abort);
    }

    #[test]
    fn resolve_with_defaults_off_when_absent() {
        let mode = SecretScanMode::resolve_with(None, |_| None).unwrap();
        assert_eq!(mode, SecretScanMode::Off);
    }

    #[test]
    fn resolve_with_never_touches_process_env() {
        let mode = SecretScanMode::resolve_with(Some(SecretScanMode::Off), |_| {
            panic!("env lookup was called")
        })
        .unwrap();
        assert_eq!(mode, SecretScanMode::Off);
    }

    #[test]
    fn scanner_off_is_identity() {
        let scanner = Scanner::new(SecretScanMode::Off, rules::vendored().unwrap());
        let text = "AWS=AKIAABCDEFGHIJKLMNOP\n".to_string();
        assert_eq!(scanner.scan(text.clone()).unwrap(), text);
    }

    #[test]
    fn scanner_redact_replaces_every_match() {
        let scanner = Scanner::new(SecretScanMode::Redact, rules::vendored().unwrap());
        let text = "token=ghp_abcdefghijklmnopqrstuvwxyz123456\nAWS=AKIAABCDEFGHIJKLMNOP\n";
        let redacted = scanner.scan(text.to_string()).unwrap();
        assert!(!redacted.contains("ghp_abcdefghijklmnopqrstuvwxyz123456"));
        assert!(!redacted.contains("AKIAABCDEFGHIJKLMNOP"));
        assert_eq!(redacted.matches("[REDACTED: secret]").count(), 2);
    }

    #[test]
    fn scanner_abort_reports_detection_count() {
        let scanner = Scanner::new(SecretScanMode::Abort, rules::vendored().unwrap());
        assert!(matches!(
            scanner.scan("API_KEY=supersecret12345".to_string()),
            Err(ScanError::SecretDetected { count: 1 })
        ));
    }

    #[test]
    fn scanner_ranges_match_detect_secret_ranges() {
        let scanner = Scanner::new(SecretScanMode::Redact, rules::vendored().unwrap());
        let text = "token=ghp_abcdefghijklmnopqrstuvwxyz123456\nAWS=AKIAABCDEFGHIJKLMNOP\n";
        let expected = detect::secret_ranges(text, scanner.engine()).len();
        let ranges = scanner.ranges(text).len();
        assert_eq!(expected, ranges);
    }
}
