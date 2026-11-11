use std::cell::RefCell;
use std::rc::Rc;

use masamune_whisker_functions::{
    CallableFailure, FunctionsResult, Method, RuntimeFunctionsAdapter,
};
use serde_json::json;

use super::*;

type Log = Rc<RefCell<Vec<FunctionRequest>>>;

const BUCKET: &str = "kanimiso.firebasestorage.app";
const PUBLIC: &str = "https://firebasestorage.googleapis.com/v0/b/kanimiso.firebasestorage.app/o/public%2Fu1%2Fa.png?alt=media";

/// Adapter whose callable answers with `respond(input)` and records every request.
fn storage(
    respond: impl Fn(&Value) -> FunctionsResult<Value> + 'static,
) -> (FunctionsStorageAdapter, Log) {
    let log: Log = Rc::default();
    let recorded = log.clone();
    let adapter = RuntimeFunctionsAdapter::new(move |request: FunctionRequest| {
        recorded.borrow_mut().push(request.clone());
        let result = respond(request.body.as_ref().unwrap());
        async move { result }
    });
    (
        FunctionsStorageAdapter::new(Functions::new(adapter), format!("/{BUCKET}/")),
        log,
    )
}

fn last(log: &Log) -> FunctionRequest {
    log.borrow().last().unwrap().clone()
}

fn assert_callable(request: &FunctionRequest, body: Value) {
    assert_eq!(request.action, "storage_firebase");
    assert_eq!(request.method, Method::POST);
    assert_eq!(request.path, None);
    assert!(request.query.is_empty());
    assert_eq!(request.timeout, Some(Duration::from_secs(120)));
    assert_eq!(request.body.as_ref().unwrap(), &body);
}

fn get_response() -> Value {
    json!({
        "status": 200,
        "binary": "aGVsbG8=",
        "meta": {
            "contentType": "image/png",
            "size": "5",
            "downloadUri": "https://signed.example/a.png?sig=1",
            "publicUri": PUBLIC,
        },
    })
}

#[tokio::test]
async fn uploads_base64_with_the_content_type() {
    let (storage, log) = storage(|_| {
        Ok(json!({
            "status": 200,
            "binary": "aGVsbG8=",
            "message": "File uploaded successfully",
            "meta": {
                "contentType": "image/png",
                "downloadUri": "https://signed.example/a.png?sig=1",
                "publicUri": PUBLIC,
            },
        }))
    });
    let file = storage
        .upload_bytes(b"png", "/public//u1/a.png?x=1", None)
        .await
        .unwrap();
    assert_callable(
        &last(&log),
        json!({
            "method": "post",
            "path": "kanimiso.firebasestorage.app/public/u1/a.png",
            "binary": "cG5n",
            "meta": {"contentType": "image/png"},
        }),
    );
    assert_eq!(file.uri, PUBLIC);
    assert_eq!(file.mime_type.as_deref(), Some("image/png"));

    // Every URL lookup reauthorizes with the server.
    assert_eq!(
        storage.fetch_public_uri("public/u1/a.png").await.unwrap(),
        PUBLIC
    );
    assert_eq!(
        storage.fetch_download_uri("public/u1/a.png").await.unwrap(),
        "https://signed.example/a.png?sig=1"
    );
    assert_eq!(log.borrow().len(), 3);

    // Files are read from disk; unknown extensions send no `meta`.
    let dir = tempfile::tempdir().unwrap();
    let local = dir.path().join("picked");
    std::fs::write(&local, b"raw").unwrap();
    storage.upload(&local, "public/u1/raw", None).await.unwrap();
    assert_callable(
        &last(&log),
        json!({
            "method": "post",
            "path": "kanimiso.firebasestorage.app/public/u1/raw",
            "binary": "cmF3",
        }),
    );

    // A URL is already uploaded (Flutter's `upload` shortcut).
    let remote = storage
        .upload(
            Path::new("https://cdn.example/a.png"),
            "public/u1/a.png",
            None,
        )
        .await
        .unwrap();
    assert_eq!(remote.uri, "https://cdn.example/a.png");
    assert_eq!(log.borrow().len(), 4);
}

#[tokio::test]
async fn downloads_into_memory_and_files_with_authorization_each_time() {
    let (storage, log) = storage(|_| Ok(get_response()));
    let file = storage.download("public/u1/a.png", None).await.unwrap();
    assert_callable(
        &last(&log),
        json!({"method": "get", "path": "kanimiso.firebasestorage.app/public/u1/a.png"}),
    );
    assert_eq!(file.bytes.as_deref(), Some(&b"hello"[..]));
    assert_eq!(file.mime_type.as_deref(), Some("image/png"));
    assert_eq!(file.path, None);

    // A second download and each URL lookup require another server request.
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("cache/a.png");
    let file = storage
        .download("/public/u1/a.png", Some(&target))
        .await
        .unwrap();
    assert_eq!(std::fs::read(&target).unwrap(), b"hello");
    assert_eq!(file.path.as_deref(), Some(target.as_path()));
    assert_eq!(
        storage.fetch_public_uri("public/u1/a.png").await.unwrap(),
        PUBLIC
    );
    assert_eq!(log.borrow().len(), 3);

    storage.clear_cache();
    storage.download("public/u1/a.png", None).await.unwrap();
    assert_eq!(log.borrow().len(), 4);
}

