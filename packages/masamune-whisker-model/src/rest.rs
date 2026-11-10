//! REST client of the Masamune Cloudflare database workers (`masamune_cloudflare_turso`,
//! `masamune_cloudflare_tidb`).
//!
//! Paths are `/{action}/database/{database}/{table}[/{id}]`:
//!
//! - `GET` with `where` / `orderBy` / `limit` / `count` / `prefix` query parameters reads rows.
//! - `POST` `{prefix?, value}` upserts, `PUT` `{prefix?, where?, value}` updates, `DELETE`
//!   `{prefix?, where?}` deletes.
//! - Responses are `{"data": [...]}` (or `{"data": n}` for counts).

use std::time::Duration;

use masamune_whisker_workers::{ApiError, ApiResult, Method, Url, Workers};
use serde_json::{Map, Value, json};

use crate::query::{Filter, Query, Row};

/// Normalizes a database prefix like the workers do: trims, drops trailing `_`, validates
/// `[A-Za-z0-9_-]+` and appends one `_`. Empty prefixes become `None`.
pub fn normalize_prefix(prefix: Option<&str>) -> ApiResult<Option<String>> {
    let Some(prefix) = prefix.map(str::trim).map(|p| p.trim_end_matches('_')) else {
        return Ok(None);
    };
    if prefix.is_empty() {
        return Ok(None);
    }
    if !prefix
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(ApiError::Other(format!(
            "invalid database prefix: {prefix}"
        )));
    }
    Ok(Some(format!("{prefix}_")))
}

/// Rows of a `{"data": [...]}` response.
pub fn data_rows(response: &Value) -> Vec<Row> {
    match response.get("data") {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|row| row.as_object().cloned())
            .collect(),
        Some(Value::Object(row)) => vec![row.clone()],
        _ => Vec::new(),
    }
}

/// Client of one database worker action (`"turso"`, `"tidb"`, …).
#[derive(Clone, Debug)]
pub struct RestDatabase {
    workers: Workers,
    action: String,
    prefix: Option<String>,
    write_timeout: Duration,
}

impl RestDatabase {
    pub fn new(workers: Workers, action: impl Into<String>) -> Self {
        Self {
            workers,
            action: action.into(),
            prefix: None,
            write_timeout: Duration::from_secs(20),
        }
    }

    /// Database name prefix (normalized with [`normalize_prefix`]).
    pub fn with_prefix(mut self, prefix: Option<&str>) -> ApiResult<Self> {
        self.prefix = normalize_prefix(prefix)?;
        Ok(self)
    }

    pub fn with_write_timeout(mut self, timeout: Duration) -> Self {
        self.write_timeout = timeout;
        self
    }

    pub fn workers(&self) -> &Workers {
        &self.workers
    }

    pub fn prefix(&self) -> Option<&str> {
        self.prefix.as_deref()
    }

    fn url(
        &self,
        database: &str,
        table: &str,
        id: Option<&str>,
        query: &[(&str, String)],
    ) -> ApiResult<Url> {
        let mut segments: Vec<&str> = self.action.split('/').filter(|s| !s.is_empty()).collect();
        segments.extend(["database", database, table]);
        if let Some(id) = id {
            segments.push(id);
        }
        let mut query = query.to_vec();
        if let Some(prefix) = &self.prefix {
            query.push(("prefix", prefix.clone()));
        }
        self.workers.url_with(&segments, &query)
    }

    fn body(&self, mut body: Map<String, Value>) -> Value {
        if let Some(prefix) = &self.prefix {
            body.insert("prefix".into(), Value::String(prefix.clone()));
        }
        Value::Object(body)
    }

