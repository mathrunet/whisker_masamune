//! Cloud Storage for Firebase as a [`StorageAdapter`] (`katana_storage_firebase` in Flutter).
//!
//! Uses the default app's default bucket through `whisker-firebase` (feature `storage`), so it
//! works on Android and iOS; other platforms fail with [`StorageError::Unsupported`]. Apps need
//! `whisker-firebase-storage` in their `[patch.crates-io]` next to the other whisker crates.
//!
//! ```ignore
//! let storage = FirebaseStorageAdapter::new();
//! let file = storage.upload_bytes(&png, "users/alice/avatar.png", None).await?;
//! let bytes = storage.download("users/alice/avatar.png", None).await?.bytes;
//! ```

use std::path::Path;
use std::time::SystemTime;

pub use masamune_whisker_storage::{
    LocalFile, RemoteFile, StorageAdapter, StorageError, StorageResult,
};
use masamune_whisker_storage::{encode_uri_component, normalize_path, resolve_mime_type};
use whisker_firebase::FirebaseApp;
use whisker_firebase::FirebaseError;
use whisker_firebase::storage::{SettableMetadata, Storage, StorageReference};

/// Cloud Storage for Firebase.
#[derive(Clone, Debug)]
pub struct FirebaseStorageAdapter {
    max_download_size: u64,
}

impl Default for FirebaseStorageAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl FirebaseStorageAdapter {
    /// Downloads into memory up to 32 MiB.
    pub fn new() -> Self {
        Self {
            max_download_size: 32 * 1024 * 1024,
        }
    }

    /// Largest object [`StorageAdapter::download`] reads into memory.
    pub fn with_max_download_size(mut self, bytes: u64) -> Self {
        self.max_download_size = bytes;
        self
    }

    fn reference(path: &str) -> StorageResult<(String, StorageReference)> {
        let path = normalize_path(path)?;
        let reference = Storage::instance().map_err(map_error)?.reference(&path);
        Ok((path, reference))
    }

    fn bucket() -> StorageResult<String> {
        FirebaseApp::initialize()
            .map_err(map_error)?
            .storage_bucket
            .ok_or_else(|| StorageError::Backend {
                code: "no-default-bucket".into(),
                message: "the Firebase configuration has no storage bucket".into(),
            })
    }

    fn metadata(path: &str, mime_type: Option<&str>) -> (Option<String>, SettableMetadata) {
        let mime_type = resolve_mime_type(path, mime_type);
        let metadata = match &mime_type {
            Some(mime_type) => SettableMetadata::new().content_type(mime_type),
            None => SettableMetadata::new(),
        };
        (mime_type, metadata)
    }
}

impl StorageAdapter for FirebaseStorageAdapter {
    /// `https://firebasestorage.googleapis.com/v0/b/{bucket}/o/{path}?alt=media`, built without
    /// a request. It only works when the storage rules allow public reads.
    async fn fetch_public_uri(&self, path: &str) -> StorageResult<String> {
        Ok(public_uri(&Self::bucket()?, &normalize_path(path)?))
    }

    async fn fetch_download_uri(&self, path: &str) -> StorageResult<String> {
        let (_, reference) = Self::reference(path)?;
        reference.download_url().await.map_err(map_error)
    }

    /// With `local`, a file that is not older than the object is reused instead of downloaded.
    async fn download(&self, path: &str, local: Option<&Path>) -> StorageResult<LocalFile> {
        let (path, reference) = Self::reference(path)?;
        let Some(local) = local else {
            let bytes = reference
                .get_bytes(self.max_download_size)
                .await
                .map_err(map_error)?;
            return Ok(LocalFile {
                path: None,
                bytes: Some(bytes),
                mime_type: resolve_mime_type(&path, None),
            });
        };
        let metadata = reference.metadata().await.map_err(map_error)?;
        let mime_type = metadata
            .settable
            .content_type
            .clone()
            .or_else(|| resolve_mime_type(&path, None));
        let modified = std::fs::metadata(local).and_then(|m| m.modified()).ok();
        let updated = metadata.updated.map(|t| t.to_system_time());
        if !is_fresh(modified, updated) {
            if let Some(parent) = local.parent().filter(|p| !p.as_os_str().is_empty()) {
                std::fs::create_dir_all(parent)?;
            }
            reference.write_to_file(local).await.map_err(map_error)?;
        }
        Ok(LocalFile {
            path: Some(local.to_path_buf()),
            bytes: None,
            mime_type,
        })
    }