#[tokio::test]
async fn fetches_urls_of_uncached_paths() {
    let (storage, log) = storage(|_| Ok(get_response()));
    assert_eq!(
        storage.fetch_download_uri("public/u1/a.png").await.unwrap(),
        "https://signed.example/a.png?sig=1"
    );
    assert_eq!(last(&log).body.unwrap()["method"], "get");
    assert_eq!(
        storage.fetch_public_uri("public/u1/a.png").await.unwrap(),
        PUBLIC
    );
    assert_eq!(log.borrow().len(), 2);
}

#[tokio::test]
async fn deletes_and_forgets_the_cache() {
    let (storage, log) = storage(|input| {
        Ok(match input["method"].as_str() {
            Some("delete") => json!({"status": 200, "message": "File deleted successfully"}),
            _ => get_response(),
        })
    });
    storage.download("public/u1/a.png", None).await.unwrap();
    storage
        .clone()
        .with_action("storage_firebase")
        .delete("public/u1/a.png")
        .await
        .unwrap();
    assert_callable(
        &last(&log),
        json!({"method": "delete", "path": "kanimiso.firebasestorage.app/public/u1/a.png"}),
    );
    storage.download("public/u1/a.png", None).await.unwrap();
    assert_eq!(log.borrow().len(), 3);
}

#[tokio::test]
async fn maps_errors() {
    let (storage, _) = storage(|input| match input["method"].as_str() {
        Some("get") => Ok(json!({"status": 404, "binary": null, "error": "File not found"})),
        Some("delete") => Err(CallableFailure::Remote {
            http_status: 403,
            status: "PERMISSION_DENIED".into(),
            message: "Cannot modify another user's assets.".into(),
            details: Value::Null,
        }
        .into()),
        _ => Ok(json!({"status": 200})),
    });
    assert_eq!(
        storage.download("public/u1/missing.png", None).await,
        Err(StorageError::NotFound)
    );
    assert_eq!(
        storage.delete("public/u2/a.png").await,
        Err(StorageError::Unauthorized)
    );
    // An upload response without `publicUri` is not a usable upload.
    assert!(matches!(
        storage.upload_bytes(b"x", "public/u1/a.png", None).await,
        Err(StorageError::Backend { code, .. }) if code == "invalid-response"
    ));
    assert!(matches!(
        storage.upload_bytes(b"", "public/u1/a.png", None).await,
        Err(StorageError::InvalidArgument(_))
    ));
    assert!(matches!(
        storage.fetch_public_uri("/").await,
        Err(StorageError::InvalidArgument(_))
    ));
    assert!(map_error(ApiError::Timeout).is_transient());
    assert_eq!(map_error(ApiError::NotSignedIn), StorageError::Unauthorized);
    assert!(matches!(
        decode_response(json!({"message": "no status"})),
        Err(StorageError::Backend { .. })
    ));
}

#[tokio::test]
async fn rejects_an_empty_bucket() {
    let functions = Functions::new(RuntimeFunctionsAdapter::new(|_| async {
        Ok(json!({"status": 200}))
    }));
    let storage = FunctionsStorageAdapter::new(functions, "");
    assert_eq!(
        storage.delete("public/u1/a.png").await,
        Err(StorageError::InvalidArgument(
            "the storage bucket is empty".into()
        ))
    );
}

#[tokio::test]
async fn switching_users_never_reuses_private_bytes_or_urls() {
    use std::cell::Cell;
    let signed_in = Rc::new(Cell::new(true));
    let current = signed_in.clone();
    let (storage, log) = storage(move |_| {
        if current.get() {
            Ok(get_response())
        } else {
            Err(ApiError::NotSignedIn.into())
        }
    });
    storage.download("private/file.png", None).await.unwrap();
    storage
        .fetch_download_uri("private/file.png")
        .await
        .unwrap();
    signed_in.set(false);
    let other = storage.clone();
    assert_eq!(
        other.download("private/file.png", None).await,
        Err(StorageError::Unauthorized)
    );
    assert_eq!(
        other.fetch_download_uri("private/file.png").await,
        Err(StorageError::Unauthorized)
    );
    assert_eq!(
        other.fetch_public_uri("private/file.png").await,
        Err(StorageError::Unauthorized)
    );
    assert_eq!(log.borrow().len(), 5);
}
