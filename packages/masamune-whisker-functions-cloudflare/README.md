# masamune-whisker-functions-cloudflare

Cloudflare Workers adapter for `masamune-whisker-functions`, modeled after
`masamune_functions_cloudflare/lib/adapter/cloudflare_functions_adapter.dart`.

```rust
use masamune_whisker_functions::{Auth, Functions};
use masamune_whisker_functions_cloudflare::CloudflareFunctionsAdapter;

let functions = Functions::new(
    CloudflareFunctionsAdapter::new("https://edge.example.com")
        .with_region_endpoint("https://region.example.com")
        .with_auth(Auth::from_fn(|_force_refresh| async {
            // Fetch the current user's ID/access token from the app's auth service.
            Ok(Some("example-token".to_owned()))
        }))
);
```

Use `request.worker_type = Some("region".into())` for region operations. Absent or
`edge` selects the default endpoint. Missing region configuration and unsupported
worker types fail before acquiring tokens or sending HTTP. Configure both endpoints
from the app's existing flavor constants at boot (`option_env!("<APP>_FLAVOR")`);
the package does not choose production implicitly.

For Firebase ID tokens, pass `masamune_whisker_auth_firebase::firebase_auth()`
(or any other `AuthAdapter`) to `.with_auth`. This crate depends only on the
`masamune-whisker-auth` interface, never on an auth implementation. The backend
must verify the token. Authentication is opt-in;
without a provider the adapter sends no managed Authorization header.

Method defaults to POST. Set GET/PUT/PATCH/DELETE explicitly as needed. Action
`path`, encoded query parameters, custom headers, JSON body and timeout are honored.
A configured provider owns bearer authentication; action headers cannot replace its
token. `with_auth`, `with_timeout` and `with_region_endpoint` can be called in any
order. No automatic retries occur.

See the core package README for typed actions, runtime stubs, error handling,
platform limits and application startup integration. No Cloudflare API key is
required to call an application's Worker endpoint.
