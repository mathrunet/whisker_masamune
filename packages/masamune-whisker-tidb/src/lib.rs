//! TiDB databases through the Masamune Cloudflare worker (`masamune_cloudflare_tidb`), the same
//! protocol as Flutter's `masamune_model_tidb`.
//!
//! The worker exposes `/tidb/database/{database}/{table}[/{id}]`. [`Tidb`] implements
//! [`RemoteStore`](masamune_whisker_model::RemoteStore), so wrap it in a [`CachedTidb`] for the
//! on-device libsql cache. TiDB-only operations (counts, vector search, bulk updates and
//! deletes) are methods of [`Tidb`].
//!
//! - Key aliases: `@uid` is the `id` column and `@time` the `updated_at` column.
//! - Keys starting with `@` are not written.
//! - Requests go through a [`Functions`] handle (usually `CloudflareFunctionsAdapter`), so the
//!   crate depends only on the adapter interface.
//! - Requests are retried on 429, 500, 502, 503, 504 and network errors (200 ms … 5 s with
//!   jitter), like Flutter.

use std::collections::HashMap;
use std::time::Duration;

pub use masamune_whisker_functions::Functions;
use masamune_whisker_functions::Method;
use masamune_whisker_model::{
    ApiError, ApiResult, CachedStore, Filter, Order, Query, RemoteStore, RestDatabase, RetryPolicy,
    Row, data_rows, retry,
};
use serde_json::{Value, json};

/// A [`Tidb`] store with the on-device cache.
pub type CachedTidb = CachedStore<Tidb>;

/// Maps key aliases to TiDB columns (`@uid` → `id`, `@time` → `updated_at`).
pub fn column_key(key: &str) -> &str {
    match key {
        "@uid" => "id",
        "@time" => "updated_at",
        other => other,
    }
}

/// Whether TiDB requests should be retried (429, 500, 502, 503, 504 and network failures).
pub fn is_retryable(error: &ApiError) -> bool {
    match error {
        ApiError::Network(_) | ApiError::Timeout => true,
        ApiError::Http { status, .. } => matches!(status, 429 | 500 | 502 | 503 | 504),
        _ => false,
    }
}

/// Settings of [`Tidb`].
#[derive(Clone, Debug)]
pub struct TidbConfig {
    /// Worker action (path segment before `/database`).
    pub action: String,
    /// Database name prefix.
    pub prefix: Option<String>,
    /// Retries of reads and writes.
    pub retry: RetryPolicy,
}

impl Default for TidbConfig {
    fn default() -> Self {
        Self {
            action: "tidb".into(),
            prefix: None,
            retry: RetryPolicy::transient(&[200, 400, 800, 1600, 3200, 5000])
                .with_jitter()
                .retry_if(is_retryable),
        }
    }
}

/// TiDB over the Masamune worker.
#[derive(Clone, Debug)]
pub struct Tidb {
    rest: RestDatabase,
    config: TidbConfig,
}

fn map_query(query: &Query) -> Query {
    Query {
        filters: query
            .filters
            .iter()
            .map(|f| Filter::new(f.kind, column_key(&f.key), f.value.clone()))
            .collect(),
        order_by: query
            .order_by
            .iter()
            .map(|o| Order {
                key: column_key(&o.key).to_owned(),
                descending: o.descending,
            })
            .collect(),
        limit: query.limit,
    }
}

fn writable(mut row: Row) -> Row {
    let time = row.get("@time").cloned();
    row.retain(|key, _| !key.starts_with('@'));
    if let Some(time) = time
        && !row.contains_key("updated_at")
    {
        row.insert("updated_at".into(), time);
    }
    row
}

impl Tidb {
    pub fn new(functions: Functions) -> Self {
        Self::with_config(functions, TidbConfig::default()).expect("default tidb config")
    }

    pub fn with_config(functions: Functions, config: TidbConfig) -> ApiResult<Self> {
        let rest = RestDatabase::new(functions, config.action.clone())
            .with_prefix(config.prefix.as_deref())?;
        Ok(Self { rest, config })
    }

    pub fn rest(&self) -> &RestDatabase {
        &self.rest
    }

    /// Number of rows matching `filters`.
    pub async fn count(&self, database: &str, table: &str, filters: &[Filter]) -> ApiResult<u64> {
        let query = map_query(&Query {
            filters: filters.to_vec(),
            ..Query::default()
        });
        retry(&self.config.retry, || {
            self.rest
                .count(database, table, &query, Duration::from_secs(20))
        })
        .await
    }

