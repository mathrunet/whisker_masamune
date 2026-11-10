//! AdMob (adaptive banners and interstitial ads).
//!
//! Set the App ID in the app manifest on Android (`com.google.android.gms.ads.APPLICATION_ID`, for example
//! with `AndroidMetaData`) and in `Info.plist` on iOS (`GADApplicationIdentifier`, written by the
//! [`cng::MasamuneAdmob`] plugin together with the SKAdNetwork identifiers and the App Tracking
//! Transparency prompt text).
//!
//! The SDK is initialized when the first ad is loaded on both platforms.

pub mod cng;

mod adapter;

pub use adapter::{AdmobMasamuneAdapter, default_banner_ad_unit_id, show_default_interstitial};
use masamune_whisker_bridge::NativeModule;
pub use masamune_whisker_bridge::{NativeError, NativeResult};
use serde::Deserialize;
use serde_json::json;
use whisker::Signal;
use whisker::event::CustomEvent;

fn module() -> NativeModule {
    NativeModule::new(env!("CARGO_PKG_NAME"), "Admob")
}

/// Height (dp) of an anchored adaptive banner sized to the screen width.
///
/// On iOS the width and height are in points and the size is the SDK's large anchored adaptive banner
/// (the replacement of the deprecated current-orientation anchored adaptive size).
pub fn banner_height(width_dp: i32) -> NativeResult<i64> {
    module().call("bannerHeight", &json!({ "widthDp": width_dp }))
}

/// Loads and shows an interstitial ad, returning once it is closed. Returns `true` if shown (`false` on no fill).
///
/// On iOS the ad is presented from the key window's top-most view controller.
pub async fn show_interstitial(ad_unit_id: &str) -> NativeResult<bool> {
    module()
        .call_async("showInterstitial", &json!({ "adUnitId": ad_unit_id }))
        .await
}

/// App Tracking Transparency authorization status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TrackingStatus {
    /// The user has not been asked yet.
    NotDetermined,
    /// Tracking is restricted (for example by parental controls).
    Restricted,
    /// The user declined tracking.
    Denied,
    /// The user allowed tracking (always the case on Android, which has no such prompt).
    Authorized,
}

impl TrackingStatus {
    /// The status as the native string (`"notDetermined"`, `"restricted"`, `"denied"`, `"authorized"`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotDetermined => "notDetermined",
            Self::Restricted => "restricted",
            Self::Denied => "denied",
            Self::Authorized => "authorized",
        }
    }
}

/// Asks for App Tracking Transparency permission (the IDFA used for personalized ads) and returns the status.
///
/// On iOS the system prompt is shown only while the status is undetermined; afterwards the stored answer
/// is returned. A request made while the app is not active waits until it becomes active. Requires
/// `NSUserTrackingUsageDescription` in `Info.plist` (set it with
/// [`cng::MasamuneAdmobConfig::tracking_usage_description`]); without it the call fails instead of crashing.
/// The Flutter version asks right before the SDK is initialized, i.e. before the first ad is loaded.
///
/// On Android there is no such permission, so this returns [`TrackingStatus::Authorized`] without a prompt.
pub async fn request_tracking_authorization() -> NativeResult<TrackingStatus> {
    module()
        .call_async("requestTrackingAuthorization", &json!({}))
        .await
}

/// Banner ad element. Loads at a width of `width_dp` and emits `loaded` or `failed` (`{message}`).
///
/// Failed loads are retried after 5, 10 and 20 seconds. On iOS the banner is attached to the key window's
/// top-most view controller (used by the SDK to present the ad's landing page).
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
