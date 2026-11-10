//! AdMob (adaptive banners and interstitial ads).
//!
//! Set the App ID in the app manifest (`com.google.android.gms.ads.APPLICATION_ID`).

use masamune_whisker_bridge::NativeModule;
pub use masamune_whisker_bridge::{NativeError, NativeResult};
use serde_json::json;
use whisker::Signal;
use whisker::event::CustomEvent;

fn module() -> NativeModule {
    NativeModule::new(env!("CARGO_PKG_NAME"), "Admob")
}

/// Height (dp) of an anchored adaptive banner sized to the screen width.
pub fn banner_height(width_dp: i32) -> NativeResult<i64> {
    module().call("bannerHeight", &json!({ "widthDp": width_dp }))
}

/// Loads and shows an interstitial ad, returning once it is closed. Returns `true` if shown (`false` on no fill).
pub async fn show_interstitial(ad_unit_id: &str) -> NativeResult<bool> {
    module()
        .call_async("showInterstitial", &json!({ "adUnitId": ad_unit_id }))
        .await
}

/// Banner ad element. Loads at a width of `width_dp` and emits `loaded` or `failed` (`{message}`).
#[doc(hidden)]
// whisker 0.16's module_element passes attributes as strings, so the width is passed as a string too.
#[whisker::module_element(name = "masamune-whisker-admob:Banner", measurement = None)]
pub fn ad_banner(
    ad_unit_id: Signal<String>,
    width_dp: Signal<String>,
    style: Style,
    on_loaded: CustomEvent,
    on_failed: CustomEvent,
) {
}

/// Element schema referenced by the generated Host bootstrap code (Web, desktop).
#[doc(hidden)]
pub fn __whisker_element_module_definition() -> whisker::ElementModuleDefinition {
    whisker::ElementModuleDefinition::new(
        env!("CARGO_PKG_NAME"),
        [ad_banner_schema::element_provider()],
    )
}
