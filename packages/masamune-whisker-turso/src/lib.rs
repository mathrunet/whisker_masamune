//! Turso databases through the Masamune Cloudflare worker (`masamune_cloudflare_turso`).
//!
//! The worker exposes each database (usually one per user) at
//! `/turso/database/{database}/{table}[/{id}]`. [`Turso`] implements
//! [`RemoteStore`](masamune_whisker_model::RemoteStore) over that REST API, so wrap it in a
//! [`CachedTurso`] for the on-device libsql cache:
//!
//! Requests go through a [`Functions`] handle, so the crate depends only on the adapter
//! interface; the app chooses the backend (usually `CloudflareFunctionsAdapter`):
//!
//! ```ignore
//! let functions = Functions::new(CloudflareFunctionsAdapter::new(EDGE).with_auth(firebase_auth()));
//! let turso = Turso::new(functions);
//! let store = CachedTurso::new(turso, Some(ModelCache::shared()?), CacheOptions::new("turso"));
//! let user = store.load(&uid, "user", &uid, ReadOptions::default()).await?;
//! ```
//!
//! Direct libsql connections (`readMode: direct`) are not used; [`Turso::token`] is available for
//! apps that need the token endpoint.

use std::collections::HashMap;
use std::time::Duration;

pub use masamune_whisker_functions::Functions;
use masamune_whisker_functions::Method;
use masamune_whisker_model::{
    ApiError, ApiResult, CachedStore, Query, RemoteStore, RestDatabase, RetryPolicy, Row, now_ms,
    retry,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

/// A [`Turso`] store with the on-device cache.
pub type CachedTurso = CachedStore<Turso>;

/// Settings of [`Turso`].
#[derive(Clone, Debug)]
pub struct TursoConfig {
    /// Worker action (path segment before `/database`).
    pub action: String,
    /// Database name prefix (the worker adds `dev_` itself for dev deployments).
    pub prefix: Option<String>,
    /// Retries of writes (5xx and network errors, 250 ms … 8 s by default).
    pub write_retry: RetryPolicy,
    /// Decodes string columns that hold JSON objects or arrays (how Turso returns JSON values).
    pub decode_json_strings: bool,
    /// Columns per table stored as SQLite integers that should read as booleans.
    pub bool_fields: HashMap<String, Vec<String>>,
}

impl Default for TursoConfig {
    fn default() -> Self {
        Self {
            action: "turso".into(),
            prefix: None,
            write_retry: RetryPolicy::transient(&[250, 500, 1000, 2000, 4000, 8000]),
            decode_json_strings: true,
            bool_fields: HashMap::new(),
        }
    }
}

/// Token for direct libsql access (`POST /turso/token/database/{db}`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TursoToken {
    pub token: String,
    #[serde(default)]
    pub expires_at: Option<i64>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub read_mode: Option<String>,
    #[serde(default)]
    pub write_mode: Option<String>,
}

/// Table permissions requested with a token.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TokenScope {
    pub table: String,
    pub operations: Vec<String>,
}

/// Turso over the Masamune worker.
#[derive(Clone, Debug)]
pub struct Turso {
    rest: RestDatabase,
    config: TursoConfig,
}

impl Turso {
    pub fn new(functions: Functions) -> Self {
        Self::with_config(functions, TursoConfig::default()).expect("default turso config")
    }

    pub fn with_config(functions: Functions, config: TursoConfig) -> ApiResult<Self> {
        let rest = RestDatabase::new(functions, config.action.clone())
            .with_prefix(config.prefix.as_deref())?;
        Ok(Self { rest, config })
    }

    pub fn rest(&self) -> &RestDatabase {
        &self.rest
    }

    /// Normalizes a row read from Turso (JSON strings, integer booleans).
    pub fn decode(&self, table: &str, row: Row) -> Row {
        let mut row = if self.config.decode_json_strings {
            decode_json_strings(row)
        } else {
            row
        };
        if let Some(fields) = self.config.bool_fields.get(table) {
            for field in fields {
                if let Some(value) = row.get_mut(field) {
                    *value = Value::Bool(match value {
                        Value::Bool(b) => *b,
                        Value::Number(n) => n.as_f64().is_some_and(|v| v != 0.0),
                        Value::String(s) => s == "true" || s == "1",
                        _ => false,
                    });
                }
            }
        }
        row
    }

    /// Requests a token for direct access to `database`.
    pub async fn token(
        &self,
        database: &str,
        scopes: &[TokenScope],
        ttl_seconds: u32,
    ) -> ApiResult<TursoToken> {
        let mut body = json!({ "targets": scopes, "ttlSeconds": ttl_seconds });
        if let Some(prefix) = self.rest.prefix() {
            body["prefix"] = Value::String(prefix.to_owned());
        }
        let request = self
            .rest
            .request(Method::POST, &["token", "database", database], &[])
            .with_body(body);
        let response = self.rest.send(request, Duration::from_secs(20)).await?;
        serde_json::from_value(response).map_err(|e| ApiError::Decode(e.to_string()))
    }
}

