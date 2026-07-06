//! `gcm resolve` — LLM-assisted merge conflict resolver (Phase 1: local markers).
//!
//! Public entry point is [`run_resolve`] (called from `main.rs` for the
//! `resolve` subcommand). All sub-modules are implementation details.

pub mod classify;
pub mod markers;
pub mod report;
pub mod validate;
