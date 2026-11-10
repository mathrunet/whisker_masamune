# masamune-whisker-storage-functions

Files through the Masamune storage Functions callable — the Whisker counterpart of
`FunctionsStorageAdapter` (`masamune_storage_functions`).

`FunctionsStorageAdapter` implements `masamune_whisker_storage::StorageAdapter` by calling
`storage_firebase` (`Functions.storageFirebase` of `@mathrunet/masamune_storage`) through a
`Functions` handle. The app holds no storage credentials; the server reads and writes the bucket
and can check the caller first. The crate depends only on `masamune-whisker-functions` and
`masamune-whisker-storage`; the app picks the backend adapter and its authentication.

```rust,ignore
use masamune_whisker_storage_functions::{Functions, FunctionsStorageAdapter, StorageAdapter};

let functions = Functions::new(FirebaseFunctionsAdapter::new(..).with_firebase_auth());
let storage = FunctionsStorageAdapter::new(functions, "my-project.firebasestorage.app");
let file = storage.upload_bytes(&png, "public/alice/avatar.png", None).await?;
let image_url = file.uri; // `publicUri` returned by the server
```

`with_action` changes the callable name and `with_transfer_timeout` the deadline of every request
(2 minutes by default).

## Wire format

Every call is a POST callable (no REST path or query), so it works with the Firebase callable
adapter. `path` is the bucket and the normalized storage path joined by `/`; files travel
base64-encoded inside JSON, so keep them to a few megabytes.

| Operation | Input | Response |
|---|---|---|
| Download | `{"method": "get", "path": "{bucket}/public/u1/a.png"}` | `{"status": 200, "binary": "<base64>", "meta": {"contentType", "size", "downloadUri", "publicUri", ..}}` (`{"status": 404}` when missing) |
| Upload | `{"method": "post", "path": "{bucket}/public/u1/a.png", "binary": "<base64>", "meta": {"contentType": "image/png"}}` | `{"status": 200, "meta": {"contentType", "downloadUri", "publicUri"}}` |
| Delete | `{"method": "delete", "path": "{bucket}/public/u1/a.png"}` | `{"status": 200}` |

`meta` is only sent when the MIME type is known (given, or guessed from the extension). A
`status` outside 2xx is an error (`404` → `StorageError::NotFound`); callable errors keep their
HTTP status (`permission-denied` → `StorageError::Unauthorized`).

## URLs and the cache

The callable has no URL operation, so `fetch_public_uri` and `fetch_download_uri` return
`publicUri` / `downloadUri` from the `meta` of the last upload or download of the path. Like
Flutter's `FunctionsStorageDatabase`, the adapter keeps that `meta` and the downloaded bytes in
memory (shared by clones), and `download` answers from it. For a path that was not transferred
yet, asking for a URL downloads the file first (Flutter fails instead). `clear_cache` forgets
everything; `delete` forgets the path.

An `upload` whose local path is already a URL (`http…`) returns it unchanged, as in Flutter.
