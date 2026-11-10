//! Remote store with an optional on-device cache (`CachedTursoModelAdapter` /
//! `BoundedTursoModelAdapter` in Flutter).
//!
//! - Reads have a deadline ([`CacheOptions::read_timeout`], 4.5 s by default); timed-out reads
//!   are not retried.
//! - Concurrent reads of the same row share one request.
//! - [`CachedStore::set_session`] starts a new generation (for example on sign-in or sign-out):
//!   results of requests started earlier are returned to their callers but never written to the
//!   cache.
//! - [`CacheOptions::cache_filter`] decides what is cached; rows it rejects are removed from
//!   the cache instead of being left stale.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use futures_util::FutureExt;
use futures_util::future::{LocalBoxFuture, Shared};
use masamune_whisker_workers::{ApiError, ApiResult, with_timeout};

use crate::cache::{CacheError, ModelCache, now_ms};
use crate::query::{Query, Row};

/// A backend that can read and write rows (Turso, TiDB, Firestore, …).
///
/// `database` is the per-user (or shared) database name and `table` the table/collection.
/// Futures run on the UI thread and do not need to be `Send`.
#[allow(async_fn_in_trait)]
pub trait RemoteStore {
    async fn get(
        &self,
        database: &str,
        table: &str,
        id: &str,
        timeout: Duration,
    ) -> ApiResult<Option<Row>>;
    async fn query(
        &self,
        database: &str,
        table: &str,
        query: &Query,
        timeout: Duration,
    ) -> ApiResult<Vec<Row>>;
    async fn save(&self, database: &str, table: &str, id: &str, row: Row) -> ApiResult<()>;
    async fn delete(&self, database: &str, table: &str, id: &str) -> ApiResult<()>;
    /// Field holding the row id in query results.
    fn id_field(&self) -> &str {
        "id"
    }
}

/// Decides whether a row of `table` is cached.
pub type CacheFilter = Rc<dyn Fn(&str, &Row) -> bool>;

/// Cache settings.
#[derive(Clone)]
pub struct CacheOptions {
    /// Prefix of every cache scope (separates backends and environments, for example
    /// `"turso:dev_"`).
    pub scope: String,
    /// Deadline of reads.
    pub read_timeout: Duration,
    /// Cached rows older than this are not used (`None` keeps them until replaced).
    pub ttl: Option<Duration>,
    /// `None` caches every row.
    pub cache_filter: Option<CacheFilter>,
}

impl CacheOptions {
    pub fn new(scope: impl Into<String>) -> Self {
        Self {
            scope: scope.into(),
            read_timeout: Duration::from_millis(4500),
            ttl: None,
            cache_filter: None,
        }
    }

    pub fn with_filter(mut self, filter: impl Fn(&str, &Row) -> bool + 'static) -> Self {
        self.cache_filter = Some(Rc::new(filter));
        self
    }

    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.ttl = Some(ttl);
        self
    }

    pub fn with_read_timeout(mut self, timeout: Duration) -> Self {
        self.read_timeout = timeout;
        self
    }
}

/// Options of one read.
#[derive(Clone, Copy, Debug, Default)]
pub struct ReadOptions {
    /// Skip the cache and read from the server.
    pub reload: bool,
    /// Return cached rows when the server cannot be reached.
    pub offline_fallback: bool,
    /// Overrides [`CacheOptions::read_timeout`].
    pub timeout: Option<Duration>,
}

impl ReadOptions {
    pub fn reload() -> Self {
        Self {
            reload: true,
            ..Self::default()
        }
    }
}

type SharedRead = Shared<LocalBoxFuture<'static, ApiResult<Option<Row>>>>;

fn cache_error(error: CacheError) -> ApiError {
    ApiError::Other(error.to_string())
}

/// A [`RemoteStore`] with a [`ModelCache`] in front of it.
pub struct CachedStore<R: RemoteStore + 'static> {
    remote: Rc<R>,
    cache: Option<&'static ModelCache>,
    options: CacheOptions,
    generation: Rc<Cell<u64>>,
    inflight: Rc<RefCell<HashMap<String, SharedRead>>>,
}

