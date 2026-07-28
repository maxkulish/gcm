//! gcm library crate.
//!
//! ```rust
//! use gcm::privacy::{rules, ScanError, Scanner, SecretScanMode};
//!
//! let engine = rules::vendored().map_err(|msg| ScanError::RulePack { message: msg })?;
//! let scanner = Scanner::new(SecretScanMode::Redact, engine);
//! let redacted = scanner.scan("token=ghp_abcdefghijklmnopqrstuvwxyz123456\n".to_string())?;
//! assert!(redacted.contains("[REDACTED: secret]"));
//!
//! # Ok::<(), ScanError>(())
//! ```

pub mod privacy;

pub mod provider;

#[doc(hidden)]
pub mod debug;
