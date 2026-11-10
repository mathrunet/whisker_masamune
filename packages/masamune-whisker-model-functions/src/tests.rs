use std::cell::RefCell;
use std::rc::Rc;

use masamune_whisker_functions::{
    CallableFailure, FunctionsError, FunctionsResult, Method, RuntimeFunctionsAdapter,
};
use serde_json::json;

use super::*;
use crate::field_value::{model_ref, row_timestamp};

type Log = Rc<RefCell<Vec<FunctionRequest>>>;

const T: Duration = Duration::from_secs(5);
const COMMENTS: &str = "author/a1/catalog/c1/content/x1/comment";

/// Store whose callables answer with `respond(action, input)` and record every request.
fn store(
    respond: impl Fn(&str, &Value) -> FunctionsResult<Value> + 'static,
) -> (FunctionsStore, Log) {
    let log: Log = Rc::default();
    let recorded = log.clone();
    let adapter = RuntimeFunctionsAdapter::new(move |request: FunctionRequest| {
        recorded.borrow_mut().push(request.clone());
        let result = respond(&request.action, request.body.as_ref().unwrap());
        async move { result }
    });
    (FunctionsStore::new(Functions::new(adapter)), log)
}

fn row(value: Value) -> Row {
    value.as_object().unwrap().clone()
}

fn timestamp(micros: i64) -> Value {
    json!({"@type": "ModelTimestamp", "@time": micros, "@source": "server"})
}

fn last(log: &Log) -> FunctionRequest {
    log.borrow().last().unwrap().clone()
}

fn assert_callable(request: &FunctionRequest, action: &str, body: Value, timeout: Duration) {
    assert_eq!(request.action, action);
    assert_eq!(request.method, Method::POST);
    assert_eq!(request.path, None);
    assert!(request.query.is_empty());
    assert_eq!(request.timeout, Some(timeout));
    assert_eq!(request.body.as_ref().unwrap(), &body);
}

#[tokio::test]
async fn reads_documents() {
    let (store, log) = store(|_, input| {
        Ok(match input["path"].as_str().unwrap() {
            "user_meta/u1" => json!({
                "status": 200,
                "data": r#"{"name":"A","createdAt":{"@type":"ModelTimestamp","@time":1700000000000000,"@source":"server"}}"#,
            }),
            _ => json!({"status": 200, "data": "{}"}),
        })
    });
    let user = store.get("", "user_meta", "u1", T).await.unwrap().unwrap();
    assert_callable(
        &last(&log),
        "document_model_firestore",
        json!({"path": "user_meta/u1", "method": "get", "databaseId": null}),
        T,
    );
    assert_eq!(user["name"], "A");
    assert_eq!(user["@uid"], "u1");
    assert_eq!(
        row_timestamp(&user, "createdAt"),
        Some(std::time::UNIX_EPOCH + Duration::from_secs(1_700_000_000))
    );
    assert_eq!(store.get("", "/user_meta/", "none", T).await.unwrap(), None);
    assert_eq!(last(&log).body.unwrap()["path"], "user_meta/none");
}

#[tokio::test]
async fn writes_and_deletes_documents() {
    let (store, log) = store(|_, _| Ok(json!({"status": 200})));
    let store = store.with_database_id("develop");
    let comment = row(json!({
        "comment": "hi",
        "user": model_ref("user_meta/u1"),
        "createdAt": {"@type": "ModelTimestamp", "@time": 5, "@now": false, "@source": "user"},
    }));
    store.save("", COMMENTS, "m1", comment).await.unwrap();
    let request = last(&log);
    let body = request.body.clone().unwrap();
    assert_callable(
        &request,
        "document_model_firestore",
        json!({
            "path": "author/a1/catalog/c1/content/x1/comment/m1",
            "method": "post",
            "databaseId": "develop",
            "data": body["data"].clone(),
        }),
        Duration::from_secs(20),
    );
    // `data` is a JSON string, with `@uid` added like Masamune's `filterOnSave`.
    assert_eq!(
        serde_json::from_str::<Value>(body["data"].as_str().unwrap()).unwrap(),
        json!({
            "comment": "hi",
            "user": {"@type": "ModelRefBase", "@ref": "user_meta/u1"},
            "createdAt": {"@type": "ModelTimestamp", "@time": 5, "@now": false, "@source": "user"},
            "@uid": "m1",
        })
    );

    store.delete("production", COMMENTS, "m1").await.unwrap();
    assert_callable(
        &last(&log),
        "document_model_firestore",
        json!({
            "path": "author/a1/catalog/c1/content/x1/comment/m1",
            "method": "delete",
            "databaseId": "production",
        }),
        Duration::from_secs(20),
    );
}

#[tokio::test]
async fn queries_collections_on_the_device() {
    let (store, log) = store(|_, _| {
        let docs = json!({
            "c1": {"comment": "old", "user": {"@type": "ModelRefBase", "@ref": "user_meta/u1"}, "createdAt": timestamp(100)},
            "c2": {"comment": "new", "user": {"@type": "ModelRefBase", "@ref": "user_meta/u2"}, "createdAt": timestamp(300)},
            "c3": {"comment": "mid", "user": {"@type": "ModelRefBase", "@ref": "user_meta/u1"}, "createdAt": timestamp(200)},
        });
        Ok(json!({"status": 200, "data": docs.to_string()}))
    });
    let query = Query::new().order_by("createdAt", true).limit(2);
    let rows = store.query("", COMMENTS, &query, T).await.unwrap();
    assert_callable(
        &last(&log),
        "collection_model_firestore",
        json!({"path": COMMENTS, "method": "get", "databaseId": null}),
        T,
    );
    let ids: Vec<&str> = rows.iter().map(|r| r["@uid"].as_str().unwrap()).collect();
    assert_eq!(ids, ["c2", "c3"]);
    // Rows keep the Masamune values untouched.
    assert_eq!(rows[0]["createdAt"], timestamp(300));

    let by_user = Query::new()
        .equal_to("user", model_ref("user_meta/u1"))
        .greater_than("createdAt", timestamp(150));
    let rows = store
        .query_collection("", COMMENTS, &by_user, T)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, "c3");

    let all = store.load_collection("", COMMENTS, T).await.unwrap();
    let ids: Vec<&str> = all.iter().map(|(id, _)| id.as_str()).collect();
    assert_eq!(ids, ["c1", "c2", "c3"], "server order is kept");
}

