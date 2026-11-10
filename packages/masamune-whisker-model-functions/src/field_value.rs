//! Masamune `ModelFieldValue` JSON (`katana_model/lib/src/model_field_value.dart`,
//! `model_ref.dart`; `@mathrunet/masamune` `model_field_value.ts`).
//!
//! The model Functions return documents as Masamune writes them, so typed fields arrive as maps
//! tagged with `@type` instead of plain values:
//!
//! | Type | JSON |
//! |---|---|
//! | `ModelTimestamp` | `{"@type": "ModelTimestamp", "@time": <µs since epoch>, "@source": "server"}` |
//! | `ModelDate` / `ModelTime` | same shape with `"@type": "ModelDate"` / `"ModelTime"` |
//! | `ModelRefBase` | `{"@type": "ModelRefBase", "@ref": "user_meta/abc"}` |
//! | `ModelCounter` | `{"@type": "ModelCounter", "@value": 3, "@increment": 0}` |
//!
//! Rows keep these maps untouched (so they can be saved back as-is); use the helpers here to
//! read and build them. [`comparable`] turns them into plain values, which is how
//! [`FunctionsStore::query`](crate::FunctionsStore) filters and sorts them.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use masamune_whisker_model::Row;
use serde_json::{Value, json};

/// Field holding the `@type` of a `ModelFieldValue`.
pub const TYPE_KEY: &str = "@type";
/// Field holding the document id in Masamune documents (`kUidFieldKey`).
pub const UID_KEY: &str = "@uid";
/// Field holding the time of `ModelTimestamp` / `ModelDate` / `ModelTime` (microseconds).
pub const TIME_KEY: &str = "@time";
/// Field holding the document path of a `ModelRefBase`.
pub const REF_KEY: &str = "@ref";

pub const MODEL_TIMESTAMP: &str = "ModelTimestamp";
pub const MODEL_DATE: &str = "ModelDate";
pub const MODEL_TIME: &str = "ModelTime";
pub const MODEL_REF: &str = "ModelRefBase";
pub const MODEL_COUNTER: &str = "ModelCounter";

/// `@type` of a `ModelFieldValue` map (`None` for plain values).
pub fn field_type(value: &Value) -> Option<&str> {
    value.as_object()?.get(TYPE_KEY)?.as_str()
}

/// Microseconds since the Unix epoch of a `ModelTimestamp`, `ModelDate` or `ModelTime`.
///
/// Raw Firestore timestamps serialized by the Admin SDK (`{"_seconds", "_nanoseconds"}` or
/// `{"seconds", "nanoseconds"}`) are accepted as well. Plain numbers and strings are not.
pub fn timestamp_micros(value: &Value) -> Option<i64> {
    let map = value.as_object()?;
    match map.get(TYPE_KEY).and_then(Value::as_str) {
        Some(MODEL_TIMESTAMP | MODEL_DATE | MODEL_TIME) => {
            let time = map.get(TIME_KEY)?;
            time.as_i64().or_else(|| time.as_f64().map(|v| v as i64))
        }
        Some(_) => None,
        None => {
            let seconds = map
                .get("_seconds")
                .or_else(|| map.get("seconds"))?
                .as_i64()?;
            let nanos = map
                .get("_nanoseconds")
                .or_else(|| map.get("nanoseconds"))
                .and_then(Value::as_i64)
                .unwrap_or(0);
            Some(seconds * 1_000_000 + nanos / 1_000)
        }
    }
}

/// [`timestamp_micros`] as a [`SystemTime`].
pub fn timestamp(value: &Value) -> Option<SystemTime> {
    let micros = timestamp_micros(value)?;
    if micros >= 0 {
        UNIX_EPOCH.checked_add(Duration::from_micros(micros as u64))
    } else {
        UNIX_EPOCH.checked_sub(Duration::from_micros(micros.unsigned_abs()))
    }
}

/// Time of the timestamp field `key` of `row` (`None` when missing or not a timestamp).
pub fn row_timestamp(row: &Row, key: &str) -> Option<SystemTime> {
    timestamp(row.get(key)?)
}

/// Microseconds since the Unix epoch of `time` (negative before 1970).
pub fn micros_since_epoch(time: SystemTime) -> i64 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(after) => after.as_micros() as i64,
        Err(before) => -(before.duration().as_micros() as i64),
    }
}

/// `ModelTimestamp` JSON as Dart's `ModelTimestamp(time).toJson()` writes it.
pub fn model_timestamp(time: SystemTime) -> Value {
    json!({
        TYPE_KEY: MODEL_TIMESTAMP,
        TIME_KEY: micros_since_epoch(time),
        "@now": false,
        "@source": "user",
    })
}

