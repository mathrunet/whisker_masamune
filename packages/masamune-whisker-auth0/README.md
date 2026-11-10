# masamune-whisker-auth0

Auth0 Universal Login for Whisker apps, with credentials kept in the iOS Keychain / Android
Keystore and renewed with the refresh token. Ported from the Flutter `RoyaltyBankAuthAdapter`
(auth0_flutter).

- `Auth0Adapter`: login (with an optional `prompt`), logout, stored credentials (renewed when
  they expire within a TTL), validity check and clear.
- `NativeAuth0Adapter`: Auth0.swift 2.x (`ASWebAuthenticationSession`, `CredentialsManager`) and
  Auth0.Android 3.x (Custom Tabs, `SecureCredentialsManager`).
- `RuntimeAuth0Adapter`: in-memory adapter for tests and development.
- `Auth0`: clonable session handle (`login`, `restore`, `access_token`, `logout`, `clear`,
  `user_id` = Auth0 `sub`). It implements `masamune_whisker_auth::AuthAdapter`, so it can be
  passed wherever an `Auth` is expected (`Authorization: Bearer <Auth0 access token>`).
- `Auth0MasamuneAdapter`: registers the session in `MasamuneAdapters`; read it with
  `Auth0::primary()`. `auth0_auth()` is an `Auth` that follows the installed session.
- `Auth0FunctionsAdapter` (feature `functions`, on by default): wraps any `FunctionsAdapter` and
  adds `auth0_domain` and `auth0_access_token` to every JSON object body (keys the request sets
  win; no token while signed out).
- `cng::MasamuneAuth0`: CNG plugin for the callback URL scheme.

Signing in to Firebase with the Auth0 session is in `masamune-whisker-auth0-firebase`.

## Auth0 application

Create a Native application and add both callback URLs to **Allowed Callback URLs** and
**Allowed Logout URLs**:

```text
{scheme}://{domain}/ios/{bundle id}/callback
{scheme}://{domain}/android/{application id}/callback
```

`scheme` must be lowercase (Auth0.Android requirement); the bundle / application id is the usual
choice. Request `offline_access` (the default scopes are `openid profile email offline_access`)
to get refresh tokens.

## whisker.rs

```rust,ignore
use masamune_whisker_auth0::cng::MasamuneAuth0;

app.project_plugin::<MasamuneAuth0>(|c| {
    c.scheme("com.example.app").domain("example.jp.auth0.com");
});
```

- iOS: adds the scheme to `CFBundleURLTypes`.
- Android: sets the `auth0Scheme` and `auth0Domain` manifest placeholders in the application
  module's `defaultConfig` through the Gradle IR. Auth0.Android's own manifest declares
  `RedirectActivity` with the intent filter
  `${auth0Scheme}://${auth0Domain}/android/${applicationId}/callback`, so the plugin does not
  write a manifest overlay (and cannot collide with the overlays of other plugins such as
  masamune-whisker-screen). A different value set for the same placeholder by another plugin is
  reported as a merge conflict.

## Preparing the app

```rust,ignore
use masamune_whisker_adapter::MasamuneAdapters;
use masamune_whisker_auth0::{Auth0Config, Auth0MasamuneAdapter};

MasamuneAdapters::new()
    .adapter(Auth0MasamuneAdapter::native(Auth0Config::new(
        config::AUTH0_DOMAIN,
        config::AUTH0_CLIENT_ID,
        "com.example.app",
    )))
    .install();
```

Restore the session in the boot task and log in from a button:

```rust,ignore
let auth0 = Auth0::primary().expect("Auth0MasamuneAdapter is installed");
match auth0.restore().await {
    Ok(Some(_)) => "/home",
    Ok(None) => "/login",
    Err(error) => { /* report, the session is cleared */ "/login" }
};

match auth0.login(Some("login")).await {
    Ok(credentials) => { /* signed in as credentials.user.sub */ }
    Err(Auth0Error::Cancelled) => {}
    Err(error) => { /* report */ }
}
```

`restore` renews a token that expires within a minute; when renewal fails it clears the stored
credentials and returns the error. `access_token(false)` returns the cached token until it
expires, then renews it; `access_token(true)` always asks the credentials manager (which renews
only when the token expires within a minute). `logout` clears the browser session and the stored
credentials (the local session is cleared even when the browser step fails).

Native rejections map to `Auth0Error::Cancelled` (`cancelled`), `Auth0Error::NoCredentials`
(`no-credentials`) or `Auth0Error::Native { code, message }`. Credentials print without their
tokens in `Debug`; never log the tokens.

## Tests

```rust,ignore
let adapter = RuntimeAuth0Adapter::new().with_user(Auth0User { sub: "auth0|u".into(), ..Default::default() });
let auth0 = Auth0::new(Auth0Config::new("example.jp.auth0.com", "client", "com.example.app"), adapter.clone());
auth0.login(None).await?;
assert_eq!(adapter.login_prompts(), vec![None]);
```

`with_stored`, `with_login_error` (for example `Auth0Error::Cancelled`) and `with_renew_error`
set up restores and failures.
