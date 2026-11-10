//! Sign in with Google (Firebase Auth `google.com` provider). Like the Flutter version, sign-in runs in a web screen.

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

/// Signs in with Google and makes it the current Firebase Auth user (scopes: email and profile).
pub async fn sign_in() -> NativeResult<GoogleSignInResult> {
    module()
        .call_async("signIn", &json!({ "scopes": ["email", "profile"] }))
        .await
}

/// Links Google to the current user (e.g. anonymous). The uid and data are kept.
pub async fn link() -> NativeResult<GoogleSignInResult> {
    module()
        .call_async("link", &json!({ "scopes": ["email", "profile"] }))
        .await
}