/// `ModelTimestamp.now()` JSON (`@now: true`, with the current client time).
pub fn model_timestamp_now() -> Value {
    json!({
        TYPE_KEY: MODEL_TIMESTAMP,
        TIME_KEY: micros_since_epoch(SystemTime::now()),
        "@now": true,
        "@source": "user",
    })
}

/// Document path of a `ModelRefBase` (`"user_meta/abc"`).
pub fn ref_path(value: &Value) -> Option<&str> {
    let map = value.as_object()?;
    if map.get(TYPE_KEY).and_then(Value::as_str) != Some(MODEL_REF) {
        return None;
    }
    map.get(REF_KEY)?.as_str()
}

/// Last segment of a `ModelRefBase` path (the referenced document id).
pub fn ref_id(value: &Value) -> Option<&str> {
    ref_path(value)?
        .trim_matches('/')
        .rsplit('/')
        .next()
        .filter(|id| !id.is_empty())
}

/// `ModelRefBase` JSON for the document at `path` (slashes at both ends are removed).
pub fn model_ref(path: &str) -> Value {
    json!({ TYPE_KEY: MODEL_REF, REF_KEY: path.trim_matches('/') })
}

/// Value of a `ModelCounter` (`@value + @increment`).
pub fn counter_value(value: &Value) -> Option<f64> {
    let map = value.as_object()?;
    if map.get(TYPE_KEY).and_then(Value::as_str) != Some(MODEL_COUNTER) {
        return None;
    }
    let base = map.get("@value").and_then(Value::as_f64).unwrap_or(0.0);
    let increment = map.get("@increment").and_then(Value::as_f64).unwrap_or(0.0);
    Some(base + increment)
}

/// Plain value used to filter and sort a field: timestamps become microseconds, references
/// their path and counters their value. Other values are returned unchanged.
pub fn comparable(value: &Value) -> Value {
    if let Some(micros) = timestamp_micros(value) {
        return Value::from(micros);
    }
    if let Some(path) = ref_path(value) {
        return Value::String(path.to_owned());
    }
    if let Some(count) = counter_value(value) {
        return if count.fract() == 0.0 && count.abs() < i64::MAX as f64 {
            Value::from(count as i64)
        } else {
            Value::from(count)
        };
    }
    value.clone()
}

/// `row` with every field passed through [`comparable`].
pub fn comparable_row(row: &Row) -> Row {
    row.iter()
        .map(|(key, value)| (key.clone(), comparable(value)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_masamune_field_values() {
        let ts = json!({"@type": "ModelTimestamp", "@time": 1_700_000_000_123_456_i64, "@source": "server"});
        assert_eq!(timestamp_micros(&ts), Some(1_700_000_000_123_456));
        assert_eq!(
            timestamp(&ts),
            Some(UNIX_EPOCH + Duration::from_micros(1_700_000_000_123_456))
        );
        let firestore = json!({"_seconds": 10, "_nanoseconds": 5_000});
        assert_eq!(timestamp_micros(&firestore), Some(10_000_005));
        assert_eq!(timestamp_micros(&json!(5)), None);

        let user = json!({"@type": "ModelRefBase", "@ref": "user_meta/abc"});
        assert_eq!(ref_path(&user), Some("user_meta/abc"));
        assert_eq!(ref_id(&user), Some("abc"));
        assert_eq!(model_ref("/user_meta/abc/"), user);

        let counter = json!({"@type": "ModelCounter", "@value": 3, "@increment": 2});
        assert_eq!(counter_value(&counter), Some(5.0));
        assert_eq!(comparable(&counter), json!(5));
        assert_eq!(comparable(&ts), json!(1_700_000_000_123_456_i64));
        assert_eq!(comparable(&user), json!("user_meta/abc"));
        assert_eq!(comparable(&json!("x")), json!("x"));
    }

    #[test]
    fn encodes_timestamps_like_dart() {
        let time = UNIX_EPOCH + Duration::from_micros(42);
        assert_eq!(
            model_timestamp(time),
            json!({"@type": "ModelTimestamp", "@time": 42, "@now": false, "@source": "user"})
        );
        let now = model_timestamp_now();
        assert_eq!(now["@now"], true);
        assert!(timestamp(&now).is_some());
        let before = UNIX_EPOCH - Duration::from_micros(7);
        assert_eq!(micros_since_epoch(before), -7);
        assert_eq!(timestamp(&model_timestamp(before)), Some(before));
    }
}
