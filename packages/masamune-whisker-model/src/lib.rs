//! Shared model layer for Whisker apps.
//!
//! - [`query`]: backend-neutral filters, ordering and limits (Masamune wire format).
//! - [`cache`]: on-device document cache backed by a local libsql database.

pub mod query;

#[cfg(not(target_arch = "wasm32"))]
pub mod cache;

#[cfg(not(target_arch = "wasm32"))]
pub use cache::{CacheError, CacheResult, CachedDocument, ModelCache, now_ms};
pub use query::{Filter, FilterKind, Order, Query, Row};
