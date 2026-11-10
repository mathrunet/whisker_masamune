//! Firestore documents and collections through the Masamune model Functions
//! (`FunctionsModelAdapter` of `masamune_model_functions` in Flutter,
//! `masamune_model_functions/lib/adapter/functions_model_adapter.dart`).
//!
//! [`FunctionsStore`] sends the callables `document_model_firestore`,
//! `collection_model_firestore` and `aggregate_model_firestore` (the `Functions` of
//! `@mathrunet/masamune_firestore`) through a [`Functions`] handle and implements
//! [`RemoteStore`], so it can sit behind the on-device cache as a [`CachedFunctionsStore`]. The
//! crate only depends on the adapter interface; the app chooses the backend (usually the
//! Firebase callable adapter, with the app's own authentication):
//!
//! ```ignore
//! let functions = Functions::new(FirebaseFunctionsAdapter::new(..).with_firebase_auth());
//! let store = CachedFunctionsStore::new(
//!     FunctionsStore::new(functions),
//!     Some(ModelCache::shared()?),
//!     CacheOptions::new("functions"),
//! );
//! let comments = store
//!     .query("", "author/a/catalog/c/content/x/comment",
//!         &Query::new().order_by("createdAt", true).limit(100), ReadOptions::reload())
//!     .await?;
//! ```
//!
//! # Mapping to [`RemoteStore`]
//!
//! - `database` is the Firestore database id sent as `databaseId`. An empty string uses
//!   [`FunctionsStoreConfig::database_id`] (`None` sends `null`, which lets the server choose).
//! - `table` is the collection path (an odd number of segments, such as
//!   `author/{id}/catalog/{id}/content/{id}/comment`) and `id` the document id.
//! - Rows carry their document id in `@uid` ([`RemoteStore::id_field`]), as Masamune documents do.
//!
//! # Wire format
//!
//! Every request is a POST callable whose input is
//! `{"path": "...", "method": "get|post|delete", "databaseId": <string|null>, "data": "<JSON>"}`;
//! `data` is a JSON-encoded string and only sent by writes. Responses are
//! `{"status": 200, "data": "<JSON>"}` (`data` is also a JSON-encoded string, absent for writes).
//! A `status` outside 2xx is an error, as in Flutter.
//!
//! # Queries
//!
//! The collection callable has no query parameters: it returns every document of the collection.
//! [`FunctionsStore::query`] applies the [`Query`] filters, ordering and limit on the device,
//! comparing Masamune field values by [`field_value::comparable`] (timestamps by time, references
//! by path), so `Query::new().order_by("createdAt", true).limit(100)` works on `ModelTimestamp`
//! fields. Aggregates ([`FunctionsStore::count`], [`FunctionsStore::sum`],
//! [`FunctionsStore::average`]) run on the server and ignore filters, like Flutter's adapter.
//!
//! Transactions, batches and real-time listeners are not available (as in Flutter).

pub mod field_value;

use std::time::Duration;

use masamune_whisker_functions::FunctionRequest;
pub use masamune_whisker_functions::Functions;
#[cfg(not(target_arch = "wasm32"))]
use masamune_whisker_model::CachedStore;
use masamune_whisker_model::{ApiError, ApiResult, Query, RemoteStore, Row};
use serde_json::{Map, Value};

use crate::field_value::{UID_KEY, comparable, comparable_row};

/// A [`FunctionsStore`] with the on-device cache.
#[cfg(not(target_arch = "wasm32"))]
pub type CachedFunctionsStore = CachedStore<FunctionsStore>;

/// Callable names and defaults of [`FunctionsStore`] (`FunctionsModelAdapter`'s parameters).
#[derive(Clone, Debug, PartialEq)]
pub struct FunctionsStoreConfig {
    /// Document callable (`document_model_firestore`).
    pub document_action: String,
    /// Collection callable (`collection_model_firestore`).
    pub collection_action: String,
    /// Aggregate callable (`aggregate_model_firestore`).
    pub aggregate_action: String,
    /// `databaseId` sent when the `database` argument is empty (`None` sends `null`).
    pub database_id: Option<String>,
    /// Deadline of writes.
    pub write_timeout: Duration,
}

impl Default for FunctionsStoreConfig {
    fn default() -> Self {
        Self {
            document_action: "document_model_firestore".into(),
            collection_action: "collection_model_firestore".into(),
            aggregate_action: "aggregate_model_firestore".into(),
            database_id: None,
            write_timeout: Duration::from_secs(20),
        }
    }
}

