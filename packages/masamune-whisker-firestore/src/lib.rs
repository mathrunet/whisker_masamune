//! Cloud Firestore reads and real-time listeners with an optional on-device cache
//! (`CachedFirestoreModelAdapter` / `CachedListenableFirestoreModelAdapter` in Flutter).
//!
//! [`FirestoreStore`] reads documents and queries of one Firestore database through
//! `masamune-whisker-firebase-firestore-db` and, when a cache is given, keeps the results in
//! the libsql cache of `masamune-whisker-model`:
//!
//! - [`CachePolicy`] decides how reads use the cache (cache first, stale-while-revalidate,
//!   network first or network only).
//! - Listeners first deliver the cached rows, then every server snapshot; added and modified
//!   documents are cached, removed ones (and ones the cache filter rejects) are dropped.
//! - Cache entries are scoped per backend/database/user ([`CacheOptions::scope`]) and
//!   [`FirestoreStore::set_session`] stops writes of requests started before a user switch.
//!
//! ```ignore
//! let store = FirestoreStore::new(
//!     "(default)",
//!     Some(ModelCache::shared()?),
//!     CacheOptions::new(format!("firestore:(default):{uid}")).with_ttl(Duration::from_secs(3600)),
//!     CachePolicy::StaleWhileRevalidate,
//! );
//! let posts = store.use_query("posts", Query::new().order_by("createdAt", true).limit(20));
//! ```

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use masamune_whisker_firebase_firestore_db::{self as native, DocumentRef, NativeError};
pub use masamune_whisker_firebase_firestore_db::{
    ChangeType, Document, DocumentChange, FieldValue, Listener, Source,
};
pub use masamune_whisker_model::{CacheOptions, ModelCache, Query, ReadOptions};
use masamune_whisker_model::{CachedStore, FilterKind, RemoteStore, Row};
use masamune_whisker_workers::{ApiError, ApiResult};
use serde_json::Value;
use whisker::prelude::*;

/// Maps a native Firestore error to [`ApiError`] (`UNAVAILABLE` / offline → `Network`).
pub fn api_error(error: NativeError) -> ApiError {
    let message = error.0;
    let lower = message.to_ascii_lowercase();
    if lower.contains("unavailable") || lower.contains("offline") || lower.contains("network") {
        ApiError::Network(message)
    } else if lower.contains("deadline") {
        ApiError::Timeout
    } else if lower.contains("not_found") || lower.contains("not found") {
        ApiError::Http {
            status: 404,
            body: message,
        }
    } else {
        ApiError::Other(message)
    }
}

/// Converts a backend-neutral [`Query`] of `collection` to a native Firestore query.
pub fn native_query(database: &str, collection: &str, query: &Query) -> ApiResult<native::Query> {
    let mut q = native::Query::new(database, collection);
    for filter in &query.filters {
        let value = filter.value.clone().unwrap_or(Value::Null);
        let key = filter.key.as_str();
        q = match filter.kind {
            FilterKind::EqualTo => q.equal(key, value),
            FilterKind::NotEqualTo => q.not_equal(key, value),
            FilterKind::LessThan => q.less_than(key, value),
            FilterKind::LessThanOrEqualTo => q.less_than_or_equal(key, value),
            FilterKind::GreaterThan => q.greater_than(key, value),
            FilterKind::GreaterThanOrEqualTo => q.greater_than_or_equal(key, value),
            FilterKind::WhereIn => q.within(key, value.as_array().cloned().unwrap_or_default()),
            FilterKind::WhereNotIn => {
                q.not_within(key, value.as_array().cloned().unwrap_or_default())
            }
            FilterKind::IsNull => q.equal(key, Value::Null),
            FilterKind::IsNotNull => q.not_equal(key, Value::Null),
            FilterKind::ArrayContains => q.array_contains(key, value),
            FilterKind::ArrayContainsAny => {
                q.array_contains_any(key, value.as_array().cloned().unwrap_or_default())
            }
            FilterKind::Like => {
                return Err(ApiError::Other(
                    "Firestore does not support `like` filters".into(),
                ));
            }
        };
    }
    for order in &query.order_by {
        q = q.order_by(&order.key, order.descending);
    }
    if let Some(limit) = query.limit {
        q = q.limit(limit as u32);
    }
    Ok(q)
}

/// Firestore as a [`RemoteStore`]: `database` is the Firestore database id, `table` the
/// collection path and ids are document ids. Writes merge into existing documents.
#[derive(Clone, Copy, Debug, Default)]
pub struct Firestore;

fn strip_meta(mut row: Row) -> Row {
    row.remove("@uid");
    row.remove("@path");
    row
}

