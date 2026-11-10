//! On-device document cache backed by a local libsql (SQLite) database.
//!
//! All statements run on one background thread that owns the connection, so callers on the
//! UI thread only await a oneshot reply and never block on disk I/O. libsql's local API is
//! synchronous under its async surface, so the worker drives it with a plain executor and
//! does not need a tokio runtime.
//!
//! Documents are keyed by `(scope, collection, id)`. `scope` separates backends, databases
//! and signed-in users (for example `turso:dev_/<uid>` or `firestore:production/<uid>`) so a
//! sign-out can drop exactly one user's data with [`ModelCache::clear_scope`].

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use futures_channel::oneshot;
use futures_executor::block_on;
use libsql::{Builder, Connection, Value as SqlValue, params};

use crate::query::{Query, Row};

/// A cached document.
#[derive(Debug, Clone, PartialEq)]
pub struct CachedDocument {
    pub id: String,
    pub data: Row,
    /// Server-side update time in milliseconds, when the backend provides one.
    pub updated_at: Option<i64>,
    /// When this entry was written to the cache, in milliseconds since the Unix epoch.
    pub cached_at: i64,
}

/// Failure of a cache operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheError(pub String);

impl std::fmt::Display for CacheError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "model cache: {}", self.0)
    }
}

impl std::error::Error for CacheError {}

impl From<libsql::Error> for CacheError {
    fn from(error: libsql::Error) -> Self {
        CacheError(error.to_string())
    }
}

pub type CacheResult<T> = Result<T, CacheError>;

type Job = Box<dyn FnOnce(&Connection) + Send>;

