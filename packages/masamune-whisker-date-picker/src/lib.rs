//! Native OS date and time picker dialogs.

use masamune_whisker_bridge::NativeModule;
pub use masamune_whisker_bridge::{NativeError, NativeResult};
use serde::Deserialize;
use serde_json::{Value, json};

fn module() -> NativeModule {
    NativeModule::new(env!("CARGO_PKG_NAME"), "DatePicker")
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
pub struct PickedDate {
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
pub struct PickedTime {
    pub hour: u32,
    pub minute: u32,
}

/// Date dialog. `initial` is (year, month 1-12, day). `min_ms` and `max_ms` are UTC milliseconds. Returns `None` if cancelled.
///
/// On iOS an inline Gregorian calendar (`UIDatePicker`) is shown in a centered dialog with Cancel /
/// Done; tapping outside the dialog cancels, like on Android.
pub async fn pick_date(
    initial: (i32, u32, u32),
    min_ms: Option<i64>,
    max_ms: Option<i64>,
) -> NativeResult<Option<PickedDate>> {
    let mut args = json!({ "year": initial.0, "month": initial.1, "day": initial.2 });
    if let Some(min) = min_ms {
        args["minMillis"] = Value::from(min);
    }
    if let Some(max) = max_ms {
        args["maxMillis"] = Value::from(max);
    }
    module().call_async("pickDate", &args).await
}

/// Time dialog (follows the device's 12/24-hour setting). Returns `None` if cancelled.
///
/// On iOS a wheel `UIDatePicker` is shown in the same dialog as [`pick_date`].
pub async fn pick_time(hour: u32, minute: u32) -> NativeResult<Option<PickedTime>> {
    module()
        .call_async("pickTime", &json!({ "hour": hour, "minute": minute }))
        .await
}