impl<R: RemoteStore + 'static> CachedStore<R> {
    /// `cache = None` disables caching (every read goes to the server).
    pub fn new(remote: R, cache: Option<&'static ModelCache>, options: CacheOptions) -> Self {
        Self {
            remote: Rc::new(remote),
            cache,
            options,
            generation: Rc::new(Cell::new(0)),
            inflight: Rc::new(RefCell::new(HashMap::new())),
        }
    }

    pub fn remote(&self) -> &R {
        &self.remote
    }

    fn collection(database: &str, table: &str) -> String {
        format!("{database}/{table}")
    }

    fn cacheable(&self, table: &str, row: &Row) -> bool {
        self.options
            .cache_filter
            .as_ref()
            .is_none_or(|filter| filter(table, row))
    }

    fn fresh(&self, cached_at: i64) -> bool {
        self.options
            .ttl
            .is_none_or(|ttl| now_ms() - cached_at <= ttl.as_millis() as i64)
    }

    /// Starts a new session (sign-in, sign-out, user switch). Pending reads keep running, but
    /// their results are no longer written to the cache.
    pub fn set_session(&self) {
        self.generation.set(self.generation.get() + 1);
        self.inflight.borrow_mut().clear();
    }

    /// Cached row, if any (and fresh).
    pub async fn cached(&self, database: &str, table: &str, id: &str) -> Option<Row> {
        let cache = self.cache?;
        let doc = cache
            .get(&self.options.scope, &Self::collection(database, table), id)
            .await
            .ok()
            .flatten()?;
        (self.fresh(doc.cached_at) && self.cacheable(table, &doc.data)).then_some(doc.data)
    }

    async fn store(
        &self,
        generation: u64,
        database: &str,
        table: &str,
        id: &str,
        row: Option<&Row>,
    ) {
        let Some(cache) = self.cache else { return };
        if generation != self.generation.get() {
            return;
        }
        let collection = Self::collection(database, table);
        let result = match row {
            Some(row) if self.cacheable(table, row) => {
                cache
                    .put(&self.options.scope, &collection, id, row, None)
                    .await
            }
            _ => cache.remove(&self.options.scope, &collection, id).await,
        };
        if let Err(error) = result {
            log::warn!("model cache write failed: {error}");
        }
    }

    /// Current session number (see [`CachedStore::set_session`]).
    pub fn session(&self) -> u64 {
        self.generation.get()
    }

    /// Writes a row received outside [`CachedStore::load`] (for example from a real-time
    /// listener) to the cache, or removes it when `row` is `None` or rejected by the filter.
    /// Ignored when `session` is no longer current.
    pub async fn write_cache(
        &self,
        session: u64,
        database: &str,
        table: &str,
        id: &str,
        row: Option<&Row>,
    ) {
        self.store(session, database, table, id, row).await;
    }

    /// Fresh cached rows of `table` matching `query`, as `(id, row)` pairs.
    pub async fn cached_rows(
        &self,
        database: &str,
        table: &str,
        query: &Query,
    ) -> Vec<(String, Row)> {
        let Some(cache) = self.cache else {
            return Vec::new();
        };
        cache
            .query(
                &self.options.scope,
                &Self::collection(database, table),
                query,
            )
            .await
            .unwrap_or_default()
            .into_iter()
            .filter(|doc| self.fresh(doc.cached_at) && self.cacheable(table, &doc.data))
            .map(|doc| (doc.id, doc.data))
            .collect()
    }

    /// Reads one row: from the cache unless `options.reload`, otherwise from the server (and
    /// caches the result). Traced as `masamune.model.load|document|load|<database>.<table>`.
    pub async fn load(
        &self,
        database: &str,
        table: &str,
        id: &str,
        options: ReadOptions,
    ) -> ApiResult<Option<Row>> {
        let _trace = masamune_whisker_logger::start_trace(format!(
            "masamune.model.load|document|load|{database}.{table}"
        ));
        if !options.reload
            && let Some(row) = self.cached(database, table, id).await
        {
            return Ok(Some(row));
        }
        let generation = self.generation.get();
        let key = format!("{generation}\u{1}{database}\u{1}{table}\u{1}{id}");
        let timeout = options.timeout.unwrap_or(self.options.read_timeout);
        let shared = {
            let mut inflight = self.inflight.borrow_mut();
            inflight
                .entry(key.clone())
                .or_insert_with(|| {
                    let remote = self.remote.clone();
                    let (database, table, id) =
                        (database.to_owned(), table.to_owned(), id.to_owned());
                    async move {
                        with_timeout(timeout, remote.get(&database, &table, &id, timeout)).await
                    }
                    .boxed_local()
                    .shared()
                })
                .clone()
        };
        let result = shared.await;
        self.inflight.borrow_mut().remove(&key);
        match result {
            Ok(row) => {
                self.store(generation, database, table, id, row.as_ref())
                    .await;
                Ok(row)
            }
            Err(error)
                if options.offline_fallback
                    && (error.is_offline() || error == ApiError::Timeout) =>
            {
                match self.cached(database, table, id).await {
                    Some(row) => Ok(Some(row)),
                    None => Err(error),
                }
            }
            Err(error) => Err(error),
        }
    }

    /// Reads rows matching `query` from the server and caches them. With
    /// `options.offline_fallback`, answers from the cache when the server cannot be reached.
    /// Returns `(id, row)` pairs. Traced as `masamune.model.load|collection|query|<database>.<table>`.
    pub async fn query(
        &self,
        database: &str,
        table: &str,
        query: &Query,
        options: ReadOptions,
    ) -> ApiResult<Vec<(String, Row)>> {
        let _trace = masamune_whisker_logger::start_trace(format!(
            "masamune.model.load|collection|query|{database}.{table}"
        ));
        let generation = self.generation.get();
        let timeout = options.timeout.unwrap_or(self.options.read_timeout);
        let result =
            with_timeout(timeout, self.remote.query(database, table, query, timeout)).await;
        match result {
            Ok(rows) => {
                let id_field = self.remote.id_field().to_owned();
                let rows: Vec<(String, Row)> = rows
                    .into_iter()
                    .filter_map(|row| {
                        let id = row.get(&id_field).and_then(|v| v.as_str())?.to_owned();
                        Some((id, row))
                    })
                    .collect();
                if let Some(cache) = self.cache
                    && generation == self.generation.get()
                {
                    let collection = Self::collection(database, table);
                    let (keep, drop): (Vec<_>, Vec<_>) =
                        rows.iter().partition(|(_, row)| self.cacheable(table, row));
                    let entries = keep
                        .into_iter()
                        .map(|(id, row)| (id.clone(), row.clone(), None))
                        .collect();
                    if let Err(error) = cache
                        .put_many(&self.options.scope, &collection, entries)
                        .await
                    {
                        log::warn!("model cache write failed: {error}");
                    }
                    for (id, _) in drop {
                        let _ = cache.remove(&self.options.scope, &collection, id).await;
                    }
                }
                Ok(rows)
            }
            Err(error)
                if options.offline_fallback
                    && (error.is_offline() || error == ApiError::Timeout) =>
            {
                let Some(cache) = self.cache else {
                    return Err(error);
                };
                let docs = cache
                    .query(
                        &self.options.scope,
                        &Self::collection(database, table),
                        query,
                    )
                    .await
                    .map_err(cache_error)?;
                Ok(docs
                    .into_iter()
                    .filter(|doc| self.fresh(doc.cached_at))
                    .map(|doc| (doc.id, doc.data))
                    .collect())
            }
            Err(error) => Err(error),
        }
    }

    /// Writes a row to the server, then to the cache (or removes it there when the filter
    /// rejects it).
    pub async fn save(&self, database: &str, table: &str, id: &str, row: Row) -> ApiResult<()> {
        let generation = self.generation.get();
        self.remote.save(database, table, id, row.clone()).await?;
        self.store(generation, database, table, id, Some(&row))
            .await;
        Ok(())
    }

    /// Deletes a row on the server and in the cache.
    pub async fn delete(&self, database: &str, table: &str, id: &str) -> ApiResult<()> {
        self.remote.delete(database, table, id).await?;
        if let Some(cache) = self.cache {
            let _ = cache
                .remove(&self.options.scope, &Self::collection(database, table), id)
                .await;
        }
        Ok(())
    }

    /// Removes every cached row of this store (all databases under its scope).
    pub async fn clear_cache(&self) -> ApiResult<()> {
        self.set_session();
        match self.cache {
            Some(cache) => cache
                .clear_scope(&self.options.scope)
                .await
                .map_err(cache_error),
            None => Ok(()),
        }
    }

    /// Removes the cached rows of one database (for example one user's).
    pub async fn clear_database(&self, database: &str) -> ApiResult<()> {
        let Some(cache) = self.cache else {
            return Ok(());
        };
        let docs = cache
            .list_collections(&self.options.scope, &format!("{database}/"))
            .await
            .map_err(cache_error)?;
        for collection in docs {
            cache
                .clear_collection(&self.options.scope, &collection)
                .await
                .map_err(cache_error)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    use serde_json::json;

    #[derive(Default)]
    struct FakeRemote {
        rows: RefCell<HashMap<String, Row>>,
        gets: Cell<u32>,
        offline: Cell<bool>,
    }

    impl RemoteStore for Rc<FakeRemote> {
        async fn get(
            &self,
            _db: &str,
            table: &str,
            id: &str,
            _t: Duration,
        ) -> ApiResult<Option<Row>> {
            self.gets.set(self.gets.get() + 1);
            if self.offline.get() {
                return Err(ApiError::Network("offline".into()));
            }
            Ok(self.rows.borrow().get(&format!("{table}/{id}")).cloned())
        }
        async fn query(
            &self,
            _db: &str,
            table: &str,
            query: &Query,
            _t: Duration,
        ) -> ApiResult<Vec<Row>> {
            if self.offline.get() {
                return Err(ApiError::Network("offline".into()));
            }
            let rows: Vec<Row> = self
                .rows
                .borrow()
                .iter()
                .filter(|(k, _)| k.starts_with(&format!("{table}/")))
                .map(|(_, v)| v.clone())
                .collect();
            Ok(query.apply(rows, |r| r))
        }
        async fn save(&self, _db: &str, table: &str, id: &str, row: Row) -> ApiResult<()> {
            self.rows.borrow_mut().insert(format!("{table}/{id}"), row);
            Ok(())
        }
        async fn delete(&self, _db: &str, table: &str, id: &str) -> ApiResult<()> {
            self.rows.borrow_mut().remove(&format!("{table}/{id}"));
            Ok(())
        }
    }

    fn row(v: serde_json::Value) -> Row {
        v.as_object().unwrap().clone()
    }

    fn setup() -> (
        Rc<FakeRemote>,
        CachedStore<Rc<FakeRemote>>,
        tempfile::TempDir,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let cache: &'static ModelCache =
            Box::leak(Box::new(ModelCache::open(dir.path().join("c.db")).unwrap()));
        let remote = Rc::new(FakeRemote::default());
        let options = CacheOptions::new("test").with_filter(|table, row| {
            table != "nansuru" || row.get("generated") == Some(&json!(true))
        });
        (
            remote.clone(),
            CachedStore::new(remote, Some(cache), options),
            dir,
        )
    }

    #[tokio::test]
    async fn caches_reads_and_respects_filters() {
        let (remote, store, _dir) = setup();
        {
            remote
                .rows
                .borrow_mut()
                .insert("user/u".into(), row(json!({"id": "u", "name": "A"})));
            remote.rows.borrow_mut().insert(
                "nansuru/x".into(),
                row(json!({"id": "x", "generated": false})),
            );
            assert_eq!(
                store
                    .load("db", "user", "u", ReadOptions::default())
                    .await
                    .unwrap()
                    .unwrap()["name"],
                "A"
            );
            assert_eq!(
                store
                    .load("db", "user", "u", ReadOptions::default())
                    .await
                    .unwrap()
                    .unwrap()["name"],
                "A"
            );
            assert_eq!(remote.gets.get(), 1, "second read comes from the cache");
            store
                .load("db", "nansuru", "x", ReadOptions::default())
                .await
                .unwrap();
            store
                .load("db", "nansuru", "x", ReadOptions::default())
                .await
                .unwrap();
            assert_eq!(remote.gets.get(), 3, "filtered rows are not cached");
            // reload bypasses the cache
            store
                .load("db", "user", "u", ReadOptions::reload())
                .await
                .unwrap();
            assert_eq!(remote.gets.get(), 4);
        }
    }

    #[tokio::test]
    async fn falls_back_offline_and_drops_stale_sessions() {
        let (remote, store, _dir) = setup();
        {
            store
                .save("db", "user", "u", row(json!({"id": "u", "v": 1})))
                .await
                .unwrap();
            remote.offline.set(true);
            let options = ReadOptions {
                reload: true,
                offline_fallback: true,
                timeout: None,
            };
            assert_eq!(
                store
                    .load("db", "user", "u", options)
                    .await
                    .unwrap()
                    .unwrap()["v"],
                1
            );
            let rows = store
                .query("db", "user", &Query::new(), options)
                .await
                .unwrap();
            assert_eq!(rows.len(), 1);
            assert!(
                store
                    .load("db", "user", "u", ReadOptions::reload())
                    .await
                    .is_err()
            );
            store.clear_cache().await.unwrap();
            assert_eq!(store.cached("db", "user", "u").await, None);
        }
    }

    #[tokio::test]
    async fn removes_rows_rejected_by_the_filter() {
        let (remote, store, _dir) = setup();
        {
            remote.rows.borrow_mut().insert(
                "nansuru/x".into(),
                row(json!({"id": "x", "generated": true})),
            );
            store
                .load("db", "nansuru", "x", ReadOptions::default())
                .await
                .unwrap();
            assert!(store.cached("db", "nansuru", "x").await.is_some());
            store
                .save(
                    "db",
                    "nansuru",
                    "x",
                    row(json!({"id": "x", "generated": false})),
                )
                .await
                .unwrap();
            assert!(store.cached("db", "nansuru", "x").await.is_none());
        }
    }
}
