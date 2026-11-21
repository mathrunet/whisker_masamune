#![doc = include_str!("../README.md")]

pub mod cng;

use std::cell::Cell;
use std::time::Duration;

use masamune_whisker_bridge::NativeModule;
pub use masamune_whisker_bridge::{NativeError, NativeResult, Subscription};
use serde::{Deserialize, Serialize};
use serde_json::json;

fn module() -> NativeModule {
    NativeModule::new(env!("CARGO_PKG_NAME"), "Location")
}

/// Location permission of the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionStatus {
    /// Not asked yet: [`request_permission`] shows the system prompt.
    NotDetermined,
    /// Refused. Only the system settings can change it ([`open_app_settings`]).
    Denied,
    /// Blocked by parental controls or a device policy (iOS).
    Restricted,
    /// Allowed while the app is in use (or in the background through a foreground service on
    /// Android).
    WhenInUse,
    /// Allowed at any time (iOS "Always", Android `ACCESS_BACKGROUND_LOCATION`).
    Always,
}

impl PermissionStatus {
    /// Whether positions can be read while the app is in use.
    pub fn is_granted(self) -> bool {
        matches!(self, Self::WhenInUse | Self::Always)
    }

    fn decode(value: &str) -> Self {
        match value {
            "notDetermined" => Self::NotDetermined,
            "restricted" => Self::Restricted,
            "whenInUse" => Self::WhenInUse,
            "always" => Self::Always,
            _ => Self::Denied,
        }
    }
}

/// The current permission. Errors (and platforms without the module) read as
/// [`PermissionStatus::Denied`].
pub fn permission_status() -> PermissionStatus {
    module()
        .call::<_, String>("status", &json!({}))
        .map_or(PermissionStatus::Denied, |status| {
            PermissionStatus::decode(&status)
        })
}

/// Asks for permission while the app is in use (iOS `requestWhenInUseAuthorization`, Android
/// `ACCESS_FINE_LOCATION` + `ACCESS_COARSE_LOCATION`) and returns the resulting status.
///
/// Returns at once when the permission was already decided; the system shows its prompt only
/// once.
pub async fn request_permission() -> NativeResult<PermissionStatus> {
    request("requestPermission").await
}

/// Asks for permission at any time (iOS `requestAlwaysAuthorization`, Android
/// `ACCESS_BACKGROUND_LOCATION`). Ask after [`request_permission`] was granted.
///
/// On Android 11 and later the system opens the app's location settings instead of a dialog,
/// and the permission has to be declared with
/// [`cng::MasamuneLocationConfig::background_permission`]. On iOS the user may keep "While
/// Using"; the call then returns [`PermissionStatus::WhenInUse`].
pub async fn request_background_permission() -> NativeResult<PermissionStatus> {
    request("requestBackgroundPermission").await
}

async fn request(method: &str) -> NativeResult<PermissionStatus> {
    let status: String = module().call_async(method, &json!({})).await?;
    Ok(PermissionStatus::decode(&status))
}

/// Whether precise locations are allowed (iOS `accuracyAuthorization == .fullAccuracy`, Android
/// `ACCESS_FINE_LOCATION`). `false` means only approximate locations (about 1-3 km).
pub fn precise() -> bool {
    module()
        .call::<_, bool>("precise", &json!({}))
        .unwrap_or(false)
}

/// Whether location services are switched on for the device.
pub fn service_enabled() -> bool {
    module()
        .call::<_, bool>("serviceEnabled", &json!({}))
        .unwrap_or(false)
}

/// Opens this app's page in the system settings (to change a denied permission).
pub fn open_app_settings() -> NativeResult<bool> {
    module().call("openAppSettings", &json!({}))
}

/// Opens the device location settings (Android `ACTION_LOCATION_SOURCE_SETTINGS`; iOS cannot
/// open them and opens the app's settings).
pub fn open_location_settings() -> NativeResult<bool> {
    module().call("openLocationSettings", &json!({}))
}