    /// A `local` that is already an `http(s)` URL is returned as is without uploading.
    async fn upload(
        &self,
        local: &Path,
        path: &str,
        mime_type: Option<&str>,
    ) -> StorageResult<RemoteFile> {
        if let Some(url) = local
            .to_str()
            .filter(|s| s.starts_with("http://") || s.starts_with("https://"))
        {
            return Ok(RemoteFile {
                uri: url.to_owned(),
                mime_type: resolve_mime_type(url, mime_type),
            });
        }
        let (path, reference) = Self::reference(path)?;
        let (mime_type, metadata) = Self::metadata(&path, mime_type);
        reference
            .put_file_with(local, &metadata)
            .await
            .map_err(map_error)?;
        Ok(RemoteFile {
            uri: public_uri(&Self::bucket()?, &path),
            mime_type,
        })
    }

    async fn upload_bytes(
        &self,
        bytes: &[u8],
        path: &str,
        mime_type: Option<&str>,
    ) -> StorageResult<RemoteFile> {
        let (path, reference) = Self::reference(path)?;
        let (mime_type, metadata) = Self::metadata(&path, mime_type);
        reference
            .put_bytes_with(bytes, &metadata)
            .await
            .map_err(map_error)?;
        Ok(RemoteFile {
            uri: public_uri(&Self::bucket()?, &path),
            mime_type,
        })
    }

    async fn delete(&self, path: &str) -> StorageResult<()> {
        let (_, reference) = Self::reference(path)?;
        reference.delete().await.map_err(map_error)
    }
}

/// Public URL of a normalized `path` in `bucket`.
pub fn public_uri(bucket: &str, path: &str) -> String {
    format!(
        "https://firebasestorage.googleapis.com/v0/b/{bucket}/o/{}?alt=media",
        encode_uri_component(path)
    )
}

/// Maps Firebase Storage error codes to [`StorageError`].
pub fn map_error(error: FirebaseError) -> StorageError {
    match error.code.as_str() {
        "object-not-found" => StorageError::NotFound,
        "unauthenticated" | "unauthorized" => StorageError::Unauthorized,
        "unsupported-platform" => StorageError::Unsupported(error.message),
        "invalid-argument" => StorageError::InvalidArgument(error.message),
        "retry-limit-exceeded" | "unavailable" | "network-request-failed" => {
            StorageError::Unavailable(error.message)
        }
        _ => StorageError::Backend {
            code: error.code,
            message: error.message,
        },
    }
}

fn is_fresh(modified: Option<SystemTime>, updated: Option<SystemTime>) -> bool {
    matches!((modified, updated), (Some(modified), Some(updated)) if modified >= updated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn builds_public_uris() {
        assert_eq!(
            public_uri("app.firebasestorage.app", "users/a b/avatar.png"),
            "https://firebasestorage.googleapis.com/v0/b/app.firebasestorage.app/o/users%2Fa%20b%2Favatar.png?alt=media"
        );
    }

    #[test]
    fn maps_errors() {
        let error = |code: &str| FirebaseError::new("storage", code, "message");
        assert_eq!(map_error(error("object-not-found")), StorageError::NotFound);
        assert_eq!(map_error(error("unauthorized")), StorageError::Unauthorized);
        assert_eq!(
            map_error(error("unsupported-platform")),
            StorageError::Unsupported("message".into())
        );
        assert!(map_error(error("retry-limit-exceeded")).is_transient());
        assert_eq!(
            map_error(error("quota-exceeded")),
            StorageError::Backend {
                code: "quota-exceeded".into(),
                message: "message".into()
            }
        );
    }

    #[test]
    fn reuses_files_not_older_than_the_object() {
        let now = SystemTime::now();
        let earlier = now - Duration::from_secs(60);
        assert!(is_fresh(Some(now), Some(earlier)));
        assert!(is_fresh(Some(now), Some(now)));
        assert!(!is_fresh(Some(earlier), Some(now)));
        assert!(!is_fresh(None, Some(now)));
        assert!(!is_fresh(Some(now), None));
    }
}
