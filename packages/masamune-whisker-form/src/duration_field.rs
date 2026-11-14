//! Duration picked with day / hour / minute / second columns in a bottom sheet (katana_form's
//! `FormDurationField`).

use std::time::Duration;

use masamune_whisker_ui::Slot;
use whisker::prelude::*;
use whisker_icons::lucide;

use crate::columns::{ColumnSheet, PickerColumn};
use crate::controller::{FormField, bind_field, check};
use crate::frame::{FormFieldFrame, PickedValue};
use crate::style::{FormStyle, resolve};

const MINUTE: u64 = 60;
const HOUR: u64 = 60 * MINUTE;
const DAY: u64 = 24 * HOUR;

/// `(days, hours, minutes, seconds)` of `duration`. Without `with_days` the hours are not
/// wrapped at 24.
pub(crate) fn duration_parts(duration: Duration, with_days: bool) -> (u32, u32, u32, u32) {
    let secs = duration.as_secs();
    let (days, rest) = if with_days {
        (secs / DAY, secs % DAY)
    } else {
        (0, secs)
    };
    (
        days as u32,
        (rest / HOUR) as u32,
        (rest % HOUR / MINUTE) as u32,
        (rest % MINUTE) as u32,
    )
}

/// The duration of the parts, kept between `begin` and `end`.
pub(crate) fn duration_from_parts(
    days: u32,
    hours: u32,
    minutes: u32,
    seconds: u32,
    begin: Duration,
    end: Duration,
) -> Duration {
    let secs = days as u64 * DAY + hours as u64 * HOUR + minutes as u64 * MINUTE + seconds as u64;
    Duration::from_secs(secs).clamp(begin.min(end), end)
}

/// The values of each column: `(days, hours)` for the range up to `end` (minutes and seconds
/// are always 0-59).
pub(crate) fn duration_columns(end: Duration, with_days: bool) -> (Vec<u32>, Vec<u32>) {
    let (days, hours, _, _) = duration_parts(end, with_days);
    if with_days {
        ((0..=days).collect(), (0..=23).collect())
    } else {
        (Vec::new(), (0..=hours).collect())
    }
}

/// Default text of a duration: `1d 02:03:04`, `26:03:04`, or without seconds `26:03`.
pub(crate) fn format_duration(duration: Duration, with_days: bool, with_seconds: bool) -> String {
    let (days, hours, minutes, seconds) = duration_parts(duration, with_days);
    let time = if with_seconds {
        format!("{hours:02}:{minutes:02}:{seconds:02}")
    } else {
        format!("{hours:02}:{minutes:02}")
    };
    if with_days {
        format!("{days}d {time}")
    } else {
        time
    }
}