/// Requested accuracy. Higher accuracy uses more power.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Accuracy {
    /// About 3 km (iOS `kCLLocationAccuracyThreeKilometers`, Android network provider).
    Lowest,
    /// About 1 km.
    Low,
    /// About 100 m.
    Medium,
    /// About 10 m (iOS `kCLLocationAccuracyNearestTenMeters`, Android GPS or fused).
    #[default]
    High,
    /// The best the device can do (iOS `kCLLocationAccuracyBest`).
    Best,
}

/// A position fix.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Position {
    pub latitude: f64,
    pub longitude: f64,
    /// Horizontal accuracy radius in meters.
    #[serde(default)]
    pub accuracy: f64,
    /// Altitude in meters above sea level (0 when unknown).
    #[serde(default)]
    pub altitude: f64,
    /// Speed in meters per second (negative or 0 when unknown).
    #[serde(default)]
    pub speed: f64,
    /// Course in degrees from north (negative when unknown on iOS).
    #[serde(default)]
    pub heading: f64,
    /// Time of the fix in UTC milliseconds.
    #[serde(default)]
    pub timestamp_ms: i64,
}

/// Reads one fresh position. Fails with a [`NativeError`] when the permission is missing, location
/// services are off, or no fix arrives within `timeout`.
pub async fn current_position(accuracy: Accuracy, timeout: Duration) -> NativeResult<Position> {
    module()
        .call_async(
            "currentPosition",
            &json!({ "accuracy": accuracy, "timeoutMs": duration_ms(timeout) }),
        )
        .await
}

/// The most recent position the device already knows, without starting location updates.
/// `None` when there is none (or the permission is missing).
pub async fn last_known_position() -> NativeResult<Option<Position>> {
    module().call_async("lastKnownPosition", &json!({})).await
}

/// The Android notification of the location foreground service used for background updates.
///
/// When `reuse_notification_id` names a notification the app already shows (for example the
/// media notification of `masamune-whisker-audio-session`, whose id is
/// `masamune_whisker_audio_session::ANDROID_NOTIFICATION_ID`), the service shows that notification
/// instead of a second one and leaves it in place when it stops. Otherwise it shows a notification
/// with `title` / `text` on `channel_id`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AndroidLocationNotification {
    pub reuse_notification_id: Option<i32>,
    pub channel_id: String,
    pub channel_name: String,
    pub title: String,
    pub text: String,
    /// Drawable or mipmap resource name (`"mipmap/ic_notification"`); empty uses the app icon.
    pub small_icon: String,
}

/// Settings of [`watch_position`].
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocationSettings {
    pub accuracy: Accuracy,
    /// Minimum movement in meters before a new position is reported (0 reports every fix).
    pub distance_filter: f64,
    /// Desired interval between fixes on Android (iOS reports as positions change).
    pub interval_ms: u64,
    /// Keep receiving positions while the app is in the background.
    ///
    /// - iOS: sets `allowsBackgroundLocationUpdates` (requires `location` in `UIBackgroundModes`;
    ///   see [`cng`]) and shows the blue location indicator.
    /// - Android: starts a `location` foreground service with [`Self::android_notification`].
    ///   Start watching while the app is in the foreground; Android 12 and later refuse to start
    ///   the service from the background.
    pub background: bool,
    pub android_notification: AndroidLocationNotification,
}

impl Default for LocationSettings {
    fn default() -> Self {
        Self {
            accuracy: Accuracy::High,
            distance_filter: 0.0,
            interval_ms: 10_000,
            background: false,
            android_notification: AndroidLocationNotification::default(),
        }
    }
}

/// Why location updates failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LocationErrorCode {
    PermissionDenied,
    ServiceDisabled,
    /// Background updates could not start (Android: the app was in the background or the
    /// notification could not be shown; iOS: `location` is missing from `UIBackgroundModes`).
    BackgroundUnavailable,
    Failed,
}

/// An error reported by [`watch_position`]. Updates stop after an error.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LocationError {
    pub code: LocationErrorCode,
    #[serde(default)]
    pub message: String,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum Update {
    Position(Position),
    Error(LocationError),
}

thread_local! {
    static WATCHERS: Cell<usize> = const { Cell::new(0) };
}

/// Stops the native updates when the last watcher goes away.
struct WatchGuard;

