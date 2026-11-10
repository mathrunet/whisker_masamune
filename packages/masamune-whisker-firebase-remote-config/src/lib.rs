//! Firebase Remote Config。値はすべて文字列で返す（数値・真偽値は呼び出し側で変換する）。

use std::collections::BTreeMap;
use std::time::Duration;

use masamune_whisker_bridge::NativeModule;
pub use masamune_whisker_bridge::{NativeError, NativeResult};
use serde_json::json;

fn module() -> NativeModule {
    NativeModule::new(env!("CARGO_PKG_NAME"), "FirebaseRemoteConfig")
}

/// 取得して有効化し、全キーの値を返す。取得に失敗した場合はキャッシュ済みか既定の値を返す。
pub async fn fetch_and_activate(
    fetch_timeout: Duration,
    minimum_fetch_interval: Duration,
) -> NativeResult<BTreeMap<String, String>> {
    module()
        .call_async(
            "fetchAndActivate",
            &json!({
                "fetchTimeoutSeconds": fetch_timeout.as_secs(),
                "minimumFetchIntervalSeconds": minimum_fetch_interval.as_secs(),
            }),
        )
        .await
}

/// 有効になっている値（取得はしない）。
pub fn values() -> NativeResult<BTreeMap<String, String>> {
    module().call("values", &json!({}))
}
