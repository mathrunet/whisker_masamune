//! Cloudflare R2 through the Masamune storage worker as a [`StorageAdapter`]
//! (`masamune_storage_cloudflare` in Flutter).
//!
//! The app holds no R2 credentials: every operation is a `POST {action}` to the worker of
//! `masamune_cloudflare_storage` (`Functions.storageCloudflare`), which checks the storage rules
//! against the Firebase user and reads or writes the bucket binding. The worker needs:
//!
//! - an R2 bucket binding (`R2_BUCKET` by default),
//! - `STORAGE_PUBLIC_BASE_URL` (or `publicBaseUrl`) for public URLs,
//! - `STORAGE_DOWNLOAD_URL_SECRET` (or `downloadUrlSecret`) for expiring download URLs,
//! - storage rules that allow the operations (everything is denied without them).
//!
//! Files travel base64-encoded inside JSON, so keep them to a few megabytes.
//!
//! Requests go through a [`Functions`] handle, so this crate depends only on the adapter
//! interface. Every operation is a plain `POST {action}` with a JSON body, so it works with
//! `CloudflareFunctionsAdapter` as well as with a Firebase callable that implements the same
//! contract.
//!
//! ```ignore
//! let functions = Functions::new(CloudflareFunctionsAdapter::new(EDGE).with_auth(firebase_auth()));
//! let storage = CloudflareStorageAdapter::new(functions, "https://files.example.com");
//! let file = storage.upload_bytes(&png, "users/alice/avatar.png", None).await?;
//! ```

use std::path::Path;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
pub use masamune_whisker_functions::Functions;
use masamune_whisker_functions::{ApiError, FunctionRequest};
pub use masamune_whisker_storage::{
    LocalFile, RemoteFile, StorageAdapter, StorageError, StorageResult,
};
use masamune_whisker_storage::{encode_path, normalize_path, resolve_mime_type, write_file};
use serde_json::{Map, Value, json};

/// Cloudflare R2 through the Masamune storage worker.
#[derive(Clone, Debug)]
pub struct CloudflareStorageAdapter {
    functions: Functions,
    public_base_url: String,
    action: String,
    download_url_expires_in: Duration,
    transfer_timeout: Duration,
}

impl CloudflareStorageAdapter {
    /// `functions` sends the requests (with the user's token) and `public_base_url` is the
    /// public domain of the bucket (the worker's `STORAGE_PUBLIC_BASE_URL`).
    pub fn new(functions: Functions, public_base_url: impl Into<String>) -> Self {
        Self {
            functions,
            public_base_url: public_base_url.into().trim_end_matches('/').to_owned(),
            action: "storage_cloudflare".into(),
            download_url_expires_in: Duration::from_secs(60 * 60),
            transfer_timeout: Duration::from_secs(120),
        }
    }

    /// Worker path (`storage_cloudflare` by default).
    pub fn with_action(mut self, action: impl Into<String>) -> Self {
        self.action = action.into();
        self
    }

    /// Lifetime of [`StorageAdapter::fetch_download_uri`] URLs (1 hour by default).
    pub fn with_download_url_expires_in(mut self, expires_in: Duration) -> Self {
        self.download_url_expires_in = expires_in;
        self
    }

    /// Timeout of uploads and downloads (2 minutes by default).
    pub fn with_transfer_timeout(mut self, timeout: Duration) -> Self {
        self.transfer_timeout = timeout;
        self
    }

    fn public_uri(&self, path: &str) -> String {
        format!("{}/{}", self.public_base_url, encode_path(path))
    }

    async fn request(
        &self,
        operation: &str,
        path: &str,
        fields: Map<String, Value>,
        timeout: Duration,
    ) -> StorageResult<Value> {
        let mut body = Map::new();
        body.insert("operation".into(), operation.into());
        body.insert("path".into(), path.into());
        body.extend(fields);
        let request = FunctionRequest::new(self.action.clone())
            .with_body(Value::Object(body))
            .with_timeout(timeout);
        let response = self.functions.send(request).await.map_err(map_error)?;
        match response.get("status").and_then(Value::as_u64) {
            Some(status) if status >= 400 => Err(map_error(ApiError::Http {
                status: status as u16,
                body: response.to_string(),
            })),
            _ => Ok(response),
        }
    }
}

