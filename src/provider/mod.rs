//! Provider identity and registry for the gcm library (CLO-596).
//!
//! This module is the **library** surface: it contains the provider identity
//! types (`ProviderId`, `AuthMethod`, `ModelSource`, `ProviderError`, `ErrorKind`),
//! the pure model resolver (`resolve_model_with_source`), the injectable model
//! registry (`models::fetch_supported_models_with`), and provider-agnostic HTTP
//! request types (`http::HttpRequest`, `http::HttpGet`).
//!
//! Binary-only concerns (the `Provider` trait, `select()`, conflict
//! resolution, per-provider backends, and the `ureq`-based transport) live in the
//! binary facade at `src/provider/facade.rs`.
//!
//! See `docs/adrs/002-library-boundary.md` for the boundary rules.

/// Internal HTTP request/response helpers shared with the binary facade.
/// Items are `pub` only because the binary facade is compiled as a separate
/// crate target; external consumers should use the injectable
/// `models::fetch_supported_models_with` API rather than reaching into this
/// module. Hidden from public docs.
#[doc(hidden)]
pub mod http;

pub mod identity;
pub mod models;

pub use identity::{
    resolve_model_with_source, AuthMethod, ErrorKind, ModelSource, ProviderError, ProviderId,
};

#[cfg(feature = "cli")]
pub use models::fetch_supported_models;
pub use models::{fetch_supported_models_with, FetchSource, ModelFetchOutcome};
