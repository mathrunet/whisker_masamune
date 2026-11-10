# masamune-whisker-auth0-firebase

Signs in to Firebase Authentication with an Auth0 session: the Auth0 access token is exchanged
for a Firebase custom token by the `auth0_custom_token` callable, so the Firebase UID is the
Auth0 `sub`. Ported from the Flutter `RoyaltyBankAuthAdapter` and `Auth0FirebaseAuthAdapter`.

The crate does not choose a Functions backend: the app passes a `Functions` handle (for example
`FirebaseFunctionsAdapter` from masamune-whisker-functions-firebase, optionally wrapped in
`Auth0FunctionsAdapter`).

## Flow

| Function | Behavior |
|---|---|
| `login(&auth0, &functions, on_auth0_signin)` | Auth0 login with `prompt=login`. `Ok(None)` when cancelled. Then runs `on_auth0_signin(&credentials)` (for example registering the user with the backend) while signing out of Firebase and signing in with the exchanged token. If either fails, the Auth0 credentials are cleared, Firebase is signed out and the error is returned. |
| `restore(&auth0, &functions)` | Restores Auth0. No session: signs out of Firebase, `Ok(None)`. Firebase UID equal to `sub`: keeps it. Otherwise signs out of Firebase and exchanges again. |
| `logout(&auth0)` | Signs out of Firebase and Auth0. |
| `sign_in_firebase(&functions, &credentials, domain)` | The exchange alone; returns the UID. |
| `sign_out_firebase()` | Signs out of Firebase when signed in. |

The free functions use the native Firebase SDK (whisker-firebase). `Auth0Firebase::new(session)`
runs the same flow over any `FirebaseSession` (tests use a fake).

```rust,ignore
use masamune_whisker_auth0::Auth0;
use masamune_whisker_auth0_firebase as auth0_firebase;

// Boot task
let auth0 = Auth0::primary().expect("installed");
let signed_in = auth0_firebase::restore(&auth0, &app.functions).await?.is_some();

// Login button
let signed_in = auth0_firebase::login(&auth0, &app.functions, |credentials| {
    let api = app.api.clone();
    let sub = credentials.user.sub.clone();
    async move { api.on_auth0_signin(&sub).await }
})
.await?;
```

`Auth0CustomTokenAction` sends `{"auth0_access_token", "auth0_domain"}` and reads the callable
result `{"token": "..."}` (a bare string is accepted too); an empty token is an error.

`Auth0FirebaseAuthAdapter` / `auth0_firebase_auth()` provide the Firebase ID token of the
signed-in user (`masamune_whisker_auth::AuthAdapter`) for Firebase-authenticated requests.

Differences from the Flutter version: there is no fallback to anonymous Firebase sign-in when the
exchange fails, and the legacy SharedPreferences token migration is not ported.
