//! 端末の言語・画面サイズ・タイムゾーン・アプリのバージョン。

use masamune_whisker_bridge::NativeModule;
pub use masamune_whisker_bridge::{NativeError, NativeResult, Subscription};

fn module() -> NativeModule {
    NativeModule::new(env!("CARGO_PKG_NAME"), "DeviceInfo")
}

use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize)]
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
}

pub fn info() -> NativeResult<DeviceInfo> {
    module().call("info", &serde_json::json!({}))
}

/// 画面（アプリのウィンドウ）の大きさ。
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

/// 画面の大きさの変化（回転・マルチウィンドウ）を受け取る。購読直後にも現在値が届く。
pub fn on_metrics_changed(handler: impl Fn(ScreenMetrics) + 'static) -> Subscription {
    module().subscribe("metricsChanged", handler)
}