#[tokio::test]
async fn writes_collections_and_aggregates() {
    let (store, log) = store(|action, input| {
        Ok(match (action, input["method"].as_str().unwrap()) {
            ("aggregate_model_firestore", "count") => {
                json!({"status": 200, "data": r#"{"value":3}"#})
            }
            ("aggregate_model_firestore", _) => json!({"status": 200, "data": r#"{"value":1.5}"#}),
            ("collection_model_firestore", "get") => json!({"status": 200, "data": "{}"}),
            _ => json!({"status": 200}),
        })
    });
    assert_eq!(store.count("", COMMENTS, T).await.unwrap(), 3);
    assert_callable(
        &last(&log),
        "aggregate_model_firestore",
        json!({"path": COMMENTS, "method": "count", "databaseId": null}),
        T,
    );
    assert_eq!(store.average("", "item", "price", T).await.unwrap(), 1.5);
    assert_eq!(
        last(&log).body.unwrap(),
        json!({"path": "item/price", "method": "average", "databaseId": null})
    );
    assert_eq!(store.sum("", "item", "price", T).await.unwrap(), 1.5);
    assert_eq!(last(&log).body.unwrap()["method"], "sum");
    assert!(
        store
            .aggregate("", "item", AggregateKind::Sum, None, T)
            .await
            .is_err()
    );

    store
        .save_collection(
            "",
            "user/u1/favorite",
            vec![("f1".into(), row(json!({"n": 1})))],
        )
        .await
        .unwrap();
    let body = last(&log).body.unwrap();
    assert_eq!(body["method"], "post");
    assert_eq!(body["path"], "user/u1/favorite");
    assert_eq!(body["data"], r#"{"f1":{"n":1}}"#);

    store
        .delete_collection("", "user/u1/favorite")
        .await
        .unwrap();
    assert_eq!(
        last(&log).body.unwrap(),
        json!({"path": "user/u1/favorite", "method": "delete", "databaseId": null})
    );
    assert!(
        store
            .load_collection("", "item", T)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn reports_failures() {
    let (store, log) = store(|_, input| match input["path"].as_str().unwrap() {
        "user_meta/denied" => Err(FunctionsError::Callable(Box::new(
            CallableFailure::Remote {
                http_status: 403,
                status: "PERMISSION_DENIED".into(),
                message: "no".into(),
                details: Value::Null,
            },
        ))),
        "user_meta/bad" => Ok(json!({"status": 500})),
        "user_meta/garbage" => Ok(json!({"status": 200, "data": "not json"})),
        _ => Ok(json!({"data": "{}"})),
    });
    let error = store.get("", "user_meta", "denied", T).await.unwrap_err();
    assert_eq!(error.status(), Some(403));
    assert_eq!(
        store
            .get("", "user_meta", "bad", T)
            .await
            .unwrap_err()
            .status(),
        Some(500)
    );
    assert!(matches!(
        store.get("", "user_meta", "garbage", T).await,
        Err(ApiError::Decode(_))
    ));
    assert!(matches!(
        store.get("", "user_meta", "nostatus", T).await,
        Err(ApiError::Decode(_))
    ));

    // Invalid paths fail before anything is sent.
    let sent = log.borrow().len();
    assert!(store.get("", "user_meta/u1", "x", T).await.is_err());
    assert!(store.get("", "user_meta", "a/b", T).await.is_err());
    assert!(store.load_collection("", "a//b", T).await.is_err());
    assert_eq!(log.borrow().len(), sent);
}

#[test]
fn decodes_responses() {
    assert_eq!(
        decode_response("a", json!({"status": 200, "data": {"x": 1}})).unwrap(),
        json!({"x": 1})
    );
    assert_eq!(
        decode_response("a", json!({"status": "201", "data": ""})).unwrap(),
        Value::Null
    );
    assert!(decode_response("a", json!(null)).is_err());
}

#[test]
fn validates_paths() {
    assert_eq!(collection_path("/a/b/c/").unwrap(), "a/b/c");
    assert!(collection_path("a/b").is_err());
    assert!(collection_path("").is_err());
    assert_eq!(document_path("a", "b").unwrap(), "a/b");
    assert!(document_path("a", "").is_err());
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test]
async fn works_behind_the_cache() {
    use masamune_whisker_model::{CacheOptions, ReadOptions};

    let (store, log) = store(|_, _| {
        Ok(json!({"status": 200, "data": json!({"c1": {"n": 1}, "c2": {"n": 2}}).to_string()}))
    });
    let cached = CachedFunctionsStore::new(store, None, CacheOptions::new("functions"));
    let rows = cached
        .query(
            "",
            COMMENTS,
            &Query::new().order_by("n", true),
            ReadOptions::reload(),
        )
        .await
        .unwrap();
    let ids: Vec<&str> = rows.iter().map(|(id, _)| id.as_str()).collect();
    assert_eq!(ids, ["c2", "c1"]);
    assert_eq!(log.borrow().len(), 1);
}
