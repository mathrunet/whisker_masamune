//! Remote file storage behind one trait (`katana_storage` in Flutter).
//!
//! - [`StorageAdapter`]: uploads, downloads, deletes and URLs of objects at slash-separated
//!   paths. Implemented by `masamune-whisker-storage-firebase` (Cloud Storage for Firebase) and
//!   `masamune-whisker-storage-cloudflare` (Cloudflare R2 through the Masamune worker).
//! - [`RuntimeStorageAdapter`]: in-memory adapter for tests and previews.
//! - [`LocalFile`] / [`RemoteFile`]: results of downloads and uploads.
//!
//! ```ignore
//! async fn save_avatar(storage: &impl StorageAdapter, uid: &str, png: &[u8]) -> StorageResult<String> {
//!     let file = storage.upload_bytes(png, &format!("users/{uid}/avatar.png"), None).await?;
//!     Ok(file.uri)
//! }
//! ```

mod error;
mod mime;
mod runtime;

use std::path::{Path, PathBuf};

pub use error::{StorageError, StorageResult};
pub use mime::guess_mime_type;
pub use runtime::RuntimeStorageAdapter;

/// A backend that stores files at slash-separated paths (`users/alice/avatar.png`).
///
/// Paths are normalized with [`normalize_path`]. Futures run on the UI thread and do not need
/// to be `Send`.
#[allow(async_fn_in_trait)]
pub trait StorageAdapter {
    /// URL of the object for anyone allowed to read it (for example in an `Image`). Adapters
    /// that can build it without a request do not check that the object exists.
    async fn fetch_public_uri(&self, path: &str) -> StorageResult<String>;

    /// URL that downloads the object without further authentication (it may expire).
    async fn fetch_download_uri(&self, path: &str) -> StorageResult<String>;

    /// Downloads the object into memory, or into the file `local` (replacing it) when given.
    /// [`LocalFile::bytes`] may be `None` when the object was written to `local`.
    async fn download(&self, path: &str, local: Option<&Path>) -> StorageResult<LocalFile>;

    /// Uploads the file `local`, replacing any object at `path`. `mime_type` defaults to
    /// [`guess_mime_type`] of `path`.
    async fn upload(
        &self,
        local: &Path,
        path: &str,
        mime_type: Option<&str>,
    ) -> StorageResult<RemoteFile>;

    /// Uploads `bytes`, replacing any object at `path`. `mime_type` defaults to
    /// [`guess_mime_type`] of `path`.
    async fn upload_bytes(
        &self,
        bytes: &[u8],
        path: &str,
        mime_type: Option<&str>,
    ) -> StorageResult<RemoteFile>;

    async fn delete(&self, path: &str) -> StorageResult<()>;
}

/// A downloaded file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LocalFile {
    /// The file the object was written to.
    pub path: Option<PathBuf>,
    pub bytes: Option<Vec<u8>>,
    pub mime_type: Option<String>,
}

/// An uploaded object.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RemoteFile {
    /// Public URL of the object ([`StorageAdapter::fetch_public_uri`]).
    pub uri: String,
    pub mime_type: Option<String>,
}

/// Removes the query, the fragment and empty segments (`/a//b/?x=1` → `a/b`). Empty paths and
/// `..` segments are rejected.
pub fn normalize_path(path: &str) -> StorageResult<String> {
    let path = path.split(['?', '#']).next().unwrap_or_default();
    let segments: Vec<&str> = path
        .split('/')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if segments.is_empty() {
        return Err(StorageError::InvalidArgument(
            "the storage path is empty".into(),
        ));
    }
    if segments.contains(&"..") {
        return Err(StorageError::InvalidArgument(format!(
            "the storage path must not contain `..`: {path}"
        )));
    }
    Ok(segments.join("/"))
}

/// JavaScript's `encodeURIComponent`.
pub fn encode_uri_component(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'!'
            | b'~'
            | b'*'
            | b'\''
            | b'('
            | b')' => encoded.push(byte as char),
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

/// Encodes each segment of a normalized path and keeps the slashes.
pub fn encode_path(path: &str) -> String {
    path.split('/')
        .map(encode_uri_component)
        .collect::<Vec<_>>()
        .join("/")
}

/// `mime_type`, or the guess from the extension of `path`.
pub fn resolve_mime_type(path: &str, mime_type: Option<&str>) -> Option<String> {
    mime_type
        .map(str::to_owned)
        .or_else(|| guess_mime_type(path).map(str::to_owned))
}

/// Writes `bytes` to `path`, creating missing parent directories.
pub fn write_file(path: &Path, bytes: &[u8]) -> StorageResult<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_paths() {
        assert_eq!(normalize_path("/users//a/b.png/").unwrap(), "users/a/b.png");
        assert_eq!(normalize_path("a/b.png?alt=media#x").unwrap(), "a/b.png");
        assert!(matches!(
            normalize_path("/?x"),
            Err(StorageError::InvalidArgument(_))
        ));
        assert!(matches!(
            normalize_path("a/../b"),
            Err(StorageError::InvalidArgument(_))
        ));
    }

    #[test]
    fn encodes_like_javascript() {
        assert_eq!(encode_uri_component("a b/c+d"), "a%20b%2Fc%2Bd");
        assert_eq!(encode_uri_component("写真.png"), "%E5%86%99%E7%9C%9F.png");
        assert_eq!(encode_path("users/a b/x.png"), "users/a%20b/x.png");
    }

    #[test]
    fn resolves_mime_types() {
        assert_eq!(
            resolve_mime_type("a.PNG", None).as_deref(),
            Some("image/png")
        );
        assert_eq!(
            resolve_mime_type("a.png", Some("image/webp")).as_deref(),
            Some("image/webp")
        );
        assert_eq!(resolve_mime_type("a", None), None);
    }
}
