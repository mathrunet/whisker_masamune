//! Device screen size, time zone, app version and appearance settings. System languages are in
//! masamune-whisker-localize.

use masamune_whisker_bridge::NativeModule;
pub use masamune_whisker_bridge::{NativeError, NativeResult, Subscription};

fn module() -> NativeModule {
    NativeModule::new(env!("CARGO_PKG_NAME"), "DeviceInfo")
}

use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    #[serde(default)]
    pub timezone: String,
    #[serde(default)]
    pub offset_minutes: i32,
    /// Android package name; the bundle identifier on iOS.
    #[serde(default)]
    pub package_name: Option<String>,
    /// `versionName`; `CFBundleShortVersionString` on iOS.
    #[serde(default)]
    pub version_name: Option<String>,
    /// `versionCode`; `CFBundleVersion` on iOS (`None` when it is not a plain integer such as `"1.2"`).
    #[serde(default)]
    pub version_code: Option<i64>,
    /// Android API level (`Build.VERSION.SDK_INT`). Android only: always `0` on iOS.
    #[serde(default)]
    pub sdk_int: i32,
    /// The app is a debuggable build (`FLAG_DEBUGGABLE`); on iOS, whether the module was compiled
    /// with the `DEBUG` condition (debug configuration).
    #[serde(default)]
    pub debuggable: bool,
    /// Window width in dp (points on iOS).
    #[serde(default)]
    pub width_dp: f64,
    /// Window height in dp (points on iOS).
    #[serde(default)]
    pub height_dp: f64,
    /// Pixels per dp (the screen scale on iOS).
    #[serde(default)]
    pub density: f64,
    /// System dark mode is on.
    #[serde(default)]
    pub night_mode: bool,
    /// `"portrait"` or `"landscape"`.
    #[serde(default)]
    pub orientation: String,
    /// System font scale (1.0 = default).
    #[serde(default)]
    pub font_scale: f64,
}

pub fn info() -> NativeResult<DeviceInfo> {
    module().call("info", &serde_json::json!({}))
}

/// Size of the screen (the app window).
#[derive(Debug, Clone, Copy, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenMetrics {
    #[serde(default)]
    pub width_dp: f64,
    #[serde(default)]
    pub height_dp: f64,
    #[serde(default)]
    pub density: f64,
}

/// Receives screen size changes (rotation, multi-window). The current value is also delivered right after subscribing.
pub fn on_metrics_changed(handler: impl Fn(ScreenMetrics) + 'static) -> Subscription {
    module().subscribe("metricsChanged", handler)
}

/// Device configuration that can change while the app runs (dark mode, orientation, font scale
/// and window size).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceConfiguration {
    #[serde(default)]
    pub night_mode: bool,
    /// `"portrait"` or `"landscape"`.
    #[serde(default)]
    pub orientation: String,
    #[serde(default)]
    pub font_scale: f64,
    #[serde(default)]
    pub width_dp: f64,
    #[serde(default)]
    pub height_dp: f64,
    #[serde(default)]
    pub density: f64,
}

/// Receives configuration changes (dark mode, orientation, font scale). The current value is
/// delivered right after subscribing.
///
/// On Android this requires `uiMode` in the activity's `configChanges`, otherwise Android
/// recreates the activity instead. On iOS it works without any setup (window size, trait and
/// Dynamic Type changes are observed directly).
pub fn on_configuration_changed(handler: impl Fn(DeviceConfiguration) + 'static) -> Subscription {
    module().subscribe("configurationChanged", handler)
}
