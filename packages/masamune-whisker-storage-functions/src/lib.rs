//! Files through the Masamune storage Functions callable as a [`StorageAdapter`]
//! (`FunctionsStorageAdapter` of `masamune_storage_functions` in Flutter,
//! `masamune_storage_functions/lib/adapter/others/functions_storage_adapter.dart`).
//!
//! The app holds no storage credentials: every operation is the callable `storage_firebase`
//! (`Functions.storageFirebase` of `@mathrunet/masamune_storage`), which reads or writes the
//! bucket with the server's own credentials and can check the caller first. Requests go through
//! a [`Functions`] handle, so this crate depends only on the adapter interface; the app chooses
//! the backend (usually the Firebase callable adapter with the app's authentication):
//!
//! ```ignore
//! let functions = Functions::new(FirebaseFunctionsAdapter::new(..).with_firebase_auth());
//! let storage = FunctionsStorageAdapter::new(functions, "my-project.firebasestorage.app");
//! let file = storage.upload_bytes(&png, "public/alice/avatar.png", None).await?;
//! ```
//!
//! # Wire format
//!
//! Every request is a POST callable (no REST path or query) whose input is
//! `{"method": "get|post|delete", "path": "{bucket}/{path}"}`; uploads add
//! `"binary": "<base64>"` and `"meta": {"contentType": "..."}`. Responses are
//! `{"status": 200, "binary": "<base64>", "meta": {"contentType", "downloadUri", "publicUri", ..}}`
//! (`binary` only for `get`, `meta` for `get` and `post`). A `status` outside 2xx is an error
//! (`404` → [`StorageError::NotFound`]), as in Flutter.
//!
//! Files travel base64-encoded inside JSON, so keep them to a few megabytes.
//!
//! # URLs and the cache
//!
//! The callable has no URL operation: the public and download URLs come from the `meta` of the
//! last upload or download of the path. Like Flutter's `FunctionsStorageDatabase`, the adapter
//! keeps that `meta` and the downloaded bytes in memory (shared by clones of the adapter), and
//! [`StorageAdapter::download`] answers from it. When a URL is asked for a path that was not
//! transferred yet, the adapter downloads it first (Flutter fails instead). Call
//! [`FunctionsStorageAdapter::clear_cache`] to forget everything.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
pub use masamune_whisker_functions::Functions;
use masamune_whisker_functions::{ApiError, FunctionRequest};
pub use masamune_whisker_storage::{
    LocalFile, RemoteFile, StorageAdapter, StorageError, StorageResult,
};
use masamune_whisker_storage::{normalize_path, resolve_mime_type, write_file};
use serde_json::{Map, Value, json};

/// `meta` key of the expiring download URL.
pub const DOWNLOAD_URI_KEY: &str = "downloadUri";
/// `meta` key of the public URL.
pub const PUBLIC_URI_KEY: &str = "publicUri";
/// `meta` key of the MIME type.
pub const CONTENT_TYPE_KEY: &str = "contentType";

/// Method of the storage callable (`ApiMethod.name` in Flutter).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorageMethod {
    Get,
    Post,
    Delete,
}

impl StorageMethod {
    pub fn name(self) -> &'static str {
        match self {
            StorageMethod::Get => "get",
            StorageMethod::Post => "post",
            StorageMethod::Delete => "delete",
        }
    }
}

/// What the adapter remembers about a path (`FunctionsStorageDatabaseItem`).
#[derive(Clone, Debug, Default, PartialEq)]
struct CacheEntry {
    bytes: Option<Vec<u8>>,
    meta: Map<String, Value>,
}

/// Files through the Masamune storage Functions callable.
#[derive(Clone, Debug)]
pub struct FunctionsStorageAdapter {
    functions: Functions,
    bucket: String,
    action: String,
    transfer_timeout: Duration,
    cache: Rc<RefCell<HashMap<String, CacheEntry>>>,
}

impl FunctionsStorageAdapter {
    /// `functions` sends the requests (with the user's token) and `bucket` is the storage bucket
    /// put in front of every path (`bucketName` in Flutter, such as
    /// `my-project.firebasestorage.app`).
    pub fn new(functions: Functions, bucket: impl Into<String>) -> Self {
        Self {
            functions,
            bucket: trim_query(&bucket.into()).trim_matches('/').to_owned(),
            action: "storage_firebase".into(),
            transfer_timeout: Duration::from_secs(120),
            cache: Rc::default(),
        }
    }

