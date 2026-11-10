//! Sign in with Google (Firebase Auth `google.com` provider). Like the Flutter version, sign-in runs in a web screen.
//!
//! On iOS sign-in uses the Google Sign-In SDK (in an `ASWebAuthenticationSession` sheet) and signs
//! in to Firebase Auth with its tokens; results and the `cancelled` / `already-registered` errors are
//! the same as on Android. The client id comes from `GoogleService-Info.plist` (`CLIENT_ID`), and
//! the [`cng::MasamuneGoogleSignIn`] plugin must register its `REVERSED_CLIENT_ID` URL scheme. The
//! account picker is the SDK's own (Android passes `prompt=select_account`), and the error raised when
//! no view controller can present the screen is still `no activity`.

pub mod cng;

mod adapter;

pub use adapter::{DEFAULT_SCOPES, GoogleSignInMasamuneAdapter};
use masamune_whisker_bridge::NativeModule;
pub use masamune_whisker_bridge::{NativeError, NativeResult};
use serde::Deserialize;
use serde_json::json;

fn module() -> NativeModule {
    NativeModule::new(env!("CARGO_PKG_NAME"), "GoogleSignIn")
}

/// Result of a sign-in.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoogleSignInResult {
    pub uid: Option<String>,
    #[serde(default)]
    pub is_new_user: bool,
}

/// Whether the user closed the screen.
pub fn is_cancelled(error: &NativeError) -> bool {
    error.0 == "cancelled"
}

/// Whether linking failed because the account is already registered to another user.
pub fn is_already_registered(error: &NativeError) -> bool {
    error.0 == "already-registered"
}

/// Signs in with Google and makes it the current Firebase Auth user (scopes:
/// [`GoogleSignInMasamuneAdapter::scopes`], email and profile by default).
pub async fn sign_in() -> NativeResult<GoogleSignInResult> {
    module()
        .call_async("signIn", &json!({ "scopes": adapter::scopes() }))
        .await
}

/// Links Google to the current user (e.g. anonymous). The uid and data are kept.
pub async fn link() -> NativeResult<GoogleSignInResult> {
    module()
        .call_async("link", &json!({ "scopes": adapter::scopes() }))
        .await
}
