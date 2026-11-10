//! Sign in with Apple (Firebase Auth `apple.com` provider).
//!
//! Android uses Firebase Auth's web flow. iOS uses the native Sign in with Apple sheet
//! (AuthenticationServices, with a SHA-256 nonce) and signs in to Firebase Auth with the Apple ID
//! token, so the result and the `cancelled` / `already-registered` errors are the same. On iOS add
//! the [`cng::MasamuneAppleSignIn`] plugin to whisker.rs for the entitlement; the error raised when
//! no window can present the sheet is still `no activity`.

pub mod cng;

mod adapter;

pub use adapter::{AppleSignInMasamuneAdapter, DEFAULT_SCOPES};
use masamune_whisker_bridge::NativeModule;
pub use masamune_whisker_bridge::{NativeError, NativeResult};
use serde::Deserialize;
use serde_json::json;

fn module() -> NativeModule {
    NativeModule::new(env!("CARGO_PKG_NAME"), "AppleSignIn")
}

/// Result of a sign-in.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppleSignInResult {
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

/// Signs in with Apple and makes it the current Firebase Auth user (scopes:
/// [`AppleSignInMasamuneAdapter::scopes`], email and name by default).
pub async fn sign_in() -> NativeResult<AppleSignInResult> {
    module()
        .call_async("signIn", &json!({ "scopes": adapter::scopes() }))
        .await
}

/// Links Apple to the current user (e.g. anonymous). The uid and data are kept.
pub async fn link() -> NativeResult<AppleSignInResult> {
    module()
        .call_async("link", &json!({ "scopes": adapter::scopes() }))
        .await
}
