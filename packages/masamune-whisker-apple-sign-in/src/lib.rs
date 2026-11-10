//! Apple でサインイン（Firebase Auth の `apple.com` プロバイダー）。

use masamune_whisker_bridge::NativeModule;
pub use masamune_whisker_bridge::{NativeError, NativeResult};
use serde::Deserialize;
use serde_json::json;

fn module() -> NativeModule {
    NativeModule::new(env!("CARGO_PKG_NAME"), "AppleSignIn")
}

/// サインインの結果。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppleSignInResult {
    pub uid: Option<String>,
    #[serde(default)]
    pub is_new_user: bool,
}

/// ユーザーが画面を閉じたか。
pub fn is_cancelled(error: &NativeError) -> bool {
    error.0 == "cancelled"
}

/// 別ユーザーとして登録済みのため紐付けられなかったか。
pub fn is_already_registered(error: &NativeError) -> bool {
    error.0 == "already-registered"
}

/// Apple でサインインし、Firebase Auth の現在のユーザーにする（scopes は email と name）。
pub async fn sign_in() -> NativeResult<AppleSignInResult> {
    module()
        .call_async("signIn", &json!({ "scopes": ["email", "name"] }))
        .await
}

/// 今のユーザー（匿名など）に Apple を紐付ける。uid とデータはそのまま引き継がれる。
pub async fn link() -> NativeResult<AppleSignInResult> {
    module()
        .call_async("link", &json!({ "scopes": ["email", "name"] }))
        .await
}
