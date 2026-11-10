//! Cloud Firestore for Whisker apps: queries, documents, writes and real-time listeners on the
//! default database (`"(default)"`) or named databases (for example `"production"`).
//!
//! Documents are JSON maps. Every read adds the document id as `@uid` and its path as `@path`.
//! Reads return Timestamps as milliseconds, GeoPoints as `{latitude, longitude}` and references
//! as path strings. Writes and filter values take typed values through [`FieldValue`]
//! (`FieldValue::timestamp(ms)`, `FieldValue::server_timestamp()`, …).
//!
//! For an optional on-device cache in front of these reads, use `masamune-whisker-firestore`.
//!
//! On iOS the default Firebase app is configured from the bundled `GoogleService-Info.plist` on
//! first use (Android initializes it at startup). Invalid arguments that the iOS SDK reports as
//! Objective-C exceptions (bad paths, invalid queries, `configure` after first use) are returned
//! as errors like on Android; error messages are the SDK's own and differ in wording between
//! platforms. Bytes fields, which have no JSON form, are read as base64 strings on iOS.

use std::sync::atomic::{AtomicU64, Ordering};

pub use masamune_whisker_bridge::{NativeError, NativeResult};
use masamune_whisker_bridge::{NativeModule, Subscription};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

fn module() -> NativeModule {
    NativeModule::new(env!("CARGO_PKG_NAME"), "FirebaseFirestoreDb")
}

/// A document: fields plus `@uid` (id) and `@path`.
pub type Document = Map<String, Value>;

/// Typed values for writes and filters.
pub struct FieldValue;

impl FieldValue {
    /// A Timestamp from milliseconds since the Unix epoch.
    pub fn timestamp(millis: i64) -> Value {
        json!({ "@timestamp": millis })
    }

    pub fn geo_point(latitude: f64, longitude: f64) -> Value {
        json!({ "@geo": { "latitude": latitude, "longitude": longitude } })
    }

    /// A reference to the document at `path` (default database).
    pub fn reference(path: &str) -> Value {
        json!({ "@ref": path })
    }

    pub fn server_timestamp() -> Value {
        json!({ "@serverTimestamp": true })
    }

    pub fn increment(by: impl Into<serde_json::Number>) -> Value {
        json!({ "@increment": Value::Number(by.into()) })
    }

    pub fn array_union(values: Vec<Value>) -> Value {
        json!({ "@arrayUnion": values })
    }

    pub fn array_remove(values: Vec<Value>) -> Value {
        json!({ "@arrayRemove": values })
    }

    /// Removes the field (in `update` or merging `set`).
    pub fn delete() -> Value {
        json!({ "@delete": true })
    }
}

/// Where a read is served from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Source {
    /// Server when online, the SDK cache when offline.
    #[default]
    Default,
    /// Server only (fails offline).
    Server,
    /// The SDK's offline cache only.
    Cache,
}

impl Source {
    fn as_str(self) -> &'static str {
        match self {
            Source::Default => "default",
            Source::Server => "server",
            Source::Cache => "cache",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct Filter {
    field: String,
    op: &'static str,
    value: Value,
}

#[derive(Debug, Clone, Serialize)]
struct Order {
    field: String,
    descending: bool,
}

/// Query of a collection (or a collection group).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Query {
    database: String,
    collection: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    collection_group: bool,
    filters: Vec<Filter>,
    order_by: Vec<Order>,
    #[serde(skip_serializing_if = "Option::is_none")]
    limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    limit_to_last: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    start_at: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    start_after: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    end_at: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    end_before: Option<Vec<Value>>,
}

impl Query {
    /// `database` is the database id (`"(default)"` for the default database); `collection` the
    /// collection path (for example `"plugins/iap/subscription"`).
    pub fn new(database: impl Into<String>, collection: impl Into<String>) -> Self {
        Self {
            database: database.into(),
            collection: collection.into(),
            collection_group: false,
            filters: Vec::new(),
            order_by: Vec::new(),
            limit: None,
            limit_to_last: None,
            start_at: None,
            start_after: None,
            end_at: None,
            end_before: None,
        }
    }

    /// Queries every collection with the id `collection` (`collectionGroup`).
    pub fn group(database: impl Into<String>, collection: impl Into<String>) -> Self {
        Self {
            collection_group: true,
            ..Self::new(database, collection)
        }
    }

    pub fn database(&self) -> &str {
        &self.database
    }

    pub fn collection(&self) -> &str {
        &self.collection
    }

    /// Adds a filter with a Firestore operator (`"=="`, `"!="`, `"<"`, `"<="`, `">"`, `">="`,
    /// `"array-contains"`, `"array-contains-any"`, `"in"`, `"not-in"`).
    pub fn filter(mut self, field: &str, op: &'static str, value: impl Into<Value>) -> Self {
        self.filters.push(Filter {
            field: field.into(),
            op,
            value: value.into(),
        });
        self
    }

    pub fn equal(self, field: &str, value: impl Into<Value>) -> Self {
        self.filter(field, "==", value)
    }

    pub fn not_equal(self, field: &str, value: impl Into<Value>) -> Self {
        self.filter(field, "!=", value)
    }