/// Duration field that opens a sheet with day, hour, minute and second columns (katana_form's
/// `FormDurationField`).
///
/// `show_days` adds the day column (hours then wrap at 24) and `show_seconds` the second
/// column. The picked duration is kept between `begin` and `end` (default 0 to 23:59:59).
#[component]
pub fn form_duration_field(
    /// The form this field belongs to and its `on_saved` (see [`crate::FormController::field`]).
    form: Option<FormField<Option<Duration>>>,
    /// Two-way bound value. Without it the field keeps its own value, starting at
    /// `initial_value`.
    value: Option<RwSignal<Option<Duration>>>,
    /// Initial value, also restored by `FormController::reset`.
    #[prop(default = None)]
    initial_value: Option<Duration>,
    /// Shortest duration.
    #[prop(default = Duration::ZERO)]
    begin: Duration,
    /// Longest duration.
    #[prop(default = Duration::from_secs(DAY - 1))]
    end: Duration,
    /// Duration the sheet starts at while the field is empty (default `begin`).
    #[prop(default = None)]
    default_duration: Option<Duration>,
    /// Shows the day column.
    #[prop(default = false)]
    show_days: bool,
    /// Shows the second column.
    #[prop(default = true)]
    show_seconds: bool,
    /// Text of a duration; defaults to `HH:MM:SS` (with `{days}d` before it when `show_days`).
    format: Option<Callback<Duration, String>>,
    /// Text after each day in the sheet.
    #[prop(default = String::new())]
    day_suffix: String,
    /// Text after each hour in the sheet.
    #[prop(default = String::new())]
    hour_suffix: String,
    /// Text after each minute in the sheet.
    #[prop(default = String::new())]
    minute_suffix: String,
    /// Text after each second in the sheet.
    #[prop(default = String::new())]
    second_suffix: String,
    /// Label of the button that applies the picked duration.
    #[prop(default = Signal::from("Confirm"))]
    confirm_text: Signal<String>,
    /// Label of the button that closes the sheet.
    #[prop(default = Signal::from("Cancel"))]
    cancel_text: Signal<String>,
    /// Label on the frame (katana_form's `labelText`).
    label: Option<Signal<String>>,
    /// Text shown while empty (katana_form's `hintText`).
    hint: Option<Signal<String>>,
    /// Error shown when empty while validating.
    empty_error_text: Option<Signal<String>>,
    /// Returns an error message for an invalid value, or `None`.
    validator: Option<Callback<Option<Duration>, Option<String>>>,
    /// Called when a duration is applied.
    on_changed: Option<Callback<Option<Duration>>>,
    /// `false` disables the field.
    #[prop(default = Signal::from(true))]
    enabled: Signal<bool>,
    /// Trailing icon; `None` hides it.
    #[prop(default = Some(lucide::Timer))]
    icon: Option<&'static str>,
    /// Shown before the value.
    #[prop(default = Slot::default())]
    prefix: Slot,
    /// Shown after the value.
    #[prop(default = Slot::default())]
    suffix: Slot,
    /// Look of the field; defaults to the provided [`FormStyle`].
    style: Option<FormStyle>,
) -> Element {
    let style = resolve(style);
    let (prefix, suffix) = (prefix.clone(), suffix.clone());
    let reset_value = match value {
        Some(value) if initial_value.is_none() => value.get_untracked(),
        _ => initial_value,
    };
    let value = value.unwrap_or_else(|| signal(initial_value));
    let error = signal(None::<String>);
    bind_field(
        form,
        move || value.get_untracked(),
        move |v: &Option<Duration>| check(v, Option::is_none, empty_error_text, validator),
        error,
        move || value.set(reset_value),
    );
    let text_of = move |duration: Duration| match format {
        Some(format) => format.run(duration),
        None => format_duration(duration, show_days, show_seconds),
    };
    let open = signal(false);
    let parts = [signal(0u32), signal(0u32), signal(0u32), signal(0u32)];
    let [days, hours, minutes, seconds] = parts;
    let picked_duration = move || {
        duration_from_parts(
            days.get(),
            hours.get(),
            minutes.get(),
            if show_seconds { seconds.get() } else { 0 },
            begin,
            end,
        )
    };
    let shown = computed(move || value.get().map(text_of).unwrap_or_default());
    let picked = computed(move || text_of(picked_duration()));
    let on_open = Callback::new(move |()| {
        let start = value.get_untracked().or(default_duration).unwrap_or(begin);
        let (d, h, m, s) = duration_parts(start, show_days);
        days.set(d);
        hours.set(h);
        minutes.set(m);
        seconds.set(s);
        open.set(true);
    });
    let on_submit = Callback::new(move |()| {
        let duration = untrack_duration(days, hours, minutes, seconds, show_seconds, begin, end);
        value.set(Some(duration));
        if let Some(on_changed) = on_changed {
            on_changed.run(Some(duration));
        }
    });
    let (day_values, hour_values) = duration_columns(end, show_days);
    let columns = StoredValue::new((day_values, hour_values));
    let suffixes = StoredValue::new((
        day_suffix.clone(),
        hour_suffix.clone(),
        minute_suffix.clone(),
        second_suffix.clone(),
    ));
    let disabled = computed(move || !enabled.get());
    render! {
        Fragment {
            FormFieldFrame(
                label_option: label,
                error: error.read_only(),
                disabled: disabled,
                prefix: prefix.clone(),
                suffix: suffix.clone(),
                style: style,
                on_tap: on_open,
            ) {
                PickedValue(
                    value: shown,
                    hint: hint.unwrap_or_default(),
                    icon_option: icon,
                    disabled: disabled,
                    style: style,
                )
            }
            ColumnSheet(
                open: open,
                picked: picked,
                confirm_text: confirm_text,
                cancel_text: cancel_text,
                on_submit: on_submit,
            ) {
                Show(when: move || show_days) {
                    PickerColumn(
                        values: columns.get().0,
                        selected: days,
                        suffix: suffixes.get().0,
                        style: style,
                    )
                }
                PickerColumn(
                    values: columns.get().1,
                    selected: hours,
                    suffix: suffixes.get().1,
                    two_digits: true,
                    style: style,
                )
                PickerColumn(
                    values: (0..=59).collect::<Vec<u32>>(),
                    selected: minutes,
                    suffix: suffixes.get().2,
                    two_digits: true,
                    style: style,
                )
                Show(when: move || show_seconds) {
                    PickerColumn(
                        values: (0..=59).collect::<Vec<u32>>(),
                        selected: seconds,
                        suffix: suffixes.get().3,
                        two_digits: true,
                        style: style,
                    )
                }
            }
        }
    }
}

