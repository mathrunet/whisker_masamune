//! Shared model layer for Whisker apps.
//!
//! - [`query`]: backend-neutral filters, ordering and limits (Masamune wire format).
//! - [`cache`]: on-device document cache backed by a local libsql database.
//! - [`rest`]: client of the Masamune Cloudflare database workers.
//! - [`store`]: remote store + cache with deadlines, sessions and cache filters.

pub mod query;

#[cfg(not(target_arch = "wasm32"))]
pub mod cache;
pub mod rest;
#[cfg(not(target_arch = "wasm32"))]
pub mod store;

#[cfg(not(target_arch = "wasm32"))]
pub use cache::{CacheError, CacheResult, CachedDocument, ModelCache, now_ms};
pub use query::{Filter, FilterKind, Order, Query, Row};
pub use rest::{RestDatabase, data_rows, normalize_prefix};
#[cfg(not(target_arch = "wasm32"))]
pub use store::{CacheFilter, CacheOptions, CachedStore, ReadOptions, RemoteStore};