/// File name used by [`ModelCache::shared`] under the platform support directory.
pub const SHARED_CACHE_FILE: &str = "masamune_model_cache.db";

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS mw_documents (
    scope TEXT NOT NULL,
    collection TEXT NOT NULL,
    id TEXT NOT NULL,
    data TEXT NOT NULL,
    updated_at INTEGER,
    cached_at INTEGER NOT NULL,
    PRIMARY KEY (scope, collection, id)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS mw_meta (
    key TEXT PRIMARY KEY,
    value TEXT
) WITHOUT ROWID;
";

/// Handle to the cache database. Cheap to share by reference; see [`ModelCache::shared`].
pub struct ModelCache {
    jobs: Mutex<Option<mpsc::Sender<Job>>>,
    worker: Mutex<Option<std::thread::JoinHandle<()>>>,
    path: PathBuf,
}

impl Drop for ModelCache {
    /// Stops the worker and waits until the connection is closed, so the file can be reopened.
    fn drop(&mut self) {
        self.jobs.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(worker) = self.worker.lock().unwrap_or_else(|e| e.into_inner()).take() {
            let _ = worker.join();
        }
    }
}

static SHARED: OnceLock<ModelCache> = OnceLock::new();
static SHARED_INIT: Mutex<()> = Mutex::new(());

impl ModelCache {
    /// Opens (or creates) the cache database at `path` and starts its worker thread.
    pub fn open(path: impl Into<PathBuf>) -> CacheResult<Self> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| CacheError(e.to_string()))?;
        }
        let (jobs, receiver) = mpsc::channel::<Job>();
        let (ready_tx, ready_rx) = mpsc::channel::<CacheResult<()>>();
        let db_path = path.clone();
        let worker = std::thread::Builder::new()
            .name("masamune-model-cache".into())
            .spawn(move || {
                let opened = block_on(async {
                    let db = Builder::new_local(&db_path).build().await?;
                    let conn = db.connect()?;
                    conn.busy_timeout(std::time::Duration::from_secs(5))?;
                    // `journal_mode` returns a row, so it has to go through `query`.
                    conn.query("PRAGMA journal_mode=WAL", ()).await?;
                    conn.execute("PRAGMA synchronous=NORMAL", ()).await?;
                    conn.execute_batch(SCHEMA).await?;
                    Ok::<_, libsql::Error>((db, conn))
                });
                let (_db, conn) = match opened {
                    Ok(opened) => {
                        let _ = ready_tx.send(Ok(()));
                        opened
                    }
                    Err(error) => {
                        let _ = ready_tx.send(Err(error.into()));
                        return;
                    }
                };
                while let Ok(job) = receiver.recv() {
                    job(&conn);
                }
            })
            .map_err(|e| CacheError(e.to_string()))?;
        ready_rx
            .recv()
            .map_err(|_| CacheError("cache worker exited during open".into()))??;
        Ok(Self {
            jobs: Mutex::new(Some(jobs)),
            worker: Mutex::new(Some(worker)),
            path,
        })
    }

    /// The app-wide cache in the platform support directory, opened on first use.
    ///
    /// Call it first on the UI thread (for example while preparing the app): the support
    /// directory comes from the `whisker-paths` native module, which only answers on the UI
    /// thread. Resolved off the UI thread it falls back to a temporary directory the OS may
    /// clear, and that fallback is cached for the rest of the process.
    pub fn shared() -> CacheResult<&'static ModelCache> {
        if let Some(cache) = SHARED.get() {
            return Ok(cache);
        }
        let _guard = SHARED_INIT.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(cache) = SHARED.get() {
            return Ok(cache);
        }
        let dir = whisker_paths::support_dir();
        if dir
            .components()
            .any(|c| c.as_os_str() == "whisker-paths-fallback")
        {
            log::warn!(
                "model cache: whisker-paths is unavailable on this thread; using a temporary directory"
            );
        }
        let cache = ModelCache::open(dir.join(SHARED_CACHE_FILE))?;
        Ok(SHARED.get_or_init(|| cache))
    }

    /// Location of the database file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Runs `f` on the worker thread with the connection and awaits its result.
    pub async fn run<T, F>(&self, f: F) -> CacheResult<T>
    where
        T: Send + 'static,
        F: FnOnce(&Connection) -> CacheResult<T> + Send + 'static,
    {
        let (tx, rx) = oneshot::channel();
        let job: Job = Box::new(move |conn| {
            let _ = tx.send(f(conn));
        });
        self.jobs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .ok_or_else(|| CacheError("cache is closed".into()))?
            .send(job)
            .map_err(|_| CacheError("cache worker is not running".into()))?;
        rx.await
            .map_err(|_| CacheError("cache worker dropped the request".into()))?
    }

    /// Reads one document.
    pub async fn get(
        &self,
        scope: &str,
        collection: &str,
        id: &str,
    ) -> CacheResult<Option<CachedDocument>> {
        let (scope, collection, id) = (scope.to_owned(), collection.to_owned(), id.to_owned());
        self.run(move |conn| {
            let mut rows = select(
                conn,
                "SELECT id, data, updated_at, cached_at FROM mw_documents WHERE scope = ?1 AND collection = ?2 AND id = ?3",
                params![scope, collection, id],
            )?;
            Ok(rows.pop())
        })
        .await
    }

    /// Writes (replaces) one document.
    pub async fn put(
        &self,
        scope: &str,
        collection: &str,
        id: &str,
        data: &Row,
        updated_at: Option<i64>,
    ) -> CacheResult<()> {
        let entries = vec![(id.to_owned(), data.clone(), updated_at)];
        self.put_many(scope, collection, entries).await
    }

    /// Writes several documents of one collection in a single transaction.
    pub async fn put_many(
        &self,
        scope: &str,
        collection: &str,
        entries: Vec<(String, Row, Option<i64>)>,
    ) -> CacheResult<()> {
        if entries.is_empty() {
            return Ok(());
        }
        let (scope, collection) = (scope.to_owned(), collection.to_owned());
        self.run(move |conn| {
            let now = now_ms();
            block_on(async {
                let tx = conn.transaction().await?;
                for (id, data, updated_at) in entries {
                    let json = serde_json::to_string(&data).unwrap_or_else(|_| "{}".into());
                    tx.execute(
                        "INSERT OR REPLACE INTO mw_documents (scope, collection, id, data, updated_at, cached_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                        params![scope.clone(), collection.clone(), id, json, updated_at, now],
                    )
                    .await?;
                }
                tx.commit().await
            })?;
            Ok(())
        })
        .await
    }

    /// Removes one document.
    pub async fn remove(&self, scope: &str, collection: &str, id: &str) -> CacheResult<()> {
        let (scope, collection, id) = (scope.to_owned(), collection.to_owned(), id.to_owned());
        self.run(move |conn| {
            block_on(conn.execute(
                "DELETE FROM mw_documents WHERE scope = ?1 AND collection = ?2 AND id = ?3",
                params![scope, collection, id],
            ))?;
            Ok(())
        })
        .await
    }

    /// Every cached document of a collection, in id order.
    pub async fn list(&self, scope: &str, collection: &str) -> CacheResult<Vec<CachedDocument>> {
        let (scope, collection) = (scope.to_owned(), collection.to_owned());
        self.run(move |conn| {
            select(
                conn,
                "SELECT id, data, updated_at, cached_at FROM mw_documents WHERE scope = ?1 AND collection = ?2 ORDER BY id",
                params![scope, collection],
            )
        })
        .await
    }

    /// Cached documents of a collection that match `query`, sorted and limited like the server.
    pub async fn query(
        &self,
        scope: &str,
        collection: &str,
        query: &Query,
    ) -> CacheResult<Vec<CachedDocument>> {
        let docs = self.list(scope, collection).await?;
        Ok(query.apply(docs, |d| &d.data))
    }

    /// Collections under `scope` whose name starts with `prefix`.
    pub async fn list_collections(&self, scope: &str, prefix: &str) -> CacheResult<Vec<String>> {
        let scope = scope.to_owned();
        let pattern = format!(
            "{}%",
            prefix
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_")
        );
        self.run(move |conn| {
            block_on(async {
                let mut rows = conn
                    .query(
                        "SELECT DISTINCT collection FROM mw_documents WHERE scope = ?1 AND collection LIKE ?2 ESCAPE '\\'",
                        params![scope, pattern],
                    )
                    .await?;
                let mut out = Vec::new();
                while let Some(row) = rows.next().await? {
                    if let SqlValue::Text(name) = row.get_value(0)? {
                        out.push(name);
                    }
                }
                Ok::<_, libsql::Error>(out)
            })
            .map_err(CacheError::from)
        })
        .await
    }

    /// Removes every document of a collection.
    pub async fn clear_collection(&self, scope: &str, collection: &str) -> CacheResult<()> {
        let (scope, collection) = (scope.to_owned(), collection.to_owned());
        self.run(move |conn| {
            block_on(conn.execute(
                "DELETE FROM mw_documents WHERE scope = ?1 AND collection = ?2",
                params![scope, collection],
            ))?;
            Ok(())
        })
        .await
    }

    /// Removes every document stored under `scope` (all collections).
    pub async fn clear_scope(&self, scope: &str) -> CacheResult<()> {
        let scope = scope.to_owned();
        self.run(move |conn| {
            block_on(conn.execute("DELETE FROM mw_documents WHERE scope = ?1", params![scope]))?;
            Ok(())
        })
        .await
    }

    /// Removes every document whose scope starts with `prefix` (for example one backend).
    pub async fn clear_scope_prefix(&self, prefix: &str) -> CacheResult<()> {
        let pattern = format!(
            "{}%",
            prefix
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_")
        );
        self.run(move |conn| {
            block_on(conn.execute(
                "DELETE FROM mw_documents WHERE scope LIKE ?1 ESCAPE '\\'",
                params![pattern],
            ))?;
            Ok(())
        })
        .await
    }

    /// Removes everything.
    pub async fn clear_all(&self) -> CacheResult<()> {
        self.run(|conn| {
            block_on(conn.execute("DELETE FROM mw_documents", ()))?;
            Ok(())
        })
        .await
    }

    /// Reads a value from the metadata table (for migrations and bookkeeping).
    pub async fn meta(&self, key: &str) -> CacheResult<Option<String>> {
        let key = key.to_owned();
        self.run(move |conn| {
            block_on(async {
                let mut rows = conn
                    .query("SELECT value FROM mw_meta WHERE key = ?1", params![key])
                    .await?;
                match rows.next().await? {
                    Some(row) => match row.get_value(0)? {
                        SqlValue::Text(text) => Ok(Some(text)),
                        _ => Ok(None),
                    },
                    None => Ok(None),
                }
            })
            .map_err(|e: libsql::Error| CacheError::from(e))
        })
        .await
    }

    /// Writes a value to the metadata table.
    pub async fn set_meta(&self, key: &str, value: &str) -> CacheResult<()> {
        let (key, value) = (key.to_owned(), value.to_owned());
        self.run(move |conn| {
            block_on(conn.execute(
                "INSERT OR REPLACE INTO mw_meta (key, value) VALUES (?1, ?2)",
                params![key, value],
            ))?;
            Ok(())
        })
        .await
    }
}

