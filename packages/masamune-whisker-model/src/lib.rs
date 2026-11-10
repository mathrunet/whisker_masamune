//! Shared model layer for Whisker apps.
//!
//! - [`query`]: backend-neutral filters, ordering and limits (Masamune wire format).
//! - [`cache`]: on-device document cache backed by a local libsql database.
//! - [`rest`]: client of the Masamune database workers through a `FunctionsAdapter`.
//! - [`store`]: remote store + cache with deadlines, sessions and cache filters.
//!
//! `CachedStore::load` / `query` run inside a `masamune_whisker_logger::start_trace` named
//! `masamune.model.load|<document|collection>|<load|query>|<database>.<table>`, so logger
//! adapters (such as `masamune-whisker-ai-debugger`) can detect slow loads.

pub mod query;

#[cfg(not(target_arch = "wasm32"))]
pub mod cache;
pub mod rest;
#[cfg(not(target_arch = "wasm32"))]
pub mod store;

#[cfg(not(target_arch = "wasm32"))]
pub use cache::{CacheError, CacheResult, CachedDocument, ModelCache, now_ms};
pub use masamune_whisker_functions::{ApiError, ApiResult, RetryPolicy, retry, with_timeout};
pub use query::{Filter, FilterKind, Order, Query, Row};
pub use rest::{RestDatabase, data_rows, normalize_prefix};
#[cfg(not(target_arch = "wasm32"))]
pub use store::{CacheFilter, CacheOptions, CachedStore, ReadOptions, RemoteStore};
