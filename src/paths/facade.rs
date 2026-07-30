//! XDG-style base-directory resolution binary facade (CLO-597).
//!
//! The actual implementation is in the library's `gcm::paths` module; the binary
//! just re-exports it so the rest of the CLI keeps using `crate::paths::*`.

pub use gcm::paths::*;