/// Row written by [`Turso::save`]: keys starting with `@` are dropped, `id` is set, `created_at`
/// is kept when present (otherwise now) and `updated_at` is now.
pub fn build_save_row(id: &str, mut row: Row) -> Row {
    row.retain(|key, _| !key.starts_with('@'));
    let now = now_ms();
    row.insert("id".into(), Value::String(id.to_owned()));
    if !matches!(row.get("created_at"), Some(v) if !v.is_null()) {
        row.insert("created_at".into(), json!(now));
    }
    row.insert("updated_at".into(), json!(now));
    row
}

/// Decodes string values that look like JSON objects or arrays.
pub fn decode_json_strings(row: Row) -> Row {
    row.into_iter()
        .map(|(key, value)| {
            let value = match value {
                Value::String(s) => {
                    let trimmed = s.trim();
                    let looks_json = (trimmed.starts_with('{') && trimmed.ends_with('}'))
                        || (trimmed.starts_with('[') && trimmed.ends_with(']'));
                    if looks_json {
                        serde_json::from_str::<Value>(trimmed).unwrap_or(Value::String(s))
                    } else {
                        Value::String(s)
                    }
                }
                other => other,
            };
            (key, value)
        })
        .collect::<Map<String, Value>>()
}

impl RemoteStore for Turso {
    async fn get(
        &self,
        database: &str,
        table: &str,
        id: &str,
        timeout: Duration,
    ) -> ApiResult<Option<Row>> {
        Ok(self
            .rest
            .get(database, table, id, timeout)
            .await?
            .map(|row| self.decode(table, row)))
    }

    async fn query(
        &self,
        database: &str,
        table: &str,
        query: &Query,
        timeout: Duration,
    ) -> ApiResult<Vec<Row>> {
        Ok(self
            .rest
            .query(database, table, query, timeout)
            .await?
            .into_iter()
            .map(|row| self.decode(table, row))
            .collect())
    }

    async fn save(&self, database: &str, table: &str, id: &str, row: Row) -> ApiResult<()> {
        let row = build_save_row(id, row);
        retry(&self.config.write_retry, || {
            let row = row.clone();
            async move {
                self.rest
                    .post(database, table, None, row)
                    .await
                    .inspect_err(|error| {
                        log::warn!("turso save failed: table={table} error={error}");
                    })
            }
        })
        .await?;
        Ok(())
    }

    async fn delete(&self, database: &str, table: &str, id: &str) -> ApiResult<()> {
        self.rest.delete(database, table, Some(id), &[]).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use masamune_whisker_functions::{Auth, RestApiFunctionsAdapter};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, Request, ResponseTemplate};

    #[test]
    fn builds_rows() {
        let row = json!({"name": "a", "@uid": "x", "created_at": 5})
            .as_object()
            .unwrap()
            .clone();
        let row = build_save_row("u", row);
        assert_eq!(row["id"], "u");
        assert_eq!(row["created_at"], 5);
        assert!(row.get("@uid").is_none());
        assert!(row["updated_at"].as_i64().unwrap() > 0);
        let decoded = decode_json_strings(
            json!({"a": "{\"b\":1}", "c": "x"})
                .as_object()
                .unwrap()
                .clone(),
        );
        assert_eq!(decoded["a"]["b"], 1);
        assert_eq!(decoded["c"], "x");
    }

    #[tokio::test]
    async fn retries_writes_and_decodes_reads() {
        let server = MockServer::start().await;
        let attempts = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let counter = attempts.clone();
        Mock::given(method("POST"))
            .and(path("/turso/database/u/user"))
            .respond_with(move |_: &Request| {
                if counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                    ResponseTemplate::new(503)
                } else {
                    ResponseTemplate::new(200).set_body_json(json!({"data": []}))
                }
            })
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/turso/database/u/user/u"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"data": [{"id": "u", "deleted": 1, "meta": "[1,2]"}]})),
            )
            .mount(&server)
            .await;
        let mut config = TursoConfig {
            write_retry: RetryPolicy::transient(&[1, 1]),
            ..TursoConfig::default()
        };
        config
            .bool_fields
            .insert("user".into(), vec!["deleted".into()]);
        let turso = Turso::with_config(
            Functions::new(RestApiFunctionsAdapter::new(server.uri()).with_auth(Auth::fixed("t"))),
            config,
        )
        .unwrap();
        turso.save("u", "user", "u", Row::new()).await.unwrap();
        assert_eq!(attempts.load(std::sync::atomic::Ordering::SeqCst), 2);
        let row = turso
            .get("u", "user", "u", Duration::from_secs(5))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row["deleted"], true);
        assert_eq!(row["meta"][1], 2);
    }
}
