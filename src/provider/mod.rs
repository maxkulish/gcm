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

pub mod identity;
pub mod http;
pub mod models;

pub use identity::{
    AuthMethod, ErrorKind, ModelSource, ProviderError, ProviderId,
    resolve_model_with_source,
};

pub use models::{FetchSource, ModelFetchOutcome, fetch_supported_models_with};
#[cfg(feature = "cli")]
pub use models::fetch_supported_models;
