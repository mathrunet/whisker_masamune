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
//! # Urgent notifications
//!
//! [`schedule_daily_with_options`] / [`schedule_once_with_options`] take [`NotificationOptions`]:
//!
//! - `channel` (Android): post to a channel created with [`create_channel`], whose
//!   [`NotificationImportance`] decides how the notification interrupts (`High` peeks in as a
//!   heads-up notification and makes a sound). Without it, the channel of [`configure`] is used.
//!   [`LocalNotificationMasamuneAdapter::channel`] creates channels at startup.
//! - `full_screen` (Android): also attach the content intent as a full-screen intent, so an idle or
//!   locked device opens the app at once (like an alarm or an incoming call). Android 14 and later
//!   grant this only to calling and alarm apps; check [`can_use_full_screen_intent`] and send the
//!   user to [`open_full_screen_intent_settings`]. The app must declare
//!   `android.permission.USE_FULL_SCREEN_INTENT` itself (for example with whisker-cng's
//!   `AndroidPermissions` plugin) and Google Play requires a declaration for it, which is why this
//!   crate does not add it. Opening the app this way is reported like a tap ([`on_tap`]).
//! - `interruption_level` (iOS): [`InterruptionLevel::Passive`] (no sound, no screen wake),
//!   `Active` (the default) or `TimeSensitive` (breaks through Focus and scheduled summaries). Time
//!   Sensitive needs the app's `com.apple.developer.usernotifications.time-sensitive` entitlement
//!   (enable the "Time Sensitive Notifications" capability of the App ID and add the key to the
//!   app's entitlements); without it iOS delivers the notification as `Active`. The user can turn
//!   it off per app: read it with [`time_sensitive_setting`].
//!
//! [`channel_status`] / [`channel_enabled`] tell whether the user turned a channel off or lowered
//! its importance in the system settings (Android; on iOS they report the app's permission).
//! Options that do not apply to a platform are ignored there, and calls without options keep the
//! previous behavior.
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

/// Importance of an Android notification channel (`NotificationManager.IMPORTANCE_*`).
///
/// It decides how notifications on the channel interrupt. The user can change it in the system
/// settings, and an app can only lower it afterwards: to raise it, create a channel with a new ID
/// (a deleted channel that is created again with the same ID gets its old settings back).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum NotificationImportance {
    /// The channel is blocked (only reported by [`channel_status`]).
    None,
    /// Shown in the shade only, collapsed.
    Min,
    /// Shown everywhere, without sound.
    Low,
    /// Shown everywhere, with sound (the importance of the [`configure`] channel).
    #[default]
    Default,
    /// Sound and a heads-up notification.
    High,
}

impl NotificationImportance {
    fn as_native(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Min => "min",
            Self::Low => "low",
            Self::Default => "default",
            Self::High => "high",
        }
    }

    fn from_native(value: &str) -> Self {
        match value {
            "none" => Self::None,
            "min" => Self::Min,
            "low" => Self::Low,
            "high" => Self::High,
            _ => Self::Default,
        }
    }
}

/// An Android notification channel created with [`create_channel`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationChannel {
    /// Channel ID, passed to [`NotificationOptions::channel`].
    pub id: String,
    /// Name shown in the system settings.
    pub name: String,
    /// Description shown in the system settings (empty for none).
    pub description: String,
    /// Importance when the channel is created.
    pub importance: NotificationImportance,
}

impl NotificationChannel {
    /// A channel with [`NotificationImportance::Default`] and no description.
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: String::new(),
            importance: NotificationImportance::Default,
        }
    }

    /// Sets the description.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// Sets the importance.
    pub fn importance(mut self, importance: NotificationImportance) -> Self {
        self.importance = importance;
        self
    }
}

/// How an iOS notification interrupts (`UNNotificationInterruptionLevel`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum InterruptionLevel {
    /// Added to the list without sound or lighting up the screen.
    Passive,
    /// Presented at once with sound (the iOS default).
    #[default]
    Active,
    /// Presented at once, also during Focus and outside scheduled summaries. Needs the Time
    /// Sensitive entitlement (see the crate documentation).
    TimeSensitive,
}

impl InterruptionLevel {
    fn as_native(self) -> &'static str {
        match self {
            Self::Passive => "passive",
            Self::Active => "active",
            Self::TimeSensitive => "timeSensitive",
        }
    }
}

/// Per-notification options of [`schedule_daily_with_options`] / [`schedule_once_with_options`].
///
/// The default keeps the behavior of the functions without options.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NotificationOptions {
    channel_id: Option<String>,
    interruption_level: Option<InterruptionLevel>,
    full_screen: bool,
}

impl NotificationOptions {
    /// No options.
    pub fn new() -> Self {
        Self::default()
    }

    /// Posts to the Android channel `channel_id` (created with [`create_channel`]; a missing
    /// channel falls back to the [`configure`] channel). Ignored on iOS.
    pub fn channel(mut self, channel_id: impl Into<String>) -> Self {
        self.channel_id = Some(channel_id.into());
        self
    }

    /// Sets the iOS interruption level. Ignored on Android (use a channel's importance).
    pub fn interruption_level(mut self, level: InterruptionLevel) -> Self {
        self.interruption_level = Some(level);
        self
    }

    /// Attaches a full-screen intent on Android (see the crate documentation). Ignored on iOS.
    pub fn full_screen(mut self, full_screen: bool) -> Self {
        self.full_screen = full_screen;
        self
    }

    /// The channel set with [`channel`](Self::channel).
    pub fn get_channel(&self) -> Option<&str> {
        self.channel_id.as_deref()
    }

    /// The interruption level set with [`interruption_level`](Self::interruption_level).
    pub fn get_interruption_level(&self) -> Option<InterruptionLevel> {
        self.interruption_level
    }