    /// Rows whose vector column `key` is nearest to `vector` (1 ≤ `limit` ≤ 100). Cannot be
    /// combined with ordering.
    pub async fn nearest(
        &self,
        database: &str,
        table: &str,
        key: &str,
        vector: &[f32],
        filters: &[Filter],
        limit: usize,
    ) -> ApiResult<Vec<Row>> {
        if !(1..=100).contains(&limit) {
            return Err(ApiError::Other(
                "nearest limit must be between 1 and 100".into(),
            ));
        }
        let mut params = vec![
            (
                "nearest",
                json!({ "key": column_key(key), "value": vector }).to_string(),
            ),
            ("limit", limit.to_string()),
        ];
        let query = map_query(&Query {
            filters: filters.to_vec(),
            ..Query::default()
        });
        if let Some(filters) = query.where_json() {
            params.push(("where", filters));
        }
        if let Some(prefix) = self.rest.prefix() {
            params.push(("prefix", prefix.to_owned()));
        }
        let request = self
            .rest
            .request(Method::GET, &["database", database, table], &params);
        retry(&self.config.retry, || {
            let request = request.clone();
            async move {
                let response = self.rest.send(request, Duration::from_secs(20)).await?;
                Ok(data_rows(&response))
            }
        })
        .await
    }

    /// Updates every row matching `filters` with `value` (`PUT` with `where`).
    pub async fn update_where(
        &self,
        database: &str,
        table: &str,
        filters: &[Filter],
        value: Row,
    ) -> ApiResult<Vec<Row>> {
        let filters = map_query(&Query {
            filters: filters.to_vec(),
            ..Query::default()
        })
        .filters;
        let value = writable(value);
        retry(&self.config.retry, || {
            self.rest
                .put(database, table, None, &filters, value.clone())
        })
        .await
    }

    /// Deletes every row matching `filters` (`DELETE` with `where`).
    pub async fn delete_where(
        &self,
        database: &str,
        table: &str,
        filters: &[Filter],
    ) -> ApiResult<()> {
        let filters = map_query(&Query {
            filters: filters.to_vec(),
            ..Query::default()
        })
        .filters;
        retry(&self.config.retry, || {
            self.rest.delete(database, table, None, &filters)
        })
        .await
    }

    /// Reads several rows of one table by id in one request (`whereIn id`).
    pub async fn preload(
        &self,
        database: &str,
        table: &str,
        ids: &[String],
    ) -> ApiResult<HashMap<String, Row>> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let query = Query::new()
            .where_in(
                "id",
                Value::Array(ids.iter().cloned().map(Value::String).collect()),
            )
            .limit(ids.len());
        let rows =
            RemoteStore::query(self, database, table, &query, Duration::from_secs(20)).await?;
        Ok(rows
            .into_iter()
            .filter_map(|row| Some((row.get("id")?.as_str()?.to_owned(), row)))
            .collect())
    }
}

impl RemoteStore for Tidb {
    async fn get(
        &self,
        database: &str,
        table: &str,
        id: &str,
        timeout: Duration,
    ) -> ApiResult<Option<Row>> {
        retry(&self.config.retry, || {
            self.rest.get(database, table, id, timeout)
        })
        .await
    }

    async fn query(
        &self,
        database: &str,
        table: &str,
        query: &Query,
        timeout: Duration,
    ) -> ApiResult<Vec<Row>> {
        let query = map_query(query);
        retry(&self.config.retry, || {
            self.rest.query(database, table, &query, timeout)
        })
        .await
    }

    async fn save(&self, database: &str, table: &str, id: &str, row: Row) -> ApiResult<()> {
        let mut row = writable(row);
        row.insert("id".into(), Value::String(id.to_owned()));
        retry(&self.config.retry, || {
            self.rest.post(database, table, Some(id), row.clone())
        })
        .await?;
        Ok(())
    }

