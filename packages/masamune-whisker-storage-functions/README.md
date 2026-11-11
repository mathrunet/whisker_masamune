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

## URLs and authorization

Every download and URL lookup sends a new callable request using the current credentials.
The adapter does not retain bytes or metadata between operations, including across clones
and user changes. The callable has no URL operation, so URL lookups download the file to
obtain `publicUri` / `downloadUri` from its metadata. This increases network requests, and
URL lookup requires download permission. `clear_cache` remains a compatibility no-op.

An `upload` whose local path is already a URL (`http…`) returns it unchanged, as in Flutter.