impl StorageAdapter for CloudflareStorageAdapter {
    /// `{public_base_url}/{path}`, built without a request.
    async fn fetch_public_uri(&self, path: &str) -> StorageResult<String> {
        Ok(self.public_uri(&normalize_path(path)?))
    }

    async fn fetch_download_uri(&self, path: &str) -> StorageResult<String> {
        let path = normalize_path(path)?;
        let mut fields = Map::new();
        fields.insert(
            "expiresIn".into(),
            self.download_url_expires_in.as_secs().max(1).into(),
        );
        let response = self
            .request("downloadUrl", &path, fields, self.transfer_timeout)
            .await?;
        meta_string(&response, "downloadUri").ok_or_else(|| invalid_response("downloadUri"))
    }

    async fn download(&self, path: &str, local: Option<&Path>) -> StorageResult<LocalFile> {
        let path = normalize_path(path)?;
        let response = self
            .request("get", &path, Map::new(), self.transfer_timeout)
            .await?;
        let bytes = response
            .get("binary")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid_response("binary"))
            .and_then(|binary| {
                BASE64
                    .decode(binary)
                    .map_err(|e| invalid_response(&format!("binary ({e})")))
            })?;
        if let Some(local) = local {
            write_file(local, &bytes)?;
        }
        Ok(LocalFile {
            path: local.map(Path::to_path_buf),
            bytes: Some(bytes),
            mime_type: meta_string(&response, "contentType")
                .or_else(|| resolve_mime_type(&path, None)),
        })
    }

    async fn upload(
        &self,
        local: &Path,
        path: &str,
        mime_type: Option<&str>,
    ) -> StorageResult<RemoteFile> {
        let bytes = std::fs::read(local)?;
        self.upload_bytes(&bytes, path, mime_type).await
    }

    async fn upload_bytes(
        &self,
        bytes: &[u8],
        path: &str,
        mime_type: Option<&str>,
    ) -> StorageResult<RemoteFile> {
        let path = normalize_path(path)?;
        let mime_type = resolve_mime_type(&path, mime_type);
        let mut fields = Map::new();
        fields.insert("binary".into(), BASE64.encode(bytes).into());
        if let Some(mime_type) = &mime_type {
            fields.insert("meta".into(), json!({ "contentType": mime_type }));
        }
        self.request("put", &path, fields, self.transfer_timeout)
            .await?;
        Ok(RemoteFile {
            uri: self.public_uri(&path),
            mime_type,
        })
    }

    async fn delete(&self, path: &str) -> StorageResult<()> {
        let path = normalize_path(path)?;
        let timeout = self.transfer_timeout;
        self.request("delete", &path, Map::new(), timeout).await?;
        Ok(())
    }
}

/// Maps worker errors ([`ApiError`] or [`FunctionsError`]) to [`StorageError`].
///
/// [`FunctionsError`]: masamune_whisker_functions::FunctionsError
pub fn map_error(error: impl Into<ApiError>) -> StorageError {
    match error.into() {
        ApiError::NotSignedIn => StorageError::Unauthorized,
        ApiError::Http { status: 404, .. } => StorageError::NotFound,
        ApiError::Http {
            status: 401 | 403, ..
        } => StorageError::Unauthorized,
        ApiError::Http { status: 400, body } => StorageError::InvalidArgument(body),
        error if error.is_transient() => StorageError::Unavailable(error.to_string()),
        ApiError::Http { status, body } => StorageError::Backend {
            code: status.to_string(),
            message: body,
        },
        error => StorageError::Backend {
            code: "worker".into(),
            message: error.to_string(),
        },
    }
}

fn meta_string(response: &Value, key: &str) -> Option<String> {
    response.get("meta")?.get(key)?.as_str().map(str::to_owned)
}

