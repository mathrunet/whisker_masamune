//! アプリ内レビューの依頼。

use masamune_whisker_bridge::NativeModule;
pub use masamune_whisker_bridge::{NativeError, NativeResult};
use serde_json::json;

fn module() -> NativeModule {
    NativeModule::new(env!("CARGO_PKG_NAME"), "InAppReview")
}

/// アプリ内レビューを依頼する。使えなければ `store_url` を開いて `false` を返す。
pub async fn request_review(store_url: &str) -> NativeResult<bool> {
    module()
        .call_async("requestReview", &json!({ "storeUrl": store_url }))
        .await
}
