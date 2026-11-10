//! Cloud Firestore の名前付きデータベースに対するクエリの取得と監視。
//!
//! 公式の whisker-firebase-firestore は既定のデータベースだけを扱うため、名前付きデータベース
//! （例: `production`）を読むときに使う。ドキュメントは JSON の Map で、ID は `@uid` キーに入る。
//! Timestamp はミリ秒、GeoPoint は `{latitude, longitude}`、参照はパス文字列になる。

use std::sync::atomic::{AtomicU64, Ordering};

pub use masamune_whisker_bridge::{NativeError, NativeResult};
use masamune_whisker_bridge::{NativeModule, Subscription};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

fn module() -> NativeModule {
    NativeModule::new(env!("CARGO_PKG_NAME"), "FirebaseFirestoreDb")
}

/// ドキュメント（フィールドと `@uid`）。
pub type Document = Map<String, Value>;

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

/// コレクションに対するクエリ。
#[derive(Debug, Clone, Serialize)]
pub struct Query {
    database: String,
    collection: String,
    filters: Vec<Filter>,
    #[serde(rename = "orderBy")]
    order_by: Vec<Order>,
    #[serde(skip_serializing_if = "Option::is_none")]
    limit: Option<u32>,
}

impl Query {
    /// `database` は名前付きデータベースの ID（既定のデータベースは `"(default)"`）。
    pub fn new(database: impl Into<String>, collection: impl Into<String>) -> Self {
        Self {
            database: database.into(),
            collection: collection.into(),
            filters: Vec::new(),
            order_by: Vec::new(),
            limit: None,
        }
    }

    fn filter(mut self, field: &str, op: &'static str, value: impl Into<Value>) -> Self {
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

    pub fn within(self, field: &str, values: Vec<Value>) -> Self {
        self.filter(field, "in", Value::Array(values))
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
}

/// クエリを 1 回取得する。
pub async fn get(query: &Query) -> NativeResult<Vec<Document>> {
    module().call_async("get", &json!({ "query": query })).await
}

/// 監視の通知。
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub documents: Vec<Document>,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SnapshotEvent {
    listener_id: String,
    #[serde(default)]
    documents: Vec<Document>,
    #[serde(default)]
    error: Option<String>,
}

/// 監視。破棄すると監視をやめる。
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

/// クエリを監視する。結果が変わるたびに `handler` が UI スレッドで呼ばれる。
pub fn listen(query: &Query, handler: impl Fn(Snapshot) + 'static) -> NativeResult<Listener> {
    let id = format!("listener-{}", NEXT_LISTENER.fetch_add(1, Ordering::Relaxed));
    let watched = id.clone();
    let subscription = module().subscribe("snapshot", move |event: SnapshotEvent| {
        if event.listener_id == watched {
            handler(Snapshot {
                documents: event.documents,
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
