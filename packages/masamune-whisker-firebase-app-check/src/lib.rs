//! Firebase App Check。
//!
//! release ビルドは端末の証明（Android は Play Integrity）、debug ビルドは debug provider を使う。

use masamune_whisker_bridge::NativeModule;
pub use masamune_whisker_bridge::{NativeError, NativeResult};
use serde_json::json;

fn module() -> NativeModule {
    NativeModule::new(env!("CARGO_PKG_NAME"), "FirebaseAppCheck")
}

/// App Check を有効にする。debug ビルドで `debug_token` を渡すと、その固定トークンを使う。
pub fn activate(debug_token: Option<&str>) -> NativeResult<bool> {
    module().call(
        "activate",
        &json!({ "debugToken": debug_token.unwrap_or("") }),
    )
}

/// App Check トークン（`X-Firebase-AppCheck` ヘッダーに付ける値）。
pub async fn token(force_refresh: bool) -> NativeResult<String> {
    module()
        .call_async("token", &json!({ "forceRefresh": force_refresh }))
        .await
}
