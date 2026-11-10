//! In-app review requests.

use masamune_whisker_bridge::NativeModule;
pub use masamune_whisker_bridge::{NativeError, NativeResult};
use serde_json::json;

fn module() -> NativeModule {
    NativeModule::new(env!("CARGO_PKG_NAME"), "InAppReview")
}

/// Requests an in-app review. If unavailable, opens `store_url` and returns `false`.
///
/// On iOS the request goes to StoreKit (`AppStore.requestReview(in:)`, `SKStoreReviewController` on
/// iOS 15). iOS decides by itself whether the prompt appears (at most three times a year, never in
/// TestFlight builds) and does not tell the app, so `true` only means the review was requested.
/// `store_url` (use the App Store URL on iOS) is opened only when no foreground window scene exists.
pub async fn request_review(store_url: &str) -> NativeResult<bool> {
    module()
        .call_async("requestReview", &json!({ "storeUrl": store_url }))
        .await
}
