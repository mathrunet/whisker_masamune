//! Opens URLs in external apps (browser, store), closes the app, styles the window and reports
//! whether the app is in the foreground ([`on_lifecycle`]).
//!
//! On iOS, [`open_url`] can only detect handlers of custom URL schemes that are declared in
//! `LSApplicationQueriesSchemes`; declare them with the [`cng::MasamuneAppControl`] CNG plugin.

pub mod cng;

use masamune_whisker_bridge::NativeModule;
pub use masamune_whisker_bridge::{NativeError, NativeResult, Subscription};
use serde::Deserialize;
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

/// A system settings screen opened by [`open_settings`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SettingsPage {
    /// This app's settings (Android: app details; iOS: the app's page in Settings).
    App,
    /// The top of the system Settings app.
    System,
    /// Developer options. Android only.
    Developer,
    /// Developer options with the Wireless debugging entry highlighted (on devices that support
    /// highlighting). Android only: the Wireless debugging screen itself cannot be opened by other
    /// apps, so this is one tap away from it.
    WirelessDebugging,
}

impl SettingsPage {
    fn as_str(self) -> &'static str {
        match self {
            SettingsPage::App => "app",
            SettingsPage::System => "system",
            SettingsPage::Developer => "developer",
            SettingsPage::WirelessDebugging => "wirelessDebugging",
        }
    }
}

/// Opens a system settings screen. Returns `false` when the screen cannot be opened.
///
/// - Android: [`SettingsPage::Developer`] and [`SettingsPage::WirelessDebugging`] open
///   `ACTION_APPLICATION_DEVELOPMENT_SETTINGS`. When developer options are turned off, the system
///   shows its own "not available" screen.
/// - iOS: apps can only open their own page in Settings, so [`SettingsPage::App`] and
///   [`SettingsPage::System`] both open it, and the Android-only pages return `false`.
pub fn open_settings(page: SettingsPage) -> NativeResult<bool> {
    module().call("openSettings", &json!({ "page": page.as_str() }))
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

/// Whether the app is visible to the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AppLifecycle {
    /// The app is on screen (iOS: active; Android: the process has a started activity).
    Foreground,
    /// The app is not on screen.
    Background,
}

/// The current [`AppLifecycle`]. Errors (and platforms without the native module) read as
/// [`AppLifecycle::Foreground`].
///
/// - iOS: `UIApplication.applicationState`; only `.background` reads as background (`.inactive`,
///   for example during launch or while a system alert is shown, reads as foreground).
/// - Android: `ProcessLifecycleOwner`'s state is at least `STARTED`.
pub fn current_lifecycle() -> AppLifecycle {
    module()
        .call::<_, AppLifecycle>("currentLifecycle", &json!({}))
        .unwrap_or(AppLifecycle::Foreground)
}

/// Receives foreground / background changes while the returned [`Subscription`] is held.
///
/// Only changes are delivered: the state at the time of subscribing is not sent (read it with
/// [`current_lifecycle`]) and the same state is never sent twice in a row.
///
/// - iOS: [`AppLifecycle::Foreground`] on `UIApplication.didBecomeActiveNotification`,
///   [`AppLifecycle::Background`] on `didEnterBackgroundNotification`. Briefly resigning active
///   (Control Center, a system alert) without entering the background sends nothing.
/// - Android: `ProcessLifecycleOwner` `ON_START` / `ON_STOP`. `ON_STOP` is dispatched about 700 ms
///   after the last activity stops, so configuration changes (rotation) send nothing.
pub fn on_lifecycle(handler: impl Fn(AppLifecycle) + 'static) -> Subscription {
    module().subscribe("lifecycle", handler)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_lifecycle_events() {
        assert_eq!(
            serde_json::from_str::<AppLifecycle>(r#""foreground""#).unwrap(),
            AppLifecycle::Foreground
        );
        assert_eq!(
            serde_json::from_str::<AppLifecycle>(r#""background""#).unwrap(),
            AppLifecycle::Background
        );
        assert!(serde_json::from_str::<AppLifecycle>(r#""inactive""#).is_err());
    }

    #[test]
    fn current_lifecycle_defaults_to_foreground_without_native_module() {
        assert_eq!(current_lifecycle(), AppLifecycle::Foreground);
    }
}