/// Method of a model callable (`ApiMethod.name` in Flutter).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelMethod {
    Get,
    Post,
    Put,
    Delete,
}

impl ModelMethod {
    pub fn name(self) -> &'static str {
        match self {
            ModelMethod::Get => "get",
            ModelMethod::Post => "post",
            ModelMethod::Put => "put",
            ModelMethod::Delete => "delete",
        }
    }
}

/// Aggregate of the aggregate callable (`ModelAggregateQueryType.name`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AggregateKind {
    Count,
    Sum,
    Average,
}

impl AggregateKind {
    pub fn name(self) -> &'static str {
        match self {
            AggregateKind::Count => "count",
            AggregateKind::Sum => "sum",
            AggregateKind::Average => "average",
        }
    }
}

/// Firestore through the Masamune model Functions, as a [`RemoteStore`].
#[derive(Clone, Debug)]
pub struct FunctionsStore {
    functions: Functions,
    config: FunctionsStoreConfig,
}

impl FunctionsStore {
    /// Store with the default callable names and no database id.
    pub fn new(functions: Functions) -> Self {
        Self::with_config(functions, FunctionsStoreConfig::default())
    }

    pub fn with_config(functions: Functions, config: FunctionsStoreConfig) -> Self {
        Self { functions, config }
    }

    /// Sets [`FunctionsStoreConfig::database_id`].
    pub fn with_database_id(mut self, database_id: impl Into<String>) -> Self {
        self.config.database_id = Some(database_id.into());
        self
    }

    pub fn functions(&self) -> &Functions {
        &self.functions
    }

    pub fn config(&self) -> &FunctionsStoreConfig {
        &self.config
    }

    fn database_id(&self, database: &str) -> Value {
        if database.is_empty() {
            self.config
                .database_id
                .clone()
                .map_or(Value::Null, Value::String)
        } else {
            Value::String(database.to_owned())
        }
    }

    /// Callable input `{path, method, databaseId[, data]}` for `action`.
    pub fn request(
        &self,
        action: &str,
        database: &str,
        path: &str,
        method: &str,
        data: Option<String>,
    ) -> FunctionRequest {
        let mut body = Map::new();
        body.insert("path".into(), Value::String(path.to_owned()));
        body.insert("method".into(), Value::String(method.to_owned()));
        body.insert("databaseId".into(), self.database_id(database));
        if let Some(data) = data {
            body.insert("data".into(), Value::String(data));
        }
        FunctionRequest::new(action).with_body(Value::Object(body))
    }

    /// Sends `request` and returns the decoded `data` of a 2xx response (`Value::Null` when the
    /// response has none).
    pub async fn send(&self, mut request: FunctionRequest, timeout: Duration) -> ApiResult<Value> {
        request.timeout = Some(timeout);
        let action = request.action.clone();
        let response = self.functions.send(request).await?;
        decode_response(&action, response)
    }

    /// Reads one document (`None` when it does not exist, which the server reports as `{}`).
    pub async fn get_document(
        &self,
        database: &str,
        table: &str,
        id: &str,
        timeout: Duration,
    ) -> ApiResult<Option<Row>> {
        let path = document_path(table, id)?;
        let request = self.request(
            &self.config.document_action,
            database,
            &path,
            ModelMethod::Get.name(),
            None,
        );
        match self.send(request, timeout).await? {
            Value::Object(row) if !row.is_empty() => Ok(Some(with_uid(row, id))),
            Value::Object(_) | Value::Null => Ok(None),
            other => Err(ApiError::Decode(format!(
                "document data is not an object: {}",
                type_name(&other)
            ))),
        }
    }

    /// Writes one document (`post`, merged into the existing one). `@uid` is set to `id`, as
    /// Masamune documents do when they save.
    pub async fn save_document(
        &self,
        database: &str,
        table: &str,
        id: &str,
        row: Row,
    ) -> ApiResult<()> {
        let path = document_path(table, id)?;
        let data = encode(&with_uid(row, id))?;
        let request = self.request(
            &self.config.document_action,
            database,
            &path,
            ModelMethod::Post.name(),
            Some(data),
        );
        self.send(request, self.config.write_timeout).await?;
        Ok(())
    }

    /// Deletes one document.
    pub async fn delete_document(&self, database: &str, table: &str, id: &str) -> ApiResult<()> {
        let path = document_path(table, id)?;
        let request = self.request(
            &self.config.document_action,
            database,
            &path,
            ModelMethod::Delete.name(),
            None,
        );
        self.send(request, self.config.write_timeout).await?;
        Ok(())
    }