impl RemoteStore for Firestore {
    async fn get(
        &self,
        database: &str,
        table: &str,
        id: &str,
        _timeout: Duration,
    ) -> ApiResult<Option<Row>> {
        DocumentRef::new(database, format!("{table}/{id}"))
            .get(Source::Default)
            .await
            .map_err(api_error)
    }

    async fn query(
        &self,
        database: &str,
        table: &str,
        query: &Query,
        _timeout: Duration,
    ) -> ApiResult<Vec<Row>> {
        native::get(&native_query(database, table, query)?)
            .await
            .map_err(api_error)
    }

    async fn save(&self, database: &str, table: &str, id: &str, row: Row) -> ApiResult<()> {
        DocumentRef::new(database, format!("{table}/{id}"))
            .set(strip_meta(row), true)
            .await
            .map_err(api_error)?;
        Ok(())
    }

    async fn delete(&self, database: &str, table: &str, id: &str) -> ApiResult<()> {
        DocumentRef::new(database, format!("{table}/{id}"))
            .delete()
            .await
            .map_err(api_error)?;
        Ok(())
    }

    fn id_field(&self) -> &str {
        "@uid"
    }
}

/// How reads use the cache.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum CachePolicy {
    /// Answer from the cache when possible; read the server otherwise (Flutter's behaviour).
    #[default]
    CacheFirst,
    /// Answer from the cache when possible and refresh it from the server in the background.
    StaleWhileRevalidate,
    /// Read the server; answer from the cache when the server cannot be reached.
    NetworkFirst,
    /// Always read the server (results are still cached for listeners and other policies).
    NetworkOnly,
}

/// Loading state for UI signals.
#[derive(Clone, Debug, PartialEq)]
pub enum Loadable<T> {
    Loading,
    Ready(T),
    Failed(String),
}

impl<T> Loadable<T> {
    pub fn ready(&self) -> Option<&T> {
        match self {
            Loadable::Ready(value) => Some(value),
            _ => None,
        }
    }
}

/// A document listener notification.
#[derive(Clone, Debug, Default)]
pub struct DocEvent {
    pub document: Option<Document>,
    /// Delivered from the on-device cache or the SDK's offline cache.
    pub from_cache: bool,
    pub error: Option<String>,
}

/// A query listener notification.
#[derive(Clone, Debug, Default)]
pub struct CollectionEvent {
    pub documents: Vec<Document>,
    pub changes: Vec<DocumentChange>,
    /// Delivered from the on-device cache or the SDK's offline cache.
    pub from_cache: bool,
    pub error: Option<String>,
}

fn with_id(id: &str, mut row: Row) -> Document {
    row.entry("@uid")
        .or_insert_with(|| Value::String(id.to_owned()));
    row
}

/// Reads and listens to one Firestore database, optionally through the on-device cache.
#[derive(Clone)]
pub struct FirestoreStore {
    inner: Rc<CachedStore<Firestore>>,
    database: String,
    policy: CachePolicy,
}

impl FirestoreStore {
    /// `database` is the Firestore database id (`"(default)"`, `"production"`, …). With
    /// `cache = None` nothing is cached and every policy behaves like `NetworkOnly`.
    pub fn new(
        database: impl Into<String>,
        cache: Option<&'static ModelCache>,
        options: CacheOptions,
        policy: CachePolicy,
    ) -> Self {
        Self {
            inner: Rc::new(CachedStore::new(Firestore, cache, options)),
            database: database.into(),
            policy,
        }
    }

    /// Without any cache.
    pub fn uncached(database: impl Into<String>) -> Self {
        Self::new(
            database,
            None,
            CacheOptions::new("firestore"),
            CachePolicy::NetworkOnly,
        )
    }

    pub fn database(&self) -> &str {
        &self.database
    }

    pub fn policy(&self) -> CachePolicy {
        self.policy
    }

    /// Starts a new session (sign-in, sign-out, user switch).
    pub fn set_session(&self) {
        self.inner.set_session();
    }

    /// Removes every cached document of this store's scope.
    pub async fn clear_cache(&self) -> ApiResult<()> {
        self.inner.clear_cache().await
    }

    fn options(&self, reload: bool) -> ReadOptions {
        match self.policy {
            CachePolicy::CacheFirst | CachePolicy::StaleWhileRevalidate => ReadOptions {
                reload,
                ..ReadOptions::default()
            },
            CachePolicy::NetworkFirst => ReadOptions {
                reload: true,
                offline_fallback: true,
                timeout: None,
            },
            CachePolicy::NetworkOnly => ReadOptions::reload(),
        }
    }