    pub fn less_than(self, field: &str, value: impl Into<Value>) -> Self {
        self.filter(field, "<", value)
    }

    pub fn less_than_or_equal(self, field: &str, value: impl Into<Value>) -> Self {
        self.filter(field, "<=", value)
    }

    pub fn greater_than(self, field: &str, value: impl Into<Value>) -> Self {
        self.filter(field, ">", value)
    }

    pub fn greater_than_or_equal(self, field: &str, value: impl Into<Value>) -> Self {
        self.filter(field, ">=", value)
    }

    pub fn array_contains(self, field: &str, value: impl Into<Value>) -> Self {
        self.filter(field, "array-contains", value)
    }

    pub fn array_contains_any(self, field: &str, values: Vec<Value>) -> Self {
        self.filter(field, "array-contains-any", values)
    }

    pub fn within(self, field: &str, values: Vec<Value>) -> Self {
        self.filter(field, "in", values)
    }

    pub fn not_within(self, field: &str, values: Vec<Value>) -> Self {
        self.filter(field, "not-in", values)
    }

    pub fn order_by(mut self, field: &str, descending: bool) -> Self {
        self.order_by.push(Order {
            field: field.into(),
            descending,
        });
        self
    }

    pub fn limit(mut self, count: u32) -> Self {
        self.limit = Some(count);
        self
    }

    pub fn limit_to_last(mut self, count: u32) -> Self {
        self.limit_to_last = Some(count);
        self
    }

    /// Cursor values for the `order_by` fields (pagination).
    pub fn start_at(mut self, values: Vec<Value>) -> Self {
        self.start_at = Some(values);
        self
    }

    pub fn start_after(mut self, values: Vec<Value>) -> Self {
        self.start_after = Some(values);
        self
    }

    pub fn end_at(mut self, values: Vec<Value>) -> Self {
        self.end_at = Some(values);
        self
    }

    pub fn end_before(mut self, values: Vec<Value>) -> Self {
        self.end_before = Some(values);
        self
    }
}

/// Configures the offline persistence of `database`. Call before its first use.
pub fn configure(
    database: &str,
    persistence: bool,
    cache_size_bytes: Option<u64>,
) -> NativeResult<bool> {
    module().call(
        "configure",
        &json!({ "database": database, "persistence": persistence, "cacheSizeBytes": cache_size_bytes.unwrap_or(0) }),
    )
}

/// Reads a query once.
pub async fn get(query: &Query) -> NativeResult<Vec<Document>> {
    get_from(query, Source::Default).await
}

/// Reads a query once from `source`.
pub async fn get_from(query: &Query, source: Source) -> NativeResult<Vec<Document>> {
    module()
        .call_async("get", &json!({ "query": query, "source": source.as_str() }))
        .await
}

/// Counts the documents matching a query on the server.
pub async fn count(query: &Query) -> NativeResult<u64> {
    module()
        .call_async("count", &json!({ "query": query }))
        .await
}

/// One write of a [`batch`].
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum BatchOperation {
    Set {
        path: String,
        data: Document,
        merge: bool,
    },
    Update {
        path: String,
        data: Document,
    },
    Delete {
        path: String,
    },
}

/// Commits several writes atomically.
pub async fn batch(database: &str, operations: Vec<BatchOperation>) -> NativeResult<bool> {
    module()
        .call_async(
            "batch",
            &json!({ "database": database, "operations": operations }),
        )
        .await
}

/// A document reference.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DocumentRef {
    pub database: String,
    pub path: String,
}

impl DocumentRef {
    /// `path` is the full document path (for example `"users/abc"`).
    pub fn new(database: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            database: database.into(),
            path: path.into(),
        }
    }

    /// Document id (last path segment).
    pub fn id(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }

    /// Collection path (everything before the id).
    pub fn collection(&self) -> &str {
        self.path
            .rsplit_once('/')
            .map_or("", |(collection, _)| collection)
    }

    fn args(&self) -> Map<String, Value> {
        let mut args = Map::new();
        args.insert("database".into(), Value::String(self.database.clone()));
        args.insert("path".into(), Value::String(self.path.clone()));
        args
    }

    /// Reads the document (`None` when it does not exist).
    pub async fn get(&self, source: Source) -> NativeResult<Option<Document>> {
        let mut args = self.args();
        args.insert("source".into(), Value::String(source.as_str().into()));
        module().call_async("getDoc", &args).await
    }

    /// Writes the document (replacing it, or merging fields with `merge`).
    pub async fn set(&self, data: Document, merge: bool) -> NativeResult<bool> {
        let mut args = self.args();
        args.insert("data".into(), Value::Object(data));
        args.insert("merge".into(), Value::Bool(merge));
        module().call_async("set", &args).await
    }

    /// Updates fields of an existing document.
    pub async fn update(&self, data: Document) -> NativeResult<bool> {
        let mut args = self.args();
        args.insert("data".into(), Value::Object(data));
        module().call_async("update", &args).await
    }

    pub async fn delete(&self) -> NativeResult<bool> {
        module().call_async("delete", &self.args()).await
    }

    /// Listens to the document. `handler` runs on the UI thread for every change (and once
    /// right away).
    pub fn listen(&self, handler: impl Fn(DocumentSnapshot) + 'static) -> NativeResult<Listener> {
        let id = next_listener_id();
        let watched = id.clone();
        let subscription = module().subscribe("snapshot", move |event: SnapshotEvent| {
            if event.listener_id == watched {
                handler(DocumentSnapshot {
                    document: event.document,
                    from_cache: event.from_cache,
                    has_pending_writes: event.has_pending_writes,
                    error: event.error,
                });
            }
        });
        let mut args = self.args();
        args.insert("listenerId".into(), Value::String(id.clone()));
        module().call::<_, bool>("listenDoc", &args)?;
        Ok(Listener {
            id,
            _subscription: subscription,
        })
    }
}