    /// Every document of the collection `table`, as `(id, row)` pairs in server order. Each row
    /// gets `@uid` set to its id.
    pub async fn load_collection(
        &self,
        database: &str,
        table: &str,
        timeout: Duration,
    ) -> ApiResult<Vec<(String, Row)>> {
        let path = collection_path(table)?;
        let request = self.request(
            &self.config.collection_action,
            database,
            &path,
            ModelMethod::Get.name(),
            None,
        );
        match self.send(request, timeout).await? {
            Value::Object(docs) => docs
                .into_iter()
                .map(|(id, row)| match row {
                    Value::Object(row) => Ok((id.clone(), with_uid(row, &id))),
                    other => Err(ApiError::Decode(format!(
                        "collection document {id} is not an object: {}",
                        type_name(&other)
                    ))),
                })
                .collect(),
            Value::Null => Ok(Vec::new()),
            other => Err(ApiError::Decode(format!(
                "collection data is not an object: {}",
                type_name(&other)
            ))),
        }
    }

    /// Documents of `table` matching `query`, filtered, sorted and limited on the device (see the
    /// crate docs).
    pub async fn query_collection(
        &self,
        database: &str,
        table: &str,
        query: &Query,
        timeout: Duration,
    ) -> ApiResult<Vec<(String, Row)>> {
        let docs = self.load_collection(database, table, timeout).await?;
        Ok(apply_query(query, docs))
    }

    /// Writes several documents of `table` in one call (`post` on the collection callable). The
    /// server merges each document and sets `@uid` and `@time`.
    pub async fn save_collection(
        &self,
        database: &str,
        table: &str,
        docs: Vec<(String, Row)>,
    ) -> ApiResult<()> {
        if docs.is_empty() {
            return Ok(());
        }
        let path = collection_path(table)?;
        let mut data = Map::new();
        for (id, row) in docs {
            validate_id(&id)?;
            data.insert(id, Value::Object(row));
        }
        let request = self.request(
            &self.config.collection_action,
            database,
            &path,
            ModelMethod::Post.name(),
            Some(encode(&data)?),
        );
        self.send(request, self.config.write_timeout).await?;
        Ok(())
    }

    /// Deletes every document of `table` (`delete` on the collection callable).
    pub async fn delete_collection(&self, database: &str, table: &str) -> ApiResult<()> {
        let path = collection_path(table)?;
        let request = self.request(
            &self.config.collection_action,
            database,
            &path,
            ModelMethod::Delete.name(),
            None,
        );
        self.send(request, self.config.write_timeout).await?;
        Ok(())
    }

    /// Runs an aggregate on the server. `field` is required for sums and averages and appended
    /// to the path (`{table}/{field}`), as Flutter does.
    pub async fn aggregate(
        &self,
        database: &str,
        table: &str,
        kind: AggregateKind,
        field: Option<&str>,
        timeout: Duration,
    ) -> ApiResult<f64> {
        let mut path = collection_path(table)?;
        match (kind, field.map(|f| f.trim_matches('/'))) {
            (AggregateKind::Count, _) => {}
            (_, Some(field)) if !field.is_empty() && !field.contains('/') => {
                path = format!("{path}/{field}");
            }
            _ => {
                return Err(ApiError::Other(format!(
                    "{} needs a field name",
                    kind.name()
                )));
            }
        }
        let request = self.request(
            &self.config.aggregate_action,
            database,
            &path,
            kind.name(),
            None,
        );
        let data = self.send(request, timeout).await?;
        match data.get("value") {
            Some(Value::Number(n)) => n
                .as_f64()
                .ok_or_else(|| ApiError::Decode(format!("aggregate value: {n}"))),
            Some(Value::Null) | None => Ok(0.0),
            Some(other) => Err(ApiError::Decode(format!(
                "aggregate value is not a number: {}",
                type_name(other)
            ))),
        }
    }

    /// Number of documents of `table` (server-side; filters are not applied).
    pub async fn count(&self, database: &str, table: &str, timeout: Duration) -> ApiResult<u64> {
        let value = self
            .aggregate(database, table, AggregateKind::Count, None, timeout)
            .await?;
        Ok(value.max(0.0) as u64)
    }

    /// Sum of `field` over `table`.
    pub async fn sum(
        &self,
        database: &str,
        table: &str,
        field: &str,
        timeout: Duration,
    ) -> ApiResult<f64> {
        self.aggregate(database, table, AggregateKind::Sum, Some(field), timeout)
            .await
    }