    /// Reads the document `collection/id` according to the policy (`reload` skips the cache).
    pub async fn get_doc(
        &self,
        collection: &str,
        id: &str,
        reload: bool,
    ) -> ApiResult<Option<Document>> {
        if self.policy == CachePolicy::StaleWhileRevalidate
            && !reload
            && let Some(row) = self.inner.cached(&self.database, collection, id).await
        {
            let (inner, database, table, doc_id) = (
                self.inner.clone(),
                self.database.clone(),
                collection.to_owned(),
                id.to_owned(),
            );
            spawn_local(async move {
                if let Err(error) = inner
                    .load(&database, &table, &doc_id, ReadOptions::reload())
                    .await
                {
                    log::debug!("firestore revalidate failed: {error}");
                }
            });
            return Ok(Some(with_id(id, row)));
        }
        let row = self
            .inner
            .load(&self.database, collection, id, self.options(reload))
            .await?;
        Ok(row.map(|row| with_id(id, row)))
    }

    /// Reads documents of `collection` matching `query` according to the policy. Cache-first
    /// policies answer from the cache only when it holds matching documents.
    pub async fn get(
        &self,
        collection: &str,
        query: &Query,
        reload: bool,
    ) -> ApiResult<Vec<Document>> {
        let cache_first = matches!(
            self.policy,
            CachePolicy::CacheFirst | CachePolicy::StaleWhileRevalidate
        );
        if cache_first && !reload {
            let cached = self
                .inner
                .cached_rows(&self.database, collection, query)
                .await;
            if !cached.is_empty() {
                if self.policy == CachePolicy::StaleWhileRevalidate {
                    let (inner, database, collection, query) = (
                        self.inner.clone(),
                        self.database.clone(),
                        collection.to_owned(),
                        query.clone(),
                    );
                    spawn_local(async move {
                        if let Err(error) = inner
                            .query(&database, &collection, &query, ReadOptions::reload())
                            .await
                        {
                            log::debug!("firestore revalidate failed: {error}");
                        }
                    });
                }
                return Ok(cached
                    .into_iter()
                    .map(|(id, row)| with_id(&id, row))
                    .collect());
            }
        }
        let options = match self.policy {
            CachePolicy::NetworkFirst => self.options(true),
            _ => ReadOptions::reload(),
        };
        let rows = self
            .inner
            .query(&self.database, collection, query, options)
            .await?;
        Ok(rows
            .into_iter()
            .map(|(id, row)| with_id(&id, row))
            .collect())
    }

    /// Writes (merges) a document and updates the cache.
    pub async fn save(&self, collection: &str, id: &str, data: Document) -> ApiResult<()> {
        self.inner.save(&self.database, collection, id, data).await
    }

    /// Deletes a document and its cache entry.
    pub async fn delete(&self, collection: &str, id: &str) -> ApiResult<()> {
        self.inner.delete(&self.database, collection, id).await
    }

    /// Listens to the document `collection/id`: the cached copy first (when there is one), then
    /// every server snapshot, which also updates the cache.
    pub fn listen_doc(
        &self,
        collection: &str,
        id: &str,
        handler: impl Fn(DocEvent) + 'static,
    ) -> ApiResult<Listener> {
        let handler = Rc::new(handler);
        let delivered = Rc::new(Cell::new(false));
        let session = self.inner.session();
        {
            let (inner, database, collection, id) = (
                self.inner.clone(),
                self.database.clone(),
                collection.to_owned(),
                id.to_owned(),
            );
            let (handler, delivered) = (handler.clone(), delivered.clone());
            spawn_local(async move {
                if let Some(row) = inner.cached(&database, &collection, &id).await
                    && !delivered.get()
                {
                    handler(DocEvent {
                        document: Some(with_id(&id, row)),
                        from_cache: true,
                        error: None,
                    });
                }
            });
        }
        let (inner, database, table, doc_id) = (
            self.inner.clone(),
            self.database.clone(),
            collection.to_owned(),
            id.to_owned(),
        );
        DocumentRef::new(self.database.clone(), format!("{collection}/{id}"))
            .listen(move |snapshot| {
                delivered.set(true);
                if snapshot.error.is_none() {
                    let (inner, database, table, doc_id, row) = (
                        inner.clone(),
                        database.clone(),
                        table.clone(),
                        doc_id.clone(),
                        snapshot.document.clone(),
                    );
                    spawn_local(async move {
                        inner
                            .write_cache(session, &database, &table, &doc_id, row.as_ref())
                            .await;
                    });
                }
                handler(DocEvent {
                    document: snapshot.document,
                    from_cache: snapshot.from_cache,
                    error: snapshot.error,
                });
            })
            .map_err(api_error)
    }

