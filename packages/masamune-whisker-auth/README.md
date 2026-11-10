# masamune-whisker-auth

Authentication interface for Whisker apps, modeled after Flutter Masamune's
`katana_auth`. Crates that send the user's token (`masamune-whisker-functions`
adapters and everything built on them) depend only on this crate; the app picks
the implementation at boot.

```rust
use masamune_whisker_auth::{Auth, RuntimeAuthAdapter};

// Tests or fixed server tokens.
let auth = Auth::fixed("example-token");
// Development: token plus user ID.
let auth = Auth::new(RuntimeAuthAdapter::default().with_token("t").with_user_id("u"));
// A custom token source, called on every request.
let auth = Auth::from_fn(|_force_refresh| async { Ok(Some("token".to_owned())) });
```

Implement `AuthAdapter` for other backends. `masamune-whisker-auth-firebase`
provides `FirebaseAuthAdapter` (Firebase ID tokens of the signed-in user).

`access_token` returning `Ok(None)` means "send no Authorization header".
Futures are not `Send`; they run on the Whisker UI thread.
