//! Daily and one-off local notifications, with notification taps.
//!
//! Schedules are persisted on the host and re-armed after device reboots and app updates.
//! Call [`configure`] first to set the notification channel and small icon.
//! With [`LocalNotificationMasamuneAdapter`] in the app's `MasamuneAdapters`, configuring,
//! re-arming and receiving taps happen at startup.
//!
//! # Scheduling
//!
//! - [`schedule_daily`] / [`schedule_daily_with_payload`]: every day at `hour:minute` (local time).
//! - [`schedule_once`]: once at a time in UTC milliseconds. Times in the past are rejected. The
//!   schedule is removed after it fires (and dropped when it is re-armed after its time).
//!
//! Scheduling again with the same ID replaces the schedule, whichever kind it was. The optional
//! payload is a string handed back with the tap (for example a route or a JSON document).
//!
//! # Taps
//!
//! Tapping one of this crate's notifications opens the app and reports a [`NotificationTap`] with
//! the schedule's ID (as passed when scheduling) and payload:
//!
//! - [`initial_tap`] returns the tap that launched the app (cold start) once.
//! - [`on_tap`] receives taps while the app is running (in the foreground or the background).
//!   Taps that arrive before the first subscription are delivered when it starts.
//!
//! The adapter subscribes in `prepare`, reads the launch tap in `start` and publishes both to
//! [`latest_tap`]; take it with [`take_pending_tap`] once handled, for example to navigate:
//!
//! ```rust,ignore
//! use masamune_whisker_local_notification::{latest_tap, take_pending_tap};
//!
//! // In a component: navigate when a tap arrives.
//! effect(move || {
//!     if latest_tap().is_some() && let Some(tap) = take_pending_tap() {
//!         // tap.id, tap.payload
//!     }
//! });
//! ```
//!
//! Without the adapter, use the functions directly:
//!
//! ```rust,ignore
//! if let Some(tap) = masamune_whisker_local_notification::initial_tap().await { /* … */ }
//! let subscription = masamune_whisker_local_notification::on_tap(|tap| { /* … */ });
//! ```
//!
//! # Platforms
//!
//! On iOS the schedules are `UNCalendarNotificationTrigger` requests (repeating for daily ones),
//! which iOS keeps across reboots and updates by itself. Like on Android, the notifications are
//! also shown while the app is in the foreground: the module installs a
//! `UNUserNotificationCenterDelegate` in front of the one already set (for example Firebase
//! Messaging), handles its own notifications and forwards every other notification to it. The
//! delegate is installed again after launch and whenever the app becomes active, so a delegate set
//! later by another module (Firebase Messaging sets one when it is registered) is wrapped too.
//!
//! On Android the schedules are `AlarmManager` alarms. A tap opens the app's launch activity with
//! `FLAG_ACTIVITY_SINGLE_TOP`; the launch intent is read by [`initial_tap`] and later intents arrive
//! through the activity's `onNewIntent`. Each tap is delivered once, and intents relaunched from the
//! recent apps are ignored.

mod adapter;

pub use adapter::{LocalNotificationMasamuneAdapter, latest_tap, take_pending_tap};
use masamune_whisker_bridge::NativeModule;
pub use masamune_whisker_bridge::{NativeError, NativeResult, Subscription};
use serde::Deserialize;
use serde_json::json;

fn module() -> NativeModule {
    NativeModule::new(env!("CARGO_PKG_NAME"), "LocalNotification")
}

/// Notification channel and appearance settings.
#[derive(Debug, Clone, Default)]
pub struct NotificationConfig<'a> {
    pub channel_id: &'a str,
    pub channel_name: &'a str,
    pub channel_description: &'a str,
    /// Small icon (a path resolved with `whisker_asset::resolve`). `None` uses the app icon.
    pub small_icon: Option<String>,
}

/// A saved schedule.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Schedule {
    pub id: String,
    /// The local time of day. For a one-off schedule, the local time it fires at.
    #[serde(default)]
    pub hour: u32,
    #[serde(default)]
    pub minute: u32,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub text: String,
    /// The string handed back in [`NotificationTap::payload`].
    #[serde(default)]
    pub payload: Option<String>,
    /// Whether this is a one-off schedule ([`schedule_once`]).
    #[serde(default)]
    pub once: bool,
    /// When a one-off schedule fires (UTC milliseconds). `None` for daily schedules.
    #[serde(default, rename = "atMs")]
    pub at_ms: Option<i64>,
}

/// A tap on one of this crate's notifications.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct NotificationTap {
    /// The schedule ID passed to [`schedule_daily`] / [`schedule_once`].
    pub id: String,
    /// The payload passed when scheduling, if any.
    #[serde(default)]
    pub payload: Option<String>,
}

/// Sets the notification channel and small icon (once at startup).
///
/// Android only: iOS has no notification channels and always uses the app icon, so the values are
/// ignored there (the call still succeeds).
pub fn configure(config: &NotificationConfig<'_>) -> NativeResult<bool> {
    module().call(
        "configure",
        &json!({
            "channelId": config.channel_id,
            "channelName": config.channel_name,
            "channelDescription": config.channel_description,
            "smallIcon": config.small_icon.as_deref().unwrap_or(""),
        }),
    )
}

