//! URL を外部アプリ（ブラウザ・ストア）で開く、アプリを閉じる。

use masamune_whisker_bridge::NativeModule;
pub use masamune_whisker_bridge::{NativeError, NativeResult};
use serde_json::json;

fn module() -> NativeModule {
    NativeModule::new(env!("CARGO_PKG_NAME"), "AppControl")
}

/// URL を外部アプリで開く。対応するアプリが無ければ `false`。
pub fn open_url(url: &str) -> NativeResult<bool> {
    module().call("openUrl", &json!({ "url": url }))
}

/// アプリを閉じる。
pub fn exit_app() -> NativeResult<bool> {
    module().call("exitApp", &json!({}))
}