    /// Whether a full-screen intent is attached.
    pub fn is_full_screen(&self) -> bool {
        self.full_screen
    }

    /// Adds the set options to a schedule request (nothing for the default).
    fn extend(&self, request: &mut serde_json::Value) {
        let Some(map) = request.as_object_mut() else {
            return;
        };
        if let Some(channel_id) = &self.channel_id {
            map.insert("channelId".into(), json!(channel_id));
        }
        if let Some(level) = self.interruption_level {
            map.insert("interruptionLevel".into(), json!(level.as_native()));
        }
        if self.full_screen {
            map.insert("fullScreen".into(), json!(true));
        }
    }
}

/// Creates (or renames) an Android notification channel. Returns `true` on success.
///
/// Creating an existing channel updates its name and description; its importance can only be
/// lowered (see [`NotificationImportance`]). iOS has no channels: the call does nothing and
/// succeeds.
pub fn create_channel(channel: &NotificationChannel) -> NativeResult<bool> {
    module().call(
        "createChannel",
        &json!({
            "id": channel.id,
            "name": channel.name,
            "description": channel.description,
            "importance": channel.importance.as_native(),
        }),
    )
}

/// Deletes an Android notification channel (does nothing on iOS).
pub fn delete_channel(id: &str) -> NativeResult<bool> {
    module().call("deleteChannel", &json!({ "id": id }))
}

/// State of a notification channel, from [`channel_status`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelStatus {
    /// Whether notifications on the channel are shown: the app may post notifications and the
    /// channel is not blocked.
    pub enabled: bool,
    /// The current importance, including changes the user made in the system settings.
    pub importance: NotificationImportance,
}

#[derive(Deserialize)]
struct NativeChannelStatus {
    #[serde(default)]
    exists: bool,
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    importance: String,
}

/// The state of the channel `id`, or `None` when it has not been created.
///
/// On iOS (no channels) every ID exists, `enabled` is the app's permission ([`authorized`]) and
/// the importance is [`NotificationImportance::Default`].
pub fn channel_status(id: &str) -> NativeResult<Option<ChannelStatus>> {
    let status: Option<NativeChannelStatus> =
        module().call("channelStatus", &json!({ "id": id }))?;
    Ok(status
        .filter(|status| status.exists)
        .map(|status| ChannelStatus {
            enabled: status.enabled,
            importance: NotificationImportance::from_native(&status.importance),
        }))
}

/// Whether notifications on the channel `id` are shown (`false` when it does not exist or on
/// errors). See [`channel_status`].
pub fn channel_enabled(id: &str) -> bool {
    matches!(channel_status(id), Ok(Some(status)) if status.enabled)
}

/// A per-app notification setting the user can change (`UNNotificationSetting`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationSetting {
    /// The setting is on.
    Enabled,
    /// The user turned it off.
    Disabled,
    /// The setting does not apply: Android, or an iOS app without the entitlement.
    NotSupported,
}

/// Whether the user allows Time Sensitive notifications for this app
/// (`UNNotificationSettings.timeSensitiveSetting`). Android and errors report
/// [`NotificationSetting::NotSupported`].
pub async fn time_sensitive_setting() -> NotificationSetting {
    match module()
        .call_async::<_, String>("timeSensitiveSetting", &json!({}))
        .await
        .as_deref()
    {
        Ok("enabled") => NotificationSetting::Enabled,
        Ok("disabled") => NotificationSetting::Disabled,
        _ => NotificationSetting::NotSupported,
    }
}

/// Whether full-screen intents are shown (Android: `NotificationManager.canUseFullScreenIntent()`
/// on Android 14 and later, the `USE_FULL_SCREEN_INTENT` declaration before). Always `false` on
/// iOS.
pub fn can_use_full_screen_intent() -> bool {
    module()
        .call::<_, bool>("canUseFullScreenIntent", &json!({}))
        .unwrap_or(false)
}

/// Opens the system page where the user allows full-screen intents for this app (Android 14 and
/// later; the app's notification settings before). Returns `false` on iOS.
pub fn open_full_screen_intent_settings() -> NativeResult<bool> {
    module().call("openFullScreenIntentSettings", &json!({}))
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
    schedule_daily_with_options(
        id,
        hour,
        minute,
        title,
        text,
        payload,
        &NotificationOptions::default(),
    )
}

/// [`schedule_daily_with_payload`] with [`NotificationOptions`] (channel, full-screen intent,
/// interruption level). The options are saved with the schedule and apply to every occurrence.
pub fn schedule_daily_with_options(
    id: &str,
    hour: u32,
    minute: u32,
    title: &str,
    text: &str,
    payload: Option<&str>,
    options: &NotificationOptions,
) -> NativeResult<i64> {
    let mut request = json!({
        "id": id,
        "hour": hour,
        "minute": minute,
        "title": title,
        "text": text,
        "payload": payload,
    });
    options.extend(&mut request);
    module().call("scheduleDaily", &request)
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
    schedule_once_with_options(
        id,
        at_ms,
        title,
        text,
        payload,
        &NotificationOptions::default(),
    )
}

/// [`schedule_once`] with [`NotificationOptions`] (channel, full-screen intent, interruption
/// level).
pub fn schedule_once_with_options(
    id: &str,
    at_ms: i64,
    title: &str,
    text: &str,
    payload: Option<&str>,
    options: &NotificationOptions,
) -> NativeResult<i64> {
    let mut request = json!({
        "id": id,
        "atMs": at_ms,
        "title": title,
        "text": text,
        "payload": payload,
    });
    options.extend(&mut request);
    module().call("scheduleOnce", &request)
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