/// The picked duration, read without tracking.
fn untrack_duration(
    days: RwSignal<u32>,
    hours: RwSignal<u32>,
    minutes: RwSignal<u32>,
    seconds: RwSignal<u32>,
    with_seconds: bool,
    begin: Duration,
    end: Duration,
) -> Duration {
    duration_from_parts(
        days.get_untracked(),
        hours.get_untracked(),
        minutes.get_untracked(),
        if with_seconds {
            seconds.get_untracked()
        } else {
            0
        },
        begin,
        end,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parts_round_trip() {
        let d = Duration::from_secs(DAY + 2 * HOUR + 3 * MINUTE + 4);
        assert_eq!(duration_parts(d, true), (1, 2, 3, 4));
        assert_eq!(duration_parts(d, false), (0, 26, 3, 4));
        let max = Duration::from_secs(10 * DAY);
        assert_eq!(duration_from_parts(1, 2, 3, 4, Duration::ZERO, max), d);
    }

    #[test]
    fn durations_are_clamped() {
        let (begin, end) = (Duration::from_secs(MINUTE), Duration::from_secs(HOUR));
        assert_eq!(duration_from_parts(0, 0, 0, 10, begin, end), begin);
        assert_eq!(duration_from_parts(0, 5, 0, 0, begin, end), end);
    }

    #[test]
    fn columns_follow_the_range() {
        let (days, hours) = duration_columns(Duration::from_secs(3 * DAY), true);
        assert_eq!(days, vec![0, 1, 2, 3]);
        assert_eq!(hours.len(), 24);
        let (days, hours) = duration_columns(Duration::from_secs(DAY - 1), false);
        assert!(days.is_empty());
        assert_eq!(hours, (0..=23).collect::<Vec<u32>>());
        let (_, hours) = duration_columns(Duration::from_secs(5 * HOUR), false);
        assert_eq!(hours, (0..=5).collect::<Vec<u32>>());
        let (_, hours) = duration_columns(Duration::from_secs(30 * HOUR), false);
        assert_eq!(hours, (0..=30).collect::<Vec<u32>>());
    }

    #[test]
    fn formats_durations() {
        let d = Duration::from_secs(DAY + 2 * HOUR + 3 * MINUTE + 4);
        assert_eq!(format_duration(d, true, true), "1d 02:03:04");
        assert_eq!(format_duration(d, false, true), "26:03:04");
        assert_eq!(format_duration(d, false, false), "26:03");
    }
}
