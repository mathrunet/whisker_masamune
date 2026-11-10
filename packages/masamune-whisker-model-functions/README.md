# masamune-whisker-model-functions

Firestore documents and collections through the Masamune model Functions — the Whisker
counterpart of `FunctionsModelAdapter` (`masamune_model_functions`).

`FunctionsStore` calls `document_model_firestore`, `collection_model_firestore` and
`aggregate_model_firestore` (the callables of `@mathrunet/masamune_firestore`) through a
`Functions` handle and implements `masamune_whisker_model::RemoteStore`, so it works behind the
on-device cache. The crate depends only on `masamune-whisker-functions` and
`masamune-whisker-model`; the app picks the backend adapter and its authentication.

```rust,ignore
use masamune_whisker_model::{CacheOptions, ModelCache, Query, ReadOptions};
use masamune_whisker_model_functions::{CachedFunctionsStore, Functions, FunctionsStore};

let functions = Functions::new(FirebaseFunctionsAdapter::new(..).with_firebase_auth());
let store = CachedFunctionsStore::new(
    FunctionsStore::new(functions),
    Some(ModelCache::shared()?),
    CacheOptions::new("functions"),
);
let path = format!("author/{author}/catalog/{catalog}/content/{content}/comment");
let comments = store
    .query("", &path, &Query::new().order_by("createdAt", true).limit(100), ReadOptions::reload())
    .await?;
let count = store.remote().count("", &path, Duration::from_secs(5)).await?;
```

## Mapping

- `database`: Firestore database id sent as `databaseId`. An empty string uses
  `FunctionsStoreConfig::database_id`; `None` sends `null` so the server decides (for example
  from the caller's identity).
- `table`: collection path (odd number of segments); `id`: document id.
- Rows carry their document id in `@uid` (`RemoteStore::id_field`). Saves add `@uid`, like
  Masamune documents.

## Wire format

Every call is a POST callable (no REST path or query), so it works with the Firebase callable
adapter. `data` is a JSON-encoded **string** in both directions.

| Operation | Input | Response |
|---|---|---|
| Document get | `{"path": "user_meta/u1", "method": "get", "databaseId": null}` | `{"status": 200, "data": "{\"name\":\"A\"}"}` (`"{}"` when missing → `None`) |
| Document save | `{"path": "user_meta/u1", "method": "post", "databaseId": null, "data": "{\"name\":\"A\",\"@uid\":\"u1\"}"}` | `{"status": 200}` |
| Document delete | `{"path": "user_meta/u1", "method": "delete", "databaseId": null}` | `{"status": 200}` |
| Collection get | `{"path": "a/x/comment", "method": "get", "databaseId": null}` | `{"status": 200, "data": "{\"c1\":{...},\"c2\":{...}}"}` |
| Collection save | `{"path": "a/x/comment", "method": "post", "databaseId": null, "data": "{\"c1\":{...}}"}` | `{"status": 200}` |
| Collection delete | `{"path": "a/x/comment", "method": "delete", "databaseId": null}` | `{"status": 200}` |
| Aggregate | `{"path": "a/x/comment", "method": "count", "databaseId": null}` (`sum` / `average` append the field: `item/price`) | `{"status": 200, "data": "{\"value\":3}"}` |

A `status` outside 2xx is an `ApiError::Http`; callable errors (such as `permission-denied`)
keep their HTTP status.

## Queries

The collection callable returns the whole collection. `FunctionsStore::query` applies the
`Query` (filters, ordering, limit) on the device. Masamune field values are compared through
`field_value::comparable`: timestamps by time, references by path, counters by value, so
`order_by("createdAt", true)` works on `ModelTimestamp` fields and
`equal_to("user", field_value::model_ref("user_meta/u1"))` matches references. Aggregates run on
the server and ignore filters. Transactions, batches and listeners are not available.

## Field values

Rows keep Masamune's `ModelFieldValue` maps as stored:

- `ModelTimestamp` / `ModelDate` / `ModelTime`: `{"@type": "ModelTimestamp", "@time": <µs>, "@source": "server"}`
- `ModelRefBase`: `{"@type": "ModelRefBase", "@ref": "user_meta/u1"}`
- `ModelCounter`: `{"@type": "ModelCounter", "@value": 3, "@increment": 0}`

`field_value` reads and builds them: `row_timestamp(&row, "createdAt")`, `timestamp_micros`,
`model_timestamp(time)` / `model_timestamp_now()`, `ref_path` / `ref_id` / `model_ref`,
`counter_value`.