fn select(
    conn: &Connection,
    sql: &str,
    params: impl libsql::params::IntoParams,
) -> CacheResult<Vec<CachedDocument>> {
    block_on(async {
        let mut rows = conn.query(sql, params).await?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            let id = match row.get_value(0)? {
                SqlValue::Text(id) => id,
                _ => continue,
            };
            let data = match row.get_value(1)? {
                SqlValue::Text(json) => serde_json::from_str::<Row>(&json).unwrap_or_default(),
                _ => Row::new(),
            };
            let updated_at = match row.get_value(2)? {
                SqlValue::Integer(v) => Some(v),
                _ => None,
            };
            let cached_at = match row.get_value(3)? {
                SqlValue::Integer(v) => v,
                _ => 0,
            };
            out.push(CachedDocument {
                id,
                data,
                updated_at,
                cached_at,
            });
        }
        Ok::<_, libsql::Error>(out)
    })
    .map_err(CacheError::from)
}

/// Milliseconds since the Unix epoch.
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn row(v: serde_json::Value) -> Row {
        v.as_object().unwrap().clone()
    }

    fn open() -> (tempfile::TempDir, ModelCache) {
        let dir = tempfile::tempdir().unwrap();
        let cache = ModelCache::open(dir.path().join("cache.db")).unwrap();
        (dir, cache)
    }

    #[test]
    fn put_get_remove() {
        let (_dir, cache) = open();
        block_on(async {
            assert_eq!(cache.get("s", "user", "u1").await.unwrap(), None);
            cache
                .put("s", "user", "u1", &row(json!({"name": "A"})), Some(10))
                .await
                .unwrap();
            let doc = cache.get("s", "user", "u1").await.unwrap().unwrap();
            assert_eq!(doc.data["name"], "A");
            assert_eq!(doc.updated_at, Some(10));
            assert!(doc.cached_at > 0);
            // Other scopes do not see it.
            assert_eq!(cache.get("other", "user", "u1").await.unwrap(), None);
            cache.remove("s", "user", "u1").await.unwrap();
            assert_eq!(cache.get("s", "user", "u1").await.unwrap(), None);
        });
    }

    #[test]
    fn query_and_clear() {
        let (_dir, cache) = open();
        block_on(async {
            cache
                .put_many(
                    "turso:/u1",
                    "nansuru",
                    vec![
                        ("a".into(), row(json!({"n": 1, "generated": 1})), None),
                        ("b".into(), row(json!({"n": 3, "generated": 1})), None),
                        ("c".into(), row(json!({"n": 2, "generated": 0})), None),
                    ],
                )
                .await
                .unwrap();
            cache
                .put("turso:/u2", "nansuru", "x", &row(json!({"n": 9})), None)
                .await
                .unwrap();
            let q = Query::new().equal_to("generated", true).order_by("n", true);
            let ids: Vec<_> = cache
                .query("turso:/u1", "nansuru", &q)
                .await
                .unwrap()
                .into_iter()
                .map(|d| d.id)
                .collect();
            assert_eq!(ids, ["b", "a"]);

            cache.clear_scope("turso:/u1").await.unwrap();
            assert!(cache.list("turso:/u1", "nansuru").await.unwrap().is_empty());
            assert_eq!(cache.list("turso:/u2", "nansuru").await.unwrap().len(), 1);

            cache.clear_scope_prefix("turso:").await.unwrap();
            assert!(cache.list("turso:/u2", "nansuru").await.unwrap().is_empty());
        });
    }

    #[test]
    fn persists_across_reopen_and_meta() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cache.db");
        {
            let cache = ModelCache::open(&path).unwrap();
            block_on(async {
                cache
                    .put("s", "c", "1", &row(json!({"v": true})), None)
                    .await
                    .unwrap();
                cache.set_meta("version", "1").await.unwrap();
            });
        }
        let cache = ModelCache::open(&path).unwrap();
        block_on(async {
            assert_eq!(
                cache.get("s", "c", "1").await.unwrap().unwrap().data["v"],
                true
            );
            assert_eq!(cache.meta("version").await.unwrap().as_deref(), Some("1"));
            assert_eq!(cache.meta("missing").await.unwrap(), None);
        });
    }
}
