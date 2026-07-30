//! gcm library crate.
//!
//! Exposes reusable, transport-free building blocks for in-org consumers:
//!
//! - `gcm::privacy` — secret scanning (CLO-595)
//! - `gcm::provider` — provider identity, model resolution, and the injectable
//!   model registry (CLO-596)
//! - `gcm::config` — persisted configuration types and pure helpers (CLO-597)
//! - `gcm::status` — source-attributed provider status resolution (CLO-597)
//!
//! The CLI lives in the `gcm` binary target; this crate carries no `clap`,
//! `cliclack`, or HTTP transport unless the `cli` feature is enabled.
//! See `docs/adrs/002-library-boundary.md`.
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

pub mod config;
pub mod paths;
pub mod privacy;
pub mod provider;
pub mod status;

#[doc(hidden)]
pub mod debug;
