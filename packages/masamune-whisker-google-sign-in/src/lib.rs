//! Google でサインイン（Firebase Auth の `google.com` プロバイダー）。Flutter 版と同じく Web 画面でサインインする。

use masamune_whisker_bridge::NativeModule;
pub use masamune_whisker_bridge::{NativeError, NativeResult};
use serde::Deserialize;
use serde_json::json;

fn module() -> NativeModule {
    NativeModule::new(env!("CARGO_PKG_NAME"), "GoogleSignIn")
}

/// サインインの結果。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoogleSignInResult {
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

/// Google でサインインし、Firebase Auth の現在のユーザーにする（scopes は email と profile）。
pub async fn sign_in() -> NativeResult<GoogleSignInResult> {
    module()
        .call_async("signIn", &json!({ "scopes": ["email", "profile"] }))
        .await
}

/// 今のユーザー（匿名など）に Google を紐付ける。uid とデータはそのまま引き継がれる。
pub async fn link() -> NativeResult<GoogleSignInResult> {
    module()
        .call_async("link", &json!({ "scopes": ["email", "profile"] }))
        .await
}