fn invalid_response(field: &str) -> StorageError {
    StorageError::Backend {
        code: "invalid-response".into(),
        message: format!("the storage worker returned no valid `{field}`"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use masamune_whisker_functions::{Auth, RestApiFunctionsAdapter, RuntimeFunctionsAdapter};
    use wiremock::matchers::{body_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn adapter(server: &MockServer) -> CloudflareStorageAdapter {
        let functions =
            Functions::new(RestApiFunctionsAdapter::new(server.uri()).with_auth(Auth::fixed("t")));
        CloudflareStorageAdapter::new(functions, "https://files.example.com/")
    }

    #[tokio::test]
    async fn uploads_base64_with_the_content_type() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/storage_cloudflare"))
            .and(header("authorization", "Bearer t"))
            .and(body_json(json!({
                "operation": "put",
                "path": "users/a b/avatar.png",
                "binary": "cG5n",
                "meta": {"contentType": "image/png"},
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"status": 200})))
            .mount(&server)
            .await;
        let file = adapter(&server)
            .upload_bytes(b"png", "/users/a b/avatar.png", None)
            .await
            .unwrap();
        assert_eq!(file.uri, "https://files.example.com/users/a%20b/avatar.png");
        assert_eq!(file.mime_type.as_deref(), Some("image/png"));
    }

    #[tokio::test]
    async fn downloads_into_memory_and_files() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(body_json(json!({"operation": "get", "path": "docs/a.bin"})))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "status": 200,
                "binary": "aGVsbG8=",
                "meta": {"contentType": "text/plain"},
            })))
            .mount(&server)
            .await;
        let storage = adapter(&server);
        let file = storage.download("docs/a.bin", None).await.unwrap();
        assert_eq!(file.bytes.as_deref(), Some(&b"hello"[..]));
        assert_eq!(file.mime_type.as_deref(), Some("text/plain"));

        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("cache/a.bin");
        storage.download("docs/a.bin", Some(&target)).await.unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"hello");
    }

    #[tokio::test]
    async fn fetches_download_urls_with_the_expiry() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(body_json(json!({
                "operation": "downloadUrl",
                "path": "a.png",
                "expiresIn": 600,
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "status": 200,
                "meta": {"downloadUri": "https://edge/download/a.png?expires=1&signature=s"},
            })))
            .mount(&server)
            .await;
        let uri = adapter(&server)
            .with_download_url_expires_in(Duration::from_secs(600))
            .fetch_download_uri("a.png")
            .await
            .unwrap();
        assert_eq!(uri, "https://edge/download/a.png?expires=1&signature=s");
    }

    #[tokio::test]
    async fn maps_worker_errors() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(body_json(json!({"operation": "get", "path": "missing"})))
            .respond_with(
                ResponseTemplate::new(404)
                    .set_body_json(json!({"status": 404, "error": "File not found"})),
            )
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(body_json(json!({"operation": "delete", "path": "locked"})))
            .respond_with(ResponseTemplate::new(403).set_body_json(json!({"error": "denied"})))
            .mount(&server)
            .await;
        let storage = adapter(&server);
        assert_eq!(
            storage.download("missing", None).await,
            Err(StorageError::NotFound)
        );
        assert_eq!(
            storage.delete("locked").await,
            Err(StorageError::Unauthorized)
        );
        assert!(map_error(ApiError::Timeout).is_transient());
        assert_eq!(
            storage.fetch_public_uri("/").await,
            Err(StorageError::InvalidArgument(
                "the storage path is empty".into()
            ))
        );
    }

    #[tokio::test]
    async fn works_with_any_functions_adapter() {
        // A callable-style backend: action name + JSON body, errors as structured failures.
        let functions = Functions::new(RuntimeFunctionsAdapter::new(|request| async move {
            assert_eq!(request.action, "storage_cloudflare");
            assert_eq!(request.path, None);
            let body = request.body.unwrap();
            match body["operation"].as_str() {
                Some("get") => Ok(json!({"binary": "aGk=", "meta": {"contentType": "text/plain"}})),
                _ => Err(masamune_whisker_functions::CallableFailure::Remote {
                    http_status: 403,
                    status: "PERMISSION_DENIED".into(),
                    message: "denied".into(),
                    details: Value::Null,
                }
                .into()),
            }
        }));
        let storage = CloudflareStorageAdapter::new(functions, "https://files.example.com");
        let file = storage.download("a.txt", None).await.unwrap();
        assert_eq!(file.bytes.as_deref(), Some(&b"hi"[..]));
        assert_eq!(
            storage.delete("a.txt").await,
            Err(StorageError::Unauthorized)
        );
    }
}