    /// Average of `field` over `table`.
    pub async fn average(
        &self,
        database: &str,
        table: &str,
        field: &str,
        timeout: Duration,
    ) -> ApiResult<f64> {
        self.aggregate(
            database,
            table,
            AggregateKind::Average,
            Some(field),
            timeout,
        )
        .await
    }
}

impl RemoteStore for FunctionsStore {
    async fn get(
        &self,
        database: &str,
        table: &str,
        id: &str,
        timeout: Duration,
    ) -> ApiResult<Option<Row>> {
        self.get_document(database, table, id, timeout).await
    }

    async fn query(
        &self,
        database: &str,
        table: &str,
        query: &Query,
        timeout: Duration,
    ) -> ApiResult<Vec<Row>> {
        Ok(self
            .query_collection(database, table, query, timeout)
            .await?
            .into_iter()
            .map(|(_, row)| row)
            .collect())
    }

    async fn save(&self, database: &str, table: &str, id: &str, row: Row) -> ApiResult<()> {
        self.save_document(database, table, id, row).await
    }

    async fn delete(&self, database: &str, table: &str, id: &str) -> ApiResult<()> {
        self.delete_document(database, table, id).await
    }

    fn id_field(&self) -> &str {
        UID_KEY
    }
}

/// Applies `query` to `(id, row)` pairs, comparing Masamune field values by
/// [`field_value::comparable`] (filter values are normalized the same way).
pub fn apply_query(query: &Query, docs: Vec<(String, Row)>) -> Vec<(String, Row)> {
    let mut query = query.clone();
    for filter in &mut query.filters {
        if let Some(value) = &filter.value {
            filter.value = Some(match value {
                Value::Array(items) => Value::Array(items.iter().map(comparable).collect()),
                other => comparable(other),
            });
        }
    }
    let docs: Vec<((String, Row), Row)> = docs
        .into_iter()
        .map(|doc| {
            let plain = comparable_row(&doc.1);
            (doc, plain)
        })
        .collect();
    query
        .apply(docs, |(_, plain)| plain)
        .into_iter()
        .map(|(doc, _)| doc)
        .collect()
}

/// Decodes a callable response `{"status": 2xx, "data": "<JSON>"}` into `data`. `data` may
/// also be an already decoded value; an empty or missing `data` is `Value::Null`.
pub fn decode_response(action: &str, response: Value) -> ApiResult<Value> {
    let Value::Object(mut map) = response else {
        return Err(ApiError::Decode(format!(
            "{action}: response is not an object"
        )));
    };
    let status = match map.get("status") {
        Some(Value::Number(n)) => n.as_u64(),
        Some(Value::String(s)) => s.parse().ok(),
        _ => None,
    };
    let Some(status) = status else {
        return Err(ApiError::Decode(format!(
            "{action}: response has no status"
        )));
    };
    if !(200..300).contains(&status) {
        return Err(ApiError::Http {
            status: u16::try_from(status).unwrap_or(u16::MAX),
            body: format!("{action} failed"),
        });
    }
    match map.remove("data") {
        None | Some(Value::Null) => Ok(Value::Null),
        Some(Value::String(text)) if text.trim().is_empty() => Ok(Value::Null),
        Some(Value::String(text)) => serde_json::from_str(&text)
            .map_err(|e| ApiError::Decode(format!("{action}: data is not JSON: {e}"))),
        Some(value) => Ok(value),
    }
}

/// Normalized collection path (`/a/b/c/` → `a/b/c`); it needs an odd number of segments.
pub fn collection_path(table: &str) -> ApiResult<String> {
    let path = table.trim_matches('/');
    let segments: Vec<&str> = path.split('/').collect();
    if path.is_empty() || segments.iter().any(|s| s.is_empty()) || segments.len() % 2 != 1 {
        return Err(ApiError::Other(format!("invalid collection path: {table}")));
    }
    Ok(path.to_owned())
}

/// Document path `{table}/{id}`; `table` must be a collection path and `id` one segment.
pub fn document_path(table: &str, id: &str) -> ApiResult<String> {
    validate_id(id)?;
    Ok(format!("{}/{id}", collection_path(table)?))
}

fn validate_id(id: &str) -> ApiResult<()> {
    if id.is_empty() || id.contains('/') {
        return Err(ApiError::Other(format!("invalid document id: {id}")));
    }
    Ok(())
}

fn with_uid(mut row: Row, id: &str) -> Row {
    row.insert(UID_KEY.into(), Value::String(id.to_owned()));
    row
}

fn encode<T: serde::Serialize>(value: &T) -> ApiResult<String> {
    serde_json::to_string(value).map_err(|e| ApiError::Other(e.to_string()))
}

fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests;
