//! 毎日決まった時刻のローカル通知。
//!
//! 予約は Host 側に保存され、端末の再起動・アプリの更新後も入れ直される。
//! 最初に [`configure`] で通知チャンネルと小さいアイコンを設定する。

use masamune_whisker_bridge::NativeModule;
pub use masamune_whisker_bridge::{NativeError, NativeResult};
use serde::Deserialize;
use serde_json::json;

fn module() -> NativeModule {
    NativeModule::new(env!("CARGO_PKG_NAME"), "LocalNotification")
}

/// 通知チャンネルと見た目の設定。
#[derive(Debug, Clone, Default)]
pub struct NotificationConfig<'a> {
    pub channel_id: &'a str,
    pub channel_name: &'a str,
    pub channel_description: &'a str,
    /// 小さいアイコン（`whisker_asset::resolve` したパス）。`None` ならアプリのアイコン。
    pub small_icon: Option<String>,
}

/// 保存済みの予約。
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

/// 通知チャンネルと小さいアイコンを設定する（起動時に 1 回）。
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

/// 通知が許可されているか（アプリ全体とチャンネル）。
pub fn authorized() -> bool {
    module()
        .call::<_, String>("status", &json!({}))
        .is_ok_and(|s| s == "authorized")
}

/// 通知の許可を求める。許可されたら `true`。
pub async fn request_permission() -> NativeResult<bool> {
    module().call_async("requestPermission", &json!({})).await
}

/// 毎日 `hour:minute` の通知を予約する（同じ ID は置き換え）。次の通知の時刻（UTC ミリ秒）を返す。
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

/// 保存済みの予約。
pub fn schedule(id: &str) -> NativeResult<Option<Schedule>> {
    module().call("schedule", &json!({ "id": id }))
}

/// 保存済みの予約をすべて入れ直す（起動時）。
pub fn rearm() -> NativeResult<bool> {
    module().call("rearm", &json!({}))
}

pub fn cancel(id: &str) -> NativeResult<bool> {
    module().call("cancel", &json!({ "id": id }))
}

pub fn cancel_all() -> NativeResult<bool> {
    module().call("cancelAll", &json!({}))
}

/// このアプリの通知設定画面を開く。
pub fn open_settings() -> NativeResult<bool> {
    module().call("openSettings", &json!({}))
}
