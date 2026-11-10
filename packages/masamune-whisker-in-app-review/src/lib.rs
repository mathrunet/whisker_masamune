//! In-app review requests.

use masamune_whisker_bridge::NativeModule;
pub use masamune_whisker_bridge::{NativeError, NativeResult};
use serde_json::json;

fn module() -> NativeModule {
    NativeModule::new(env!("CARGO_PKG_NAME"), "InAppReview")
}

/// Requests an in-app review. If unavailable, opens `store_url` and returns `false`.
pub async fn request_review(store_url: &str) -> NativeResult<bool> {
    module()
        .call_async("requestReview", &json!({ "storeUrl": store_url }))
        .await
}
