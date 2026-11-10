use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

use crate::{
    LocalFile, RemoteFile, StorageAdapter, StorageError, StorageResult, normalize_path,
    resolve_mime_type, write_file,
};

#[derive(Clone, Debug)]
struct Object {
    bytes: Vec<u8>,
    mime_type: Option<String>,
}

/// In-memory storage for tests and previews (`RuntimeStorageAdapter` in Flutter).
///
/// Clones share the same objects. URLs are `memory://<path>`.
#[derive(Clone, Debug, Default)]
pub struct RuntimeStorageAdapter {
    objects: Rc<RefCell<HashMap<String, Object>>>,
}

impl RuntimeStorageAdapter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Starts with `data` (path → bytes).
    pub fn with_data<P: AsRef<str>>(data: impl IntoIterator<Item = (P, Vec<u8>)>) -> Self {
        let adapter = Self::new();
        for (path, bytes) in data {
            let path = normalize_path(path.as_ref()).expect("invalid storage path");
            adapter.objects.borrow_mut().insert(
                path.clone(),
                Object {
                    bytes,
                    mime_type: resolve_mime_type(&path, None),
                },
            );
        }
        adapter
    }

    /// Bytes stored at `path`.
    pub fn get(&self, path: &str) -> Option<Vec<u8>> {
        let path = normalize_path(path).ok()?;
        self.objects.borrow().get(&path).map(|o| o.bytes.clone())
    }

    pub fn contains(&self, path: &str) -> bool {
        self.get(path).is_some()
    }

    fn uri(path: &str) -> String {
        format!("memory://{path}")
    }
}

impl StorageAdapter for RuntimeStorageAdapter {
    async fn fetch_public_uri(&self, path: &str) -> StorageResult<String> {
        Ok(Self::uri(&normalize_path(path)?))
    }

    async fn fetch_download_uri(&self, path: &str) -> StorageResult<String> {
        let path = normalize_path(path)?;
        if !self.objects.borrow().contains_key(&path) {
            return Err(StorageError::NotFound);
        }
        Ok(Self::uri(&path))
    }

    async fn download(&self, path: &str, local: Option<&Path>) -> StorageResult<LocalFile> {
        let path = normalize_path(path)?;
        let object = self
            .objects
            .borrow()
            .get(&path)
            .cloned()
            .ok_or(StorageError::NotFound)?;
        if let Some(local) = local {
            write_file(local, &object.bytes)?;
        }
        Ok(LocalFile {
            path: local.map(Path::to_path_buf),
            bytes: Some(object.bytes),
            mime_type: object.mime_type,
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
        self.objects.borrow_mut().insert(
            path.clone(),
            Object {
                bytes: bytes.to_vec(),
                mime_type: mime_type.clone(),
            },
        );
        Ok(RemoteFile {
            uri: Self::uri(&path),
            mime_type,
        })
    }

    async fn delete(&self, path: &str) -> StorageResult<()> {
        self.objects.borrow_mut().remove(&normalize_path(path)?);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_executor::block_on;

    #[test]
    fn round_trips_objects() {
        block_on(async {
            let storage = RuntimeStorageAdapter::new();
            let file = storage
                .upload_bytes(b"png", "/users/a/avatar.png", None)
                .await
                .unwrap();
            assert_eq!(file.uri, "memory://users/a/avatar.png");
            assert_eq!(file.mime_type.as_deref(), Some("image/png"));

            let shared = storage.clone();
            let local = shared.download("users/a/avatar.png", None).await.unwrap();
            assert_eq!(local.bytes.as_deref(), Some(&b"png"[..]));

            storage.delete("users/a/avatar.png").await.unwrap();
            assert_eq!(
                shared.download("users/a/avatar.png", None).await,
                Err(StorageError::NotFound)
            );
            assert_eq!(
                shared.fetch_download_uri("users/a/avatar.png").await,
                Err(StorageError::NotFound)
            );
        });
    }

    #[test]
    fn reads_and_writes_local_files() {
        block_on(async {
            let dir = tempfile::tempdir().unwrap();
            let source = dir.path().join("source.txt");
            std::fs::write(&source, b"hello").unwrap();
            let storage = RuntimeStorageAdapter::with_data([("seed.json", b"{}".to_vec())]);
            assert!(storage.contains("/seed.json"));

            storage
                .upload(&source, "docs/hello.txt", None)
                .await
                .unwrap();
            let target = dir.path().join("nested/hello.txt");
            let local = storage
                .download("docs/hello.txt", Some(&target))
                .await
                .unwrap();
            assert_eq!(local.path.as_deref(), Some(target.as_path()));
            assert_eq!(local.mime_type.as_deref(), Some("text/plain"));
            assert_eq!(std::fs::read(&target).unwrap(), b"hello");
        });
    }
}