    /// One row by id (`None` when it does not exist).
    pub async fn get(
        &self,
        database: &str,
        table: &str,
        id: &str,
        timeout: Duration,
    ) -> ApiResult<Option<Row>> {
        let url = self.url(database, table, Some(id), &[])?;
        match self.workers.send(Method::GET, url, None, timeout).await {
            Ok(response) => Ok(data_rows(&response).into_iter().next()),
            Err(error) if error.is_not_found() => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// Rows matching `query`.
    pub async fn query(
        &self,
        database: &str,
        table: &str,
        query: &Query,
        timeout: Duration,
    ) -> ApiResult<Vec<Row>> {
        let mut params = Vec::new();
        if let Some(filters) = query.where_json() {
            params.push(("where", filters));
        }
        if let Some(order) = query.order_json() {
            params.push(("orderBy", order));
        }
        if let Some(limit) = query.limit {
            params.push(("limit", limit.to_string()));
        }
        let url = self.url(database, table, None, &params)?;
        Ok(data_rows(
            &self.workers.send(Method::GET, url, None, timeout).await?,
        ))
    }

    /// Number of rows matching `query` (`count=true`).
    pub async fn count(
        &self,
        database: &str,
        table: &str,
        query: &Query,
        timeout: Duration,
    ) -> ApiResult<u64> {
        let mut params = vec![("count", "true".to_string())];
        if let Some(filters) = query.where_json() {
            params.push(("where", filters));
        }
        let url = self.url(database, table, None, &params)?;
        let response = self.workers.send(Method::GET, url, None, timeout).await?;
        match response.get("data") {
            Some(Value::Number(n)) => n
                .as_u64()
                .ok_or_else(|| ApiError::Decode(format!("count: {n}"))),
            Some(Value::String(s)) => s
                .parse()
                .map_err(|_| ApiError::Decode(format!("count: {s}"))),
            other => Err(ApiError::Decode(format!("count: {other:?}"))),
        }
    }

    /// Upserts `value` (`POST`). `id` in the path is optional; the worker also reads `value.id`.
    pub async fn post(
        &self,
        database: &str,
        table: &str,
        id: Option<&str>,
        value: Row,
    ) -> ApiResult<Vec<Row>> {
        let url = self.url(database, table, id, &[])?;
        let mut body = Map::new();
        body.insert("value".into(), Value::Object(value));
        let response = self
            .workers
            .send(
                Method::POST,
                url,
                Some(&self.body(body)),
                self.write_timeout,
            )
            .await?;
        Ok(data_rows(&response))
    }

    /// Updates rows (`PUT`) selected by `id` or `filters` (one of them is required).
    pub async fn put(
        &self,
        database: &str,
        table: &str,
        id: Option<&str>,
        filters: &[Filter],
        value: Row,
    ) -> ApiResult<Vec<Row>> {
        if id.is_none() && filters.is_empty() {
            return Err(ApiError::Other("update needs an id or filters".into()));
        }
        let url = self.url(database, table, id, &[])?;
        let mut body = Map::new();
        if !filters.is_empty() {
            body.insert("where".into(), json!(filters));
        }
        body.insert("value".into(), Value::Object(value));
        let response = self
            .workers
            .send(Method::PUT, url, Some(&self.body(body)), self.write_timeout)
            .await?;
        Ok(data_rows(&response))
    }

    /// Deletes rows (`DELETE`) selected by `id` or `filters` (one of them is required).
    pub async fn delete(
        &self,
        database: &str,
        table: &str,
        id: Option<&str>,
        filters: &[Filter],
    ) -> ApiResult<()> {
        if id.is_none() && filters.is_empty() {
            return Err(ApiError::Other("delete needs an id or filters".into()));
        }
        let url = self.url(database, table, id, &[])?;
        let mut body = Map::new();
        if !filters.is_empty() {
            body.insert("where".into(), json!(filters));
        }
        self.workers
            .send(
                Method::DELETE,
                url,
                Some(&self.body(body)),
                self.write_timeout,
            )
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use masamune_whisker_workers::AuthProvider;
    use serde_json::json;
    use wiremock::matchers::{body_json, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn normalizes_prefixes() {
        assert_eq!(normalize_prefix(None).unwrap(), None);
        assert_eq!(
            normalize_prefix(Some(" dev__ ")).unwrap(),
            Some("dev_".into())
        );
        assert_eq!(normalize_prefix(Some("")).unwrap(), None);
        assert!(normalize_prefix(Some("a b")).is_err());
    }

    #[tokio::test]
    async fn sends_masamune_requests() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/turso/database/u1/nansuru"))
            .and(query_param(
                "where",
                r#"[{"type":"equalTo","key":"generated","value":true}]"#,
            ))
            .and(query_param(
                "orderBy",
                r#"[{"key":"date","descending":false}]"#,
            ))
            .and(query_param("limit", "3"))
            .and(query_param("prefix", "dev_"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": [{"id": "a"}]})))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/turso/database/u1/user/missing"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/turso/database/u1/user"))
            .and(body_json(json!({"prefix": "dev_", "value": {"id": "u1"}})))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": [{"id": "u1"}]})))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/turso/database/u1/usage"))
            .and(query_param("count", "true"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": 4})))
            .mount(&server)
            .await;
        let db = RestDatabase::new(
            Workers::new(server.uri()).with_auth(AuthProvider::fixed("t")),
            "turso",
        )
        .with_prefix(Some("dev"))
        .unwrap();
        let t = Duration::from_secs(5);
        let q = Query::new()
            .equal_to("generated", true)
            .order_by("date", false)
            .limit(3);
        assert_eq!(
            db.query("u1", "nansuru", &q, t).await.unwrap()[0]["id"],
            "a"
        );
        assert_eq!(db.get("u1", "user", "missing", t).await.unwrap(), None);
        let row = json!({"id": "u1"}).as_object().unwrap().clone();
        assert_eq!(db.post("u1", "user", None, row).await.unwrap().len(), 1);
        assert_eq!(db.count("u1", "usage", &Query::new(), t).await.unwrap(), 4);
        assert!(db.delete("u1", "user", None, &[]).await.is_err());
    }
}
