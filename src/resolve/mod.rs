//! `gcm resolve` — LLM-assisted merge conflict resolver (Phase 1: local markers).
//!
//! Public entry point is [`run_resolve`] (called from `main.rs` for the
//! `resolve` subcommand). All sub-modules are implementation details.

pub mod classify;
pub mod markers;
pub mod report;
pub mod validate;

use crate::cli::Cli;
use crate::error::GcmError;

/// Entry point for `gcm resolve`. Stub for ST5; fully implemented in ST9.
pub fn run_resolve(_args: &Cli) -> Result<(), GcmError> {
    Err(GcmError::Config("gcm resolve not yet implemented".to_string()))
}