    async fn delete(&self, database: &str, table: &str, id: &str) -> ApiResult<()> {
        retry(&self.config.retry, || {
            self.rest.delete(database, table, Some(id), &[])
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use masamune_whisker_functions::{Auth, RestApiFunctionsAdapter};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};
    use wiremock::matchers::{body_json, method, path, query_param};
    use wiremock::{Mock, MockServer, Request, ResponseTemplate};

    fn tidb(server: &MockServer) -> Tidb {
        let config = TidbConfig {
            prefix: Some("dev".into()),
            retry: RetryPolicy::transient(&[1, 1]).retry_if(is_retryable),
            ..TidbConfig::default()
        };
        Tidb::with_config(
            Functions::new(RestApiFunctionsAdapter::new(server.uri()).with_auth(Auth::fixed("t"))),
            config,
        )
        .unwrap()
    }

    #[tokio::test]
    async fn speaks_the_worker_protocol() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/tidb/database/app/posts"))
            .and(query_param(
                "where",
                r#"[{"type":"greaterThan","key":"updated_at","value":5}]"#,
            ))
            .and(query_param(
                "orderBy",
                r#"[{"key":"id","descending":true}]"#,
            ))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"data": [{"id": "p1", "price": "1.50"}]})),
            )
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/tidb/database/app/posts/p1"))
            .and(body_json(
                json!({"prefix": "dev_", "value": {"id": "p1", "title": "a", "updated_at": 9}}),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": []})))
            .mount(&server)
            .await;
        Mock::given(method("PUT"))
            .and(path("/tidb/database/app/posts"))
            .and(body_json(json!({"prefix": "dev_", "where": [{"type": "equalTo", "key": "id", "value": "p1"}], "value": {"title": "b"}})))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": [{"id": "p1"}]})))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/tidb/database/app/posts"))
            .and(query_param(
                "nearest",
                r#"{"key":"embedding","value":[0.5,1.0]}"#,
            ))
            .and(query_param("limit", "3"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": [{"id": "p2"}]})))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/tidb/database/app/posts"))
            .and(query_param("count", "true"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": 7})))
            .mount(&server)
            .await;
        let db = tidb(&server);
        let t = Duration::from_secs(5);
        let q = Query::new().greater_than("@time", 5).order_by("@uid", true);
        let rows = db.query("app", "posts", &q, t).await.unwrap();
        assert_eq!(rows[0]["price"], "1.50", "DECIMAL stays a string");
        let row = json!({"title": "a", "@uid": "p1", "@time": 9})
            .as_object()
            .unwrap()
            .clone();
        db.save("app", "posts", "p1", row).await.unwrap();
        let title = json!({"title": "b"}).as_object().unwrap().clone();
        let filter = [Filter::new(
            masamune_whisker_model::FilterKind::EqualTo,
            "@uid",
            Some(json!("p1")),
        )];
        assert_eq!(
            db.update_where("app", "posts", &filter, title)
                .await
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            db.nearest("app", "posts", "embedding", &[0.5, 1.0], &[], 3)
                .await
                .unwrap()[0]["id"],
            "p2"
        );
        assert!(
            db.nearest("app", "posts", "embedding", &[0.5], &[], 0)
                .await
                .is_err()
        );
        assert_eq!(db.count("app", "posts", &[]).await.unwrap(), 7);
        assert!(
            db.delete_where("app", "posts", &[]).await.is_err(),
            "bulk delete needs filters"
        );
    }

    #[tokio::test]
    async fn retries_rate_limits_but_not_denials() {
        let server = MockServer::start().await;
        let attempts = Arc::new(AtomicU32::new(0));
        let counter = attempts.clone();
        Mock::given(method("GET"))
            .and(path("/tidb/database/app/posts/a"))
            .respond_with(move |_: &Request| {
                if counter.fetch_add(1, Ordering::SeqCst) == 0 {
                    ResponseTemplate::new(429)
                } else {
                    ResponseTemplate::new(200).set_body_json(json!({"data": [{"id": "a"}]}))
                }
            })
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/tidb/database/app/posts/b"))
            .respond_with(ResponseTemplate::new(403).set_body_json(json!({"error": "denied"})))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/tidb/database/app/posts/c"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": []})))
            .mount(&server)
            .await;
        let db = tidb(&server);
        let t = Duration::from_secs(5);
        assert!(db.get("app", "posts", "a", t).await.unwrap().is_some());
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
        assert_eq!(
            db.get("app", "posts", "b", t).await.unwrap_err().status(),
            Some(403)
        );
        assert_eq!(db.get("app", "posts", "c", t).await.unwrap(), None);
    }

    /// Against a real worker: `TIDB_E2E_BASE=https://… TIDB_E2E_ID_TOKEN=… cargo test -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn e2e_round_trip() {
        let (Ok(base), Ok(token)) = (
            std::env::var("TIDB_E2E_BASE"),
            std::env::var("TIDB_E2E_ID_TOKEN"),
        ) else {
            return;
        };
        let db = Tidb::new(Functions::new(
            RestApiFunctionsAdapter::new(base).with_auth(Auth::fixed(token)),
        ));
        let table = std::env::var("TIDB_E2E_TABLE").unwrap_or_else(|_| "whisker_e2e".into());
        let database = std::env::var("TIDB_E2E_DATABASE").unwrap_or_else(|_| "e2e".into());
        let row = json!({"title": "e2e"}).as_object().unwrap().clone();
        db.save(&database, &table, "e2e-1", row).await.unwrap();
        let read = db
            .get(&database, &table, "e2e-1", Duration::from_secs(20))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(read["title"], "e2e");
        db.delete(&database, &table, "e2e-1").await.unwrap();
    }
}