/// Whether notifications are allowed (for both the app and the channel).
///
/// On iOS this returns the authorization status the module caches at startup, after
/// [`request_permission`] and whenever the app becomes active (authorized, provisional and
/// ephemeral count as allowed).
pub fn authorized() -> bool {
    module()
        .call::<_, String>("status", &json!({}))
        .is_ok_and(|s| s == "authorized")
}

/// Notification permission state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionStatus {
    /// Notifications can be shown.
    Authorized,
    /// The user turned notifications off; only the system settings can change it.
    Denied,
    /// The app has not asked yet, so [`request_permission`] shows the system prompt (iOS only;
    /// Android reports a missing permission as [`PermissionStatus::Denied`]).
    NotDetermined,
}

/// The current permission state (cached on iOS like [`authorized`]).
pub fn permission_status() -> PermissionStatus {
    match module().call::<_, String>("status", &json!({})).as_deref() {
        Ok("authorized") => PermissionStatus::Authorized,
        Ok("notDetermined") => PermissionStatus::NotDetermined,
        _ => PermissionStatus::Denied,
    }
}

/// Requests notification permission. Returns `true` if granted.
pub async fn request_permission() -> NativeResult<bool> {
    module().call_async("requestPermission", &json!({})).await
}

/// Schedules a daily notification at `hour:minute` (replacing any with the same ID). Returns the
/// next fire time (UTC milliseconds).
///
/// Same as [`schedule_daily_with_payload`] without a payload.
pub fn schedule_daily(
    id: &str,
    hour: u32,
    minute: u32,
    title: &str,
    text: &str,
) -> NativeResult<i64> {
    schedule_daily_with_payload(id, hour, minute, title, text, None)
}

/// Schedules a daily notification at `hour:minute` (replacing any with the same ID) whose taps
/// carry `payload`. Returns the next fire time (UTC milliseconds).
pub fn schedule_daily_with_payload(
    id: &str,
    hour: u32,
    minute: u32,
    title: &str,
    text: &str,
    payload: Option<&str>,
) -> NativeResult<i64> {
    module().call(
        "scheduleDaily",
        &json!({
            "id": id,
            "hour": hour,
            "minute": minute,
            "title": title,
            "text": text,
            "payload": payload,
        }),
    )
}

/// Schedules a notification once at `at_ms` (UTC milliseconds), replacing any schedule with the
/// same ID. Returns the fire time (UTC milliseconds).
///
/// Fails when `at_ms` is not in the future. iOS fires at whole seconds; Android uses an exact
/// alarm when the app may schedule one and an inexact one otherwise (like daily schedules).
pub fn schedule_once(
    id: &str,
    at_ms: i64,
    title: &str,
    text: &str,
    payload: Option<&str>,
) -> NativeResult<i64> {
    module().call(
        "scheduleOnce",
        &json!({
            "id": id,
            "atMs": at_ms,
            "title": title,
            "text": text,
            "payload": payload,
        }),
    )
}

/// Returns the saved schedule (`None` for a one-off schedule that has already fired).
pub fn schedule(id: &str) -> NativeResult<Option<Schedule>> {
    module().call("schedule", &json!({ "id": id }))
}

/// Re-arms all saved schedules (at startup) and drops one-off schedules whose time has passed.
///
/// Not needed on iOS, where requests survive by themselves; it re-registers the saved schedules,
/// which is harmless.
pub fn rearm() -> NativeResult<bool> {
    module().call("rearm", &json!({}))
}

pub fn cancel(id: &str) -> NativeResult<bool> {
    module().call("cancel", &json!({ "id": id }))
}

pub fn cancel_all() -> NativeResult<bool> {
    module().call("cancelAll", &json!({}))
}

/// Opens this app's notification settings screen.
///
/// On iOS 16 and later this opens the app's notification settings, on iOS 15 the app's settings.
pub fn open_settings() -> NativeResult<bool> {
    module().call("openSettings", &json!({}))
}

/// The notification tap that launched the app, if any. It is consumed: later calls return `None`.
///
/// - iOS: the tap response delivered at launch. Asked early, the call waits until the app has been
///   active for a moment (at most about two seconds) for the response to arrive.
/// - Android: the launch activity's intent, once the activity is attached (ignored when the app is
///   reopened from the recent apps).
///
/// Errors (and platforms without the native module) return `None`.
pub async fn initial_tap() -> Option<NotificationTap> {
    module()
        .call_async::<_, Option<NotificationTap>>("getInitialTap", &json!({}))
        .await
        .ok()
        .flatten()
}

/// Receives taps on this crate's notifications while the returned [`Subscription`] is held.
///
/// Taps that arrive while nobody subscribes are queued and delivered when a subscription starts.
/// The tap that launched the app is returned by [`initial_tap`] instead (on iOS, a launch tap that
/// nobody asks for within a few seconds of launch is delivered here).
pub fn on_tap(handler: impl Fn(NotificationTap) + 'static) -> Subscription {
    module().subscribe("tap", move |tap: NotificationTap| {
        if !tap.id.is_empty() {
            handler(tap);
        }
    })
}