/// Kind of a [`DocumentChange`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChangeType {
    Added,
    Modified,
    Removed,
}

/// One change of a query snapshot.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentChange {
    #[serde(rename = "type")]
    pub kind: ChangeType,
    pub id: String,
    pub old_index: i64,
    pub new_index: i64,
}

/// Notification of a query listener.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub documents: Vec<Document>,
    pub changes: Vec<DocumentChange>,
    /// The data came from the SDK cache (not yet confirmed by the server).
    pub from_cache: bool,
    /// Local writes not yet committed to the server.
    pub has_pending_writes: bool,
    pub error: Option<String>,
}

/// Notification of a document listener.
#[derive(Debug, Clone, Default)]
pub struct DocumentSnapshot {
    /// `None` when the document does not exist.
    pub document: Option<Document>,
    pub from_cache: bool,
    pub has_pending_writes: bool,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SnapshotEvent {
    listener_id: String,
    #[serde(default)]
    documents: Vec<Document>,
    #[serde(default)]
    document: Option<Document>,
    #[serde(default)]
    changes: Vec<DocumentChange>,
    #[serde(default)]
    from_cache: bool,
    #[serde(default)]
    has_pending_writes: bool,
    #[serde(default)]
    error: Option<String>,
}

/// A running listener. Dropping it stops listening.
pub struct Listener {
    id: String,
    _subscription: Subscription,
}

impl Drop for Listener {
    fn drop(&mut self) {
        let _ = module().call::<_, bool>("stop", &json!({ "listenerId": self.id }));
    }
}

static NEXT_LISTENER: AtomicU64 = AtomicU64::new(1);

fn next_listener_id() -> String {
    format!("listener-{}", NEXT_LISTENER.fetch_add(1, Ordering::Relaxed))
}

/// Listens to a query. `handler` runs on the UI thread for every change (and once right away).
pub fn listen(query: &Query, handler: impl Fn(Snapshot) + 'static) -> NativeResult<Listener> {
    let id = next_listener_id();
    let watched = id.clone();
    let subscription = module().subscribe("snapshot", move |event: SnapshotEvent| {
        if event.listener_id == watched {
            handler(Snapshot {
                documents: event.documents,
                changes: event.changes,
                from_cache: event.from_cache,
                has_pending_writes: event.has_pending_writes,
                error: event.error,
            });
        }
    });
    module().call::<_, bool>("listen", &json!({ "listenerId": id, "query": query }))?;
    Ok(Listener {
        id,
        _subscription: subscription,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_queries_and_values() {
        let q = Query::group("production", "items")
            .equal("userId", "u")
            .greater_than_or_equal("expiresAt", FieldValue::timestamp(5))
            .not_within("state", vec![json!("a")])
            .order_by("expiresAt", true)
            .start_after(vec![json!(10)])
            .limit(5);
        let v = serde_json::to_value(&q).unwrap();
        assert_eq!(v["collectionGroup"], true);
        assert_eq!(v["filters"][1]["value"], json!({"@timestamp": 5}));
        assert_eq!(v["filters"][2]["op"], "not-in");
        assert_eq!(v["startAfter"], json!([10]));
        assert!(v.get("limitToLast").is_none());
        assert!(
            serde_json::to_value(Query::new("(default)", "c"))
                .unwrap()
                .get("collectionGroup")
                .is_none()
        );

        let op = BatchOperation::Set {
            path: "a/b".into(),
            data: Document::new(),
            merge: true,
        };
        assert_eq!(serde_json::to_value(op).unwrap()["type"], "set");
        assert_eq!(FieldValue::increment(2), json!({"@increment": 2}));
        let doc = DocumentRef::new("(default)", "users/u1/items/i9");
        assert_eq!((doc.id(), doc.collection()), ("i9", "users/u1/items"));
    }

    #[test]
    fn parses_snapshot_events() {
        let event: SnapshotEvent = serde_json::from_value(json!({
            "listenerId": "listener-1",
            "documents": [{"@uid": "a"}],
            "changes": [{"type": "removed", "id": "b", "oldIndex": 1, "newIndex": -1}],
            "fromCache": true
        }))
        .unwrap();
        assert_eq!(event.changes[0].kind, ChangeType::Removed);
        assert!(event.from_cache);
        assert!(!event.has_pending_writes);
    }
}
