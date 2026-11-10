# masamune-whisker-functions

Typed external API calls for Whisker, modeled after Flutter Masamune's
`katana_functions`. Use an action to define application input/output, and select
an adapter at boot. Store the clonable `Functions` handle in AppState and pass it
to controllers. Calls need a Tokio runtime; from Whisker use `spawn_app` for work
that survives a page or `spawn_local` for page-local work. Futures are not `Send`.

```rust
use masamune_whisker_functions::{
    FunctionRequest, Functions, FunctionsAction, FunctionsResult,
    RestApiFunctionsAdapter,
};
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
struct Greeting { message: String }

struct Greet { name: String }
impl FunctionsAction for Greet {
    type Response = Greeting;
    fn request(&self) -> FunctionsResult<FunctionRequest> {
        let mut request = FunctionRequest::new("greet");
        request.body = Some(json!({"name": self.name}));
        Ok(request)
    }
}

async fn greet() -> FunctionsResult<String> {
    let functions = Functions::new(RestApiFunctionsAdapter::new("https://api.example.com"));
    let response = functions.execute(Greet { name: "Whisker".into() }).await?;
    Ok(response.message)
}
```

The default HTTP method is **POST**, unlike Flutter's Cloudflare adapter's default
GET. Set `request.method = Method::GET` explicitly for reads and use `request.query`
for URL parameters. `path` overrides the action name on REST/Cloudflare adapters;
path values are URL paths (percent-encode dynamic path segments). `query` values
are encoded automatically. JSON bodies may be objects, arrays, scalars or null.
An absent body sends no REST payload. Empty successful REST responses become null.

`FunctionsAction::decode` deserializes JSON into the associated `Response`; override
it for custom mapping. `on_error` can provide an application-specific fallback.
Errors from request construction, transport and decoding all reach this hook.
`FunctionsError::Api` preserves the existing `ApiError`, including HTTP status/body,
network, timeout and decode failures. Firebase errors preserve their status,
message and details in `FunctionsError::Callable`.

## Adapter switching

The same `FunctionsAction` can be passed to any adapter when its endpoint accepts
the same input/output contract. REST-only options (method/path/query) are rejected
by Firebase callable adapters. `worker_type` is used only by Cloudflare.

- `RestApiFunctionsAdapter`: generic JSON HTTP APIs, optional bearer `Auth`.
- `masamune-whisker-functions-cloudflare`: edge/region Workers and bearer auth.
- `masamune-whisker-functions-firebase`: Firebase callable protocol and App Check.
- `RuntimeFunctionsAdapter`: a callback receiving the complete request for tests.

No adapter retries automatically, including non-idempotent calls. A per-action
`timeout` overrides the adapter default (REST/Cloudflare 30s, Firebase 60s) and
includes token acquisition, HTTP dispatch and response reading. Action construction
and response conversion are application code and run outside this deadline.
Managed JSON/auth headers take precedence over custom headers when configured.
Tokens come from `masamune-whisker-auth`: pass any `AuthAdapter` wrapped in `Auth`
(for example `masamune_whisker_auth_firebase::firebase_auth()`), or
`Auth::from_fn(...)` to load a fresh token on each call. `Auth::fixed` is intended
for tests and explicitly fixed tokens.

## Runtime tests

```rust
use masamune_whisker_functions::{Functions, RuntimeFunctionsAdapter};
use serde_json::json;

let functions = Functions::new(RuntimeFunctionsAdapter::new(|request| async move {
    assert_eq!(request.action, "greet");
    Ok(json!({"message": "Hello"}))
}));
// functions.execute(Greet { name: "test".into() }).await?;
```

Return `Err(ApiError::Timeout.into())` to simulate failures. Runtime adapters use
the same action decoding/error hooks, but do not perform transport validation or
apply network deadlines. There is no process-global adapter or implicit success
for unmatched stub names; implement dispatch in the callback.

## Dependencies and platform support

Add `masamune-whisker-functions` and the selected backend crate using the same git
revision/tag as the rest of `whisker_masamune`. This crate is the shared HTTP
foundation: `ApiError`, `RetryPolicy` / `retry` / `with_timeout` and the reqwest
client live here. Backend crates (`masamune-whisker-turso`, `-tidb`,
`-storage-cloudflare`, …) accept a `Functions` handle and depend only on this
interface, so the app picks Cloudflare, Firebase or a runtime stub at boot.
`Functions::send` exposes raw `FunctionRequest`s for such crates;
`FunctionRequest::with_segments` percent-encodes dynamic path segments.
`ApiError: From<FunctionsError>` flattens adapter errors (callable failures keep
their HTTP status).

The HTTP transport uses the workspace's native Reqwest/Tokio setup. It is not a
browser-WASM transport. Native Firebase providers follow the platform coverage of
the existing Auth and App Check modules; an injectable provider alone does not add
native SDK support for a platform. No server deployments or credentials are included.
