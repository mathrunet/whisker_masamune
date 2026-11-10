# masamune-whisker-functions-firebase

Firebase HTTPS callable adapter for `masamune-whisker-functions`, modeled after
`katana_functions_firebase/lib/adapter/firebase_functions_adapter.dart`.
It reuses `masamune-whisker-workers::CallableClient` and needs no native SDK when
custom token providers are used.

```rust
use masamune_whisker_functions::{AuthProvider, Functions};
use masamune_whisker_functions_firebase::FirebaseFunctionsAdapter;

let functions = Functions::new(
    FirebaseFunctionsAdapter::new("project-id", "asia-northeast1")
        .with_auth(AuthProvider::new(|_force_refresh| async {
            Ok(Some("example-id-token".to_owned()))
        }))
        .with_app_check(AuthProvider::new(|_force_refresh| async {
            Ok(Some("example-app-check-token".to_owned()))
        }))
);
```

Enable the `firebase` Cargo feature for `.with_firebase_providers()`, which uses
the current Firebase Auth user and native App Check. Activate App Check in the
app's boot task first. That setup currently follows the existing modules' native
platform support (App Check has Android support). Limited-use App Check tokens
are not implemented. Never embed service-account credentials in a client app.

Constructors default to unauthenticated calls without App Check; select providers
explicitly. `with_app_check` requires a nonempty token: provider failure prevents
dispatch. The existing feature-gated `workers::Callable` also uses this transport
and now propagates App Check acquisition failure instead of logging and proceeding.

For a local emulator use:

```rust
use masamune_whisker_functions_firebase::FirebaseFunctionsAdapter;
let emulator = FirebaseFunctionsAdapter::at_endpoint(
    "http://127.0.0.1:5001/project-id/asia-northeast1"
);
```

Select project, region and emulator URL from the application's dev/prod configuration
at boot. Explicit base URLs also support deployed callable URLs by splitting the
base endpoint and final function name. Use a host address reachable from the device
when connecting to a local emulator.

Calls POST `{"data": <action body>}` to `<base>/<action name>`. An absent body
becomes `{}`; explicit JSON null remains null. Responses accept `data` or `result`,
including null. Missing result keys, invalid JSON, and malformed error envelopes
fail with a decode error. Structured callable errors preserve HTTP status plus
`status`, `message` and `details`, including on HTTP 4xx/5xx. Other non-2xx responses
preserve HTTP status and body.

Use `.with_firestore_database_id("named")` to inject `firestoreDatabaseId` into
object input; configured values override action input. Scalar/array/null input is
rejected when this setting is present. Authentication and App Check headers are
owned by providers, so action headers cannot inject those credentials. Additional
headers are intended for compatible gateways: Firebase's callable endpoint may
reject headers outside its protocol.

REST method/path/query overrides are rejected instead of silently dropping input.
`worker_type` is ignored. Default timeout is 60 seconds, overridden by the action,
and includes both token providers and HTTP. There are no automatic retries.
See the core README for typed actions and test stubs.