    /// Listens to documents of `collection` matching `query`: the cached matches first (when
    /// there are any), then every server snapshot. Added and modified documents are cached,
    /// removed ones are dropped from the cache.
    pub fn listen(
        &self,
        collection: &str,
        query: &Query,
        handler: impl Fn(CollectionEvent) + 'static,
    ) -> ApiResult<Listener> {
        let native_query = native_query(&self.database, collection, query)?;
        let handler = Rc::new(handler);
        let delivered = Rc::new(Cell::new(false));
        let session = self.inner.session();
        {
            let (inner, database, collection, query) = (
                self.inner.clone(),
                self.database.clone(),
                collection.to_owned(),
                query.clone(),
            );
            let (handler, delivered) = (handler.clone(), delivered.clone());
            spawn_local(async move {
                let cached = inner.cached_rows(&database, &collection, &query).await;
                if !cached.is_empty() && !delivered.get() {
                    handler(CollectionEvent {
                        documents: cached
                            .into_iter()
                            .map(|(id, row)| with_id(&id, row))
                            .collect(),
                        changes: Vec::new(),
                        from_cache: true,
                        error: None,
                    });
                }
            });
        }
        let (inner, database, table) = (
            self.inner.clone(),
            self.database.clone(),
            collection.to_owned(),
        );
        native::listen(&native_query, move |snapshot| {
            delivered.set(true);
            if snapshot.error.is_none() {
                let updates: Vec<(String, Option<Row>)> = snapshot
                    .changes
                    .iter()
                    .map(|change| {
                        let row = match change.kind {
                            ChangeType::Removed => None,
                            _ => snapshot
                                .documents
                                .iter()
                                .find(|doc| {
                                    doc.get("@uid").and_then(Value::as_str)
                                        == Some(change.id.as_str())
                                })
                                .cloned(),
                        };
                        (change.id.clone(), row)
                    })
                    .collect();
                let (inner, database, table) = (inner.clone(), database.clone(), table.clone());
                spawn_local(async move {
                    for (id, row) in updates {
                        inner
                            .write_cache(session, &database, &table, &id, row.as_ref())
                            .await;
                    }
                });
            }
            handler(CollectionEvent {
                documents: snapshot.documents,
                changes: snapshot.changes,
                from_cache: snapshot.from_cache,
                error: snapshot.error,
            });
        })
        .map_err(api_error)
    }

    /// The document as a signal that follows the server (listener kept alive with the calling
    /// component). Call in a component body.
    pub fn use_doc(&self, collection: &str, id: &str) -> ReadSignal<Loadable<Option<Document>>> {
        let state = signal(Loadable::Loading);
        match self.listen_doc(collection, id, move |event| {
            state.set(match event.error {
                Some(error) => Loadable::Failed(error),
                None => Loadable::Ready(event.document),
            });
        }) {
            Ok(listener) => {
                let _keep = StoredValue::new(listener);
            }
            Err(error) => state.set(Loadable::Failed(error.to_string())),
        }
        state.read_only()
    }

    /// The query results as a signal that follows the server (listener kept alive with the
    /// calling component). Call in a component body.
    pub fn use_query(&self, collection: &str, query: Query) -> ReadSignal<Loadable<Vec<Document>>> {
        let state = signal(Loadable::Loading);
        match self.listen(collection, &query, move |event| {
            state.set(match event.error {
                Some(error) => Loadable::Failed(error),
                None => Loadable::Ready(event.documents),
            });
        }) {
            Ok(listener) => {
                let _keep = StoredValue::new(listener);
            }
            Err(error) => state.set(Loadable::Failed(error.to_string())),
        }
        state.read_only()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn converts_queries() {
        let q = Query::new()
            .equal_to("userId", "u")
            .where_in("state", json!(["a", "b"]))
            .is_null("deletedAt")
            .order_by("createdAt", true)
            .limit(10);
        let native =
            serde_json::to_value(native_query("production", "items", &q).unwrap()).unwrap();
        assert_eq!(native["database"], "production");
        assert_eq!(native["filters"][1]["op"], "in");
        assert_eq!(native["filters"][2]["op"], "==");
        assert_eq!(native["filters"][2]["value"], Value::Null);
        assert_eq!(native["orderBy"][0]["descending"], true);
        assert_eq!(native["limit"], 10);
        assert!(native_query("d", "c", &Query::new().like("name", "a%")).is_err());
    }

    #[test]
    fn maps_errors() {
        assert!(api_error(NativeError("UNAVAILABLE: offline".into())).is_offline());
        assert!(api_error(NativeError("NOT_FOUND: no document".into())).is_not_found());
        assert_eq!(
            api_error(NativeError("PERMISSION_DENIED".into())),
            ApiError::Other("PERMISSION_DENIED".into())
        );
    }
}
