#![doc = include_str!("../README.md")]

pub mod cng;

use std::cell::Cell;

use masamune_whisker_bridge::NativeModule;
pub use masamune_whisker_bridge::{NativeError, NativeResult, Subscription};
use serde::Deserialize;
use serde_json::json;

fn module() -> NativeModule {
    NativeModule::new(env!("CARGO_PKG_NAME"), "Activity")
}

/// Motion activity permission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionStatus {
    /// Not asked yet: [`request_permission`] shows the system prompt.
    NotDetermined,
    /// Refused. Only the system settings can change it.
    Denied,
    /// Blocked by a device policy (iOS).
    Restricted,
    Granted,
    /// The device cannot recognize activities (no motion coprocessor on iOS, no Google Play
    /// services on Android).
    Unsupported,
}

impl PermissionStatus {
    fn decode(value: &str) -> Self {
        match value {
            "notDetermined" => Self::NotDetermined,
            "restricted" => Self::Restricted,
            "granted" => Self::Granted,
            "unsupported" => Self::Unsupported,
            _ => Self::Denied,
        }
    }
}

/// The current permission. Errors (and platforms without the module) read as
/// [`PermissionStatus::Unsupported`].
pub fn permission_status() -> PermissionStatus {
    module()
        .call::<_, String>("status", &json!({}))
        .map_or(PermissionStatus::Unsupported, |status| {
            PermissionStatus::decode(&status)
        })
}

/// Asks for the permission and returns the resulting status.
///
/// - iOS: Motion & Fitness. iOS has no explicit request, so the module runs a short activity
///   query, which shows the prompt.
/// - Android: `ACTIVITY_RECOGNITION` (Android 10 and later; granted at install before).
pub async fn request_permission() -> NativeResult<PermissionStatus> {
    let status: String = module().call_async("requestPermission", &json!({})).await?;
    Ok(PermissionStatus::decode(&status))
}

/// Opens this app's page in the system settings.
pub fn open_app_settings() -> NativeResult<bool> {
    module().call("openAppSettings", &json!({}))
}

/// The recognized activity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ActivityKind {
    /// Not moving (iOS `stationary`, Android `STILL`).
    Stationary,
    Walking,
    Running,
    /// In a car, bus or train (iOS `automotive`, Android `IN_VEHICLE`).
    Automotive,
    Cycling,
    /// Moving in an unknown way, or no activity is known.
    Unknown,
}

impl ActivityKind {
    /// Whether the user is moving (walking, running, cycling or in a vehicle).
    pub fn is_moving(self) -> bool {
        matches!(
            self,
            Self::Walking | Self::Running | Self::Automotive | Self::Cycling
        )
    }
}

/// A change of the user's activity.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityEvent {
    pub kind: ActivityKind,
    /// Confidence from 0 to 1 (iOS low / medium / high as 0.33 / 0.66 / 1.0; Android transitions
    /// are reported with 1.0).
    pub confidence: f64,
    /// When the activity started, in UTC milliseconds.
    pub timestamp_ms: i64,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum Update {
    Activity(ActivityEvent),
    Error {
        #[serde(default)]
        message: String,
    },
}

thread_local! {
    static WATCHERS: Cell<usize> = const { Cell::new(0) };
}

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
            log::debug!("activity.stop_failed: {error}");
        }
    }
}

/// Starts activity recognition and calls `handler` with each change until the returned
/// [`Subscription`] is dropped. Errors (permission revoked, Google Play services failures) arrive
/// as `Err` with a message; updates stop after an error.
///
/// - iOS: `CMMotionActivityManager.startActivityUpdates`. Updates with no activity flag are
///   skipped; several flags report the most specific one (automotive, cycling, running, walking,
///   stationary).
/// - Android: the Activity Recognition Transition API, entering `IN_VEHICLE`, `ON_BICYCLE`,
///   `RUNNING`, `WALKING` and `STILL`. Transitions are delivered while the app process runs.
///
/// One native stream serves every subscription; it stops when the last one is dropped.
pub fn watch_activity(
    handler: impl Fn(Result<ActivityEvent, String>) + 'static,
) -> NativeResult<Subscription> {
    let events = module().subscribe("update", move |update: Update| match update {
        Update::Activity(event) => handler(Ok(event)),
        Update::Error { message } => handler(Err(message)),
    });
    module().call::<_, bool>("startUpdates", &json!({}))?;
    WATCHERS.with(|count| count.set(count.get() + 1));
    Ok(Subscription::new((events, WatchGuard)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_statuses() {
        assert_eq!(
            PermissionStatus::decode("granted"),
            PermissionStatus::Granted
        );
        assert_eq!(
            PermissionStatus::decode("unsupported"),
            PermissionStatus::Unsupported
        );
        assert_eq!(
            PermissionStatus::decode("notDetermined"),
            PermissionStatus::NotDetermined
        );
        assert_eq!(PermissionStatus::decode("x"), PermissionStatus::Denied);
    }

    #[test]
    fn decodes_updates() {
        let update: Update = serde_json::from_str(
            r#"{"type":"activity","kind":"automotive","confidence":0.66,"timestampMs":5}"#,
        )
        .unwrap();
        let Update::Activity(event) = update else {
            panic!("expected an activity")
        };
        assert_eq!(event.kind, ActivityKind::Automotive);
        assert!(event.kind.is_moving());
        assert!(!ActivityKind::Stationary.is_moving());
        assert_eq!(event.timestamp_ms, 5);
        let update: Update = serde_json::from_str(r#"{"type":"error","message":"no"}"#).unwrap();
        assert!(matches!(update, Update::Error { message } if message == "no"));
    }
}