    /// Callable name (`storage_firebase` by default).
    pub fn with_action(mut self, action: impl Into<String>) -> Self {
        self.action = action.into();
        self
    }

    /// Timeout of every request (2 minutes by default).
    pub fn with_transfer_timeout(mut self, timeout: Duration) -> Self {
        self.transfer_timeout = timeout;
        self
    }

    pub fn functions(&self) -> &Functions {
        &self.functions
    }

    pub fn bucket(&self) -> &str {
        &self.bucket
    }

    pub fn action(&self) -> &str {
        &self.action
    }

    /// Forgets the cached `meta` and bytes of every path.
    pub fn clear_cache(&self) {
        self.cache.borrow_mut().clear();
    }

    /// Callable input `{method, path: "{bucket}/{path}"[, binary, meta]}` for a normalized
    /// `path`.
    pub fn request(
        &self,
        method: StorageMethod,
        path: &str,
        binary: Option<&[u8]>,
        meta: Option<Map<String, Value>>,
    ) -> StorageResult<FunctionRequest> {
        if self.bucket.is_empty() {
            return Err(StorageError::InvalidArgument(
                "the storage bucket is empty".into(),
            ));
        }
        let mut body = Map::new();
        body.insert("method".into(), method.name().into());
        body.insert("path".into(), format!("{}/{path}", self.bucket).into());
        if let Some(binary) = binary {
            body.insert("binary".into(), BASE64.encode(binary).into());
        }
        if let Some(meta) = meta {
            body.insert("meta".into(), Value::Object(meta));
        }
        Ok(FunctionRequest::new(self.action.clone())
            .with_body(Value::Object(body))
            .with_timeout(self.transfer_timeout))
    }

    /// Sends a request and returns the response of a 2xx `status`.
    async fn send(&self, request: FunctionRequest) -> StorageResult<Map<String, Value>> {
        let response = self.functions.send(request).await.map_err(map_error)?;
        decode_response(response)
    }

    /// Downloads `path` (normalized) from the server and caches its bytes and `meta`.
    async fn fetch(&self, path: &str) -> StorageResult<CacheEntry> {
        let response = self
            .send(self.request(StorageMethod::Get, path, None, None)?)
            .await?;
        let bytes = response
            .get("binary")
            .and_then(Value::as_str)
            .filter(|binary| !binary.is_empty())
            .ok_or_else(|| invalid_response("binary"))
            .and_then(|binary| {
                BASE64
                    .decode(binary)
                    .map_err(|e| invalid_response(&format!("binary ({e})")))
            })?;
        let entry = CacheEntry {
            bytes: Some(bytes),
            meta: response_meta(&response),
        };
        self.cache
            .borrow_mut()
            .insert(path.to_owned(), entry.clone());
        Ok(entry)
    }

    /// `meta[key]` of `path`, downloading it when it is not cached.
    async fn meta_uri(&self, path: &str, key: &str) -> StorageResult<String> {
        let path = normalize_path(path)?;
        let cached = self
            .cache
            .borrow()
            .get(&path)
            .and_then(|entry| meta_string(&entry.meta, key));
        if let Some(uri) = cached {
            return Ok(uri);
        }
        let entry = self.fetch(&path).await?;
        meta_string(&entry.meta, key).ok_or_else(|| invalid_response(key))
    }
}

impl StorageAdapter for FunctionsStorageAdapter {
    /// `publicUri` of the last transfer of `path` (see the crate docs).
    async fn fetch_public_uri(&self, path: &str) -> StorageResult<String> {
        self.meta_uri(path, PUBLIC_URI_KEY).await
    }

    /// `downloadUri` of the last transfer of `path` (see the crate docs).
    async fn fetch_download_uri(&self, path: &str) -> StorageResult<String> {
        self.meta_uri(path, DOWNLOAD_URI_KEY).await
    }

