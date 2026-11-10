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

/// Screen orientation lock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum OrientationLock {
    /// Follow the sensor and the system setting.
    #[default]
    Unspecified,
    Portrait,
    Landscape,
    /// Portrait, either way up.
    SensorPortrait,
    /// Landscape, either side.
    SensorLandscape,
}

impl OrientationLock {
    fn as_str(self) -> &'static str {
        match self {
            OrientationLock::Unspecified => "unspecified",
            OrientationLock::Portrait => "portrait",
            OrientationLock::Landscape => "landscape",
            OrientationLock::SensorPortrait => "sensorPortrait",
            OrientationLock::SensorLandscape => "sensorLandscape",
        }
    }
}

/// Locks (or unlocks) the screen orientation.
pub fn set_orientation(lock: OrientationLock) -> NativeResult<bool> {
    module().call("setOrientation", &json!({ "lock": lock.as_str() }))
}

/// Dark system bar icons for light backgrounds (`true`) or light icons for dark ones (`false`).
pub fn set_system_bars_style(
    light_status_bar: bool,
    light_navigation_bar: bool,
) -> NativeResult<bool> {
    module().call(
        "setSystemBarsStyle",
        &json!({ "lightStatusBar": light_status_bar, "lightNavigationBar": light_navigation_bar }),
    )
}

/// Background color of the window (`0xAARRGGBB`), visible before the first frame and between
/// screens.
pub fn set_window_background(argb: u32) -> NativeResult<bool> {
    module().call("setWindowBackground", &json!({ "argb": argb }))
}
