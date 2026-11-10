# masamune-whisker-auth-firebase

Firebase Authentication implementation of `masamune-whisker-auth`, modeled after
Flutter Masamune's `katana_auth_firebase`.

```rust,ignore
use masamune_whisker_auth_firebase::firebase_auth;
use masamune_whisker_functions::Functions;
use masamune_whisker_functions_cloudflare::CloudflareFunctionsAdapter;

let functions = Functions::new(
    CloudflareFunctionsAdapter::new("https://edge.example.com").with_auth(firebase_auth()),
);
```

`FirebaseAuthAdapter::access_token` returns the signed-in user's ID token and fails
with `AuthError::NotSignedIn` when nobody is signed in. `current_uid()` returns the
user's UID. Sign-in itself is handled by the native modules
(`masamune-whisker-google-sign-in`, `masamune-whisker-apple-sign-in`, …).
