//! Daily local notifications at a fixed time.
//!
//! Schedules are persisted on the host and re-armed after device reboots and app updates.
//! Call [`configure`] first to set the notification channel and small icon.

use masamune_whisker_bridge::NativeModule;
pub use masamune_whisker_bridge::{NativeError, NativeResult};
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
    pub hour: u32,
    pub minute: u32,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub text: String,
}

/// Sets the notification channel and small icon (once at startup).
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
pub fn authorized() -> bool {
    module()
        .call::<_, String>("status", &json!({}))
        .is_ok_and(|s| s == "authorized")
}

/// Requests notification permission. Returns `true` if granted.
pub async fn request_permission() -> NativeResult<bool> {
    module().call_async("requestPermission", &json!({})).await
}

/// Schedules a daily notification at `hour:minute` (replacing any with the same ID). Returns the next fire time (UTC milliseconds).
pub fn schedule_daily(
    id: &str,
    hour: u32,
    minute: u32,
    title: &str,
    text: &str,
) -> NativeResult<i64> {
    module().call(
        "scheduleDaily",
        &json!({ "id": id, "hour": hour, "minute": minute, "title": title, "text": text }),
    )
}

/// Returns the saved schedule.
pub fn schedule(id: &str) -> NativeResult<Option<Schedule>> {
    module().call("schedule", &json!({ "id": id }))
}

/// Re-arms all saved schedules (at startup).
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
pub fn open_settings() -> NativeResult<bool> {
    module().call("openSettings", &json!({}))
}
