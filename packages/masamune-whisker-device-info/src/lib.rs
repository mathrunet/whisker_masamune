//! Device language, screen size, time zone, and app version.

use masamune_whisker_bridge::NativeModule;
pub use masamune_whisker_bridge::{NativeError, NativeResult, Subscription};

fn module() -> NativeModule {
    NativeModule::new(env!("CARGO_PKG_NAME"), "DeviceInfo")
}

use serde::Deserialize;

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceLocale {
    pub language: String,
    #[serde(default)]
    pub country: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    #[serde(default)]
    pub locales: Vec<DeviceLocale>,
    #[serde(default)]
    pub timezone: String,
    #[serde(default)]
    pub offset_minutes: i32,
    #[serde(default)]
    pub package_name: Option<String>,
    #[serde(default)]
    pub version_name: Option<String>,
    #[serde(default)]
    pub version_code: Option<i64>,
    #[serde(default)]
    pub sdk_int: i32,
    #[serde(default)]
    pub debuggable: bool,
    #[serde(default)]
    pub width_dp: f64,
    #[serde(default)]
    pub height_dp: f64,
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

/// Device configuration that can change while the app runs (dark mode, languages, orientation,
/// font scale and window size).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceConfiguration {
    #[serde(default)]
    pub night_mode: bool,
    #[serde(default)]
    pub locales: Vec<DeviceLocale>,
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

/// Receives configuration changes (dark mode, languages, orientation, font scale). The current
/// value is delivered right after subscribing. Requires `uiMode|locale` in the activity's
/// `configChanges`, otherwise Android recreates the activity instead.
pub fn on_configuration_changed(handler: impl Fn(DeviceConfiguration) + 'static) -> Subscription {
    module().subscribe("configurationChanged", handler)
}