impl Drop for WatchGuard {
    fn drop(&mut self) {
        let left = WATCHERS.with(|count| {
            let left = count.get().saturating_sub(1);
            count.set(left);
            left
        });
        if left == 0
            && let Err(error) = module().call::<_, bool>("stopUpdates", &json!({}))
        {
            log::debug!("location.stop_failed: {error}");
        }
    }
}

/// Starts location updates and calls `handler` with each position or error until the returned
/// [`Subscription`] is dropped.
///
/// The device runs one stream: starting another watcher applies its `settings` to every
/// watcher, and the stream stops when the last subscription is dropped.
pub fn watch_position(
    settings: &LocationSettings,
    handler: impl Fn(Result<Position, LocationError>) + 'static,
) -> NativeResult<Subscription> {
    let events = module().subscribe("update", move |update: Update| match update {
        Update::Position(position) => handler(Ok(position)),
        Update::Error(error) => handler(Err(error)),
    });
    let distance_filter = if settings.distance_filter.is_finite() {
        settings.distance_filter.max(0.0)
    } else {
        0.0
    };
    module().call::<_, bool>(
        "startUpdates",
        &LocationSettings {
            distance_filter,
            ..settings.clone()
        },
    )?;
    WATCHERS.with(|count| count.set(count.get() + 1));
    Ok(Subscription::new((events, WatchGuard)))
}

fn duration_ms(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

/// Distance in meters between two coordinates (haversine, mean earth radius).
pub fn distance_between(from: (f64, f64), to: (f64, f64)) -> f64 {
    const EARTH_RADIUS_M: f64 = 6_371_008.8;
    let (lat1, lon1) = (from.0.to_radians(), from.1.to_radians());
    let (lat2, lon2) = (to.0.to_radians(), to.1.to_radians());
    let a = ((lat2 - lat1) / 2.0).sin().powi(2)
        + lat1.cos() * lat2.cos() * ((lon2 - lon1) / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_M * a.sqrt().min(1.0).asin()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_statuses() {
        assert_eq!(
            PermissionStatus::decode("whenInUse"),
            PermissionStatus::WhenInUse
        );
        assert_eq!(PermissionStatus::decode("always"), PermissionStatus::Always);
        assert_eq!(
            PermissionStatus::decode("notDetermined"),
            PermissionStatus::NotDetermined
        );
        assert_eq!(
            PermissionStatus::decode("restricted"),
            PermissionStatus::Restricted
        );
        assert_eq!(PermissionStatus::decode("bogus"), PermissionStatus::Denied);
        assert!(PermissionStatus::Always.is_granted());
        assert!(!PermissionStatus::Restricted.is_granted());
    }

    #[test]
    fn decodes_updates() {
        let update: Update = serde_json::from_str(
            r#"{"type":"position","latitude":35.6,"longitude":139.7,"accuracy":5,"timestampMs":1}"#,
        )
        .unwrap();
        let Update::Position(position) = update else {
            panic!("expected a position")
        };
        assert_eq!(position.latitude, 35.6);
        assert_eq!(position.speed, 0.0);
        assert_eq!(position.timestamp_ms, 1);
        let update: Update =
            serde_json::from_str(r#"{"type":"error","code":"serviceDisabled"}"#).unwrap();
        let Update::Error(error) = update else {
            panic!("expected an error")
        };
        assert_eq!(error.code, LocationErrorCode::ServiceDisabled);
    }

    #[test]
    fn encodes_settings() {
        let settings = LocationSettings {
            background: true,
            android_notification: AndroidLocationNotification {
                reuse_notification_id: Some(7),
                ..Default::default()
            },
            ..Default::default()
        };
        let value = serde_json::to_value(&settings).unwrap();
        assert_eq!(value["accuracy"], "high");
        assert_eq!(value["intervalMs"], 10_000);
        assert_eq!(value["androidNotification"]["reuseNotificationId"], 7);
        assert_eq!(duration_ms(Duration::from_secs(2)), 2000);
    }

    #[test]
    fn measures_distances() {
        let tokyo = (35.681236, 139.767125);
        let shinjuku = (35.690921, 139.700258);
        let distance = distance_between(tokyo, shinjuku);
        assert!((distance - 6_150.0).abs() < 100.0, "{distance}");
        assert_eq!(distance_between(tokyo, tokyo), 0.0);
    }
}