    /// Answers from the cache when the bytes of `path` were downloaded before.
    async fn download(&self, path: &str, local: Option<&Path>) -> StorageResult<LocalFile> {
        let path = normalize_path(path)?;
        let cached = self
            .cache
            .borrow()
            .get(&path)
            .filter(|entry| entry.bytes.is_some())
            .cloned();
        let entry = match cached {
            Some(entry) => entry,
            None => self.fetch(&path).await?,
        };
        let bytes = entry.bytes.unwrap_or_default();
        if let Some(local) = local {
            write_file(local, &bytes)?;
        }
        Ok(LocalFile {
            path: local.map(Path::to_path_buf),
            bytes: Some(bytes),
            mime_type: meta_string(&entry.meta, CONTENT_TYPE_KEY)
                .or_else(|| resolve_mime_type(&path, None)),
        })
    }

    /// A `local` that is already a URL (`http…`) is returned as is, as in Flutter.
    async fn upload(
        &self,
        local: &Path,
        path: &str,
        mime_type: Option<&str>,
    ) -> StorageResult<RemoteFile> {
        if let Some(uri) = local.to_str().filter(|local| local.starts_with("http")) {
            return Ok(RemoteFile {
                uri: uri.to_owned(),
                mime_type: resolve_mime_type(path, mime_type),
            });
        }
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
        if bytes.is_empty() {
            return Err(StorageError::InvalidArgument(
                "the uploaded file is empty".into(),
            ));
        }
        let mime_type = resolve_mime_type(&path, mime_type);
        let sent_meta = mime_type.as_ref().map(|mime_type| {
            let mut meta = Map::new();
            meta.insert(CONTENT_TYPE_KEY.into(), json!(mime_type));
            meta
        });
        let response = self
            .send(self.request(StorageMethod::Post, &path, Some(bytes), sent_meta)?)
            .await?;
        let meta = response_meta(&response);
        let uri =
            meta_string(&meta, PUBLIC_URI_KEY).ok_or_else(|| invalid_response(PUBLIC_URI_KEY))?;
        let mime_type = meta_string(&meta, CONTENT_TYPE_KEY).or(mime_type);
        // Like Flutter, an upload keeps only the `meta` (the next download fetches the bytes).
        self.cache
            .borrow_mut()
            .insert(path, CacheEntry { bytes: None, meta });
        Ok(RemoteFile { uri, mime_type })
    }

    async fn delete(&self, path: &str) -> StorageResult<()> {
        let path = normalize_path(path)?;
        self.send(self.request(StorageMethod::Delete, &path, None, None)?)
            .await?;
        self.cache.borrow_mut().remove(&path);
        Ok(())
    }
}

/// Checks the `status` of a callable response (2xx, as in Flutter) and returns the response.
pub fn decode_response(response: Value) -> StorageResult<Map<String, Value>> {
    let Value::Object(response) = response else {
        return Err(invalid_response("status"));
    };
    let status = match response.get("status") {
        Some(Value::Number(n)) => n.as_u64(),
        Some(Value::String(s)) => s.parse().ok(),
        _ => None,
    }
    .ok_or_else(|| invalid_response("status"))?;
    if !(200..300).contains(&status) {
        let message = response
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("storage request failed")
            .to_owned();
        return Err(map_error(ApiError::Http {
            status: u16::try_from(status).unwrap_or(u16::MAX),
            body: message,
        }));
    }
    Ok(response)
}

/// Maps Functions errors ([`ApiError`] or [`FunctionsError`]) to [`StorageError`].
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
            code: "functions".into(),
            message: error.to_string(),
        },
    }
}

fn response_meta(response: &Map<String, Value>) -> Map<String, Value> {
    match response.get("meta") {
        Some(Value::Object(meta)) => meta.clone(),
        _ => Map::new(),
    }
}

fn meta_string(meta: &Map<String, Value>, key: &str) -> Option<String> {
    meta.get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn trim_query(value: &str) -> &str {
    value.split(['?', '#']).next().unwrap_or_default()
}

fn invalid_response(field: &str) -> StorageError {
    StorageError::Backend {
        code: "invalid-response".into(),
        message: format!("the storage Functions returned no valid `{field}`"),
    }
}

#[cfg(test)]
mod tests;
