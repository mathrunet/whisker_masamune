//! Opens URLs in external apps (browser, store) and closes the app.
//!
//! On iOS, [`open_url`] can only detect handlers of custom URL schemes that are declared in
//! `LSApplicationQueriesSchemes`; declare them with the [`cng::MasamuneAppControl`] CNG plugin.

pub mod cng;

use masamune_whisker_bridge::NativeModule;
pub use masamune_whisker_bridge::{NativeError, NativeResult};
use serde_json::json;

fn module() -> NativeModule {
    NativeModule::new(env!("CARGO_PKG_NAME"), "AppControl")
}

/// Opens a URL in an external app. Returns `false` if no app can handle it.
///
/// On iOS, `http` / `https` URLs always open (in the default browser or the app that claims the
/// universal link). Other schemes open only when `UIApplication.canOpenURL` confirms a handler,
/// which requires the scheme to be listed in `LSApplicationQueriesSchemes` (see
/// [`cng::MasamuneAppControl`]); otherwise `false` is returned and nothing is opened.
pub fn open_url(url: &str) -> NativeResult<bool> {
    module().call("openUrl", &json!({ "url": url }))
}

/// Closes the app.
///
/// On iOS this does nothing and returns `false`: Apple does not allow apps to quit themselves
/// programmatically (App Store Review Guidelines), so offer another way forward instead.
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
///
/// On iOS the lock is applied to the host's root view controller (`supportedInterfaceOrientations`)
/// and, on iOS 16 and later, the window rotates right away (`UIWindowScene.requestGeometryUpdate`);
/// on iOS 15 it takes effect at the next device rotation. `Landscape` maps to landscape-right (the
/// device turned counter-clockwise, like Android) and `SensorPortrait` includes upside-down portrait
/// where the device supports it. Orientations missing from the app's `UISupportedInterfaceOrientations`
/// cannot be selected, and iPad apps that support multitasking ignore orientation locks.
pub fn set_orientation(lock: OrientationLock) -> NativeResult<bool> {
    module().call("setOrientation", &json!({ "lock": lock.as_str() }))
}

/// Dark system bar icons for light backgrounds (`true`) or light icons for dark ones (`false`).
///
/// On iOS this is best effort: the status bar style belongs to the host's view controller, so the
/// root view controller's `preferredStatusBarStyle` is overridden (dark content for
/// `light_status_bar`), which has no effect when the app sets
/// `UIViewControllerBasedStatusBarAppearance` to `NO` or while a presented screen chooses its own
/// style. iOS has no navigation bar, so `light_navigation_bar` is ignored.
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
///
/// On iOS this sets the key window's and the root view's background color. The launch screen is a
/// storyboard, so the color shown before the first frame is the build-time background instead.
pub fn set_window_background(argb: u32) -> NativeResult<bool> {
    module().call("setWindowBackground", &json!({ "argb": argb }))
}
