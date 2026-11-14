//! Date and time picked with the OS pickers (katana_form's `FormDateTimeField`).

use chrono::{Datelike, Local, NaiveDate, NaiveDateTime, NaiveTime, Timelike};
use masamune_whisker_date_picker::{pick_date, pick_time};
use masamune_whisker_ui::Slot;
use whisker::prelude::*;
use whisker_icons::lucide;

use crate::controller::{FormField, bind_field, check};
use crate::frame::{FormFieldFrame, PickedValue};
use crate::style::{FormStyle, resolve};

/// Which OS pickers [`FormDateTimeField`] opens (katana_form's
/// `FormDateTimeFieldDateTimePicker` / `DatePicker` / `TimePicker`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FormDateTimePickerMode {
    /// The date picker, then the time picker.
    #[default]
    DateTime,
    /// The date picker only (the time stays as it was, or midnight).
    Date,
    /// The time picker only (the date stays as it was, or today).
    Time,
}

impl FormDateTimePickerMode {
    /// The default `chrono` format of the shown value.
    pub fn default_format(self) -> &'static str {
        match self {
            Self::DateTime => "%Y/%m/%d %H:%M",
            Self::Date => "%Y/%m/%d",
            Self::Time => "%H:%M",
        }
    }
}

/// UTC milliseconds of the start of `date` (the bounds of the native date picker).
pub(crate) fn date_millis(date: NaiveDate) -> i64 {
    date.and_time(NaiveTime::MIN).and_utc().timestamp_millis()
}

/// Date and time field that opens the OS date and / or time pickers
/// (`masamune-whisker-date-picker`; katana_form's `FormDateTimeField`).
///
/// The pickers start at the current value, or at `default_date_time` (now) while empty.
/// Cancelling either picker keeps the value. The value is shown with `format` (`chrono`
/// syntax), which defaults to [`FormDateTimePickerMode::default_format`].
#[component]
pub fn form_date_time_field(
    /// The form this field belongs to and its `on_saved` (see [`crate::FormController::field`]).
    form: Option<FormField<Option<NaiveDateTime>>>,
    /// Two-way bound value. Without it the field keeps its own value, starting at
    /// `initial_value`.
    value: Option<RwSignal<Option<NaiveDateTime>>>,
    /// Initial value, also restored by `FormController::reset`.
    #[prop(default = None)]
    initial_value: Option<NaiveDateTime>,
    /// Which pickers open.
    #[prop(default = FormDateTimePickerMode::DateTime)]
    mode: FormDateTimePickerMode,
    /// Earliest selectable date.
    #[prop(default = None)]
    start_date: Option<NaiveDate>,
    /// Latest selectable date.
    #[prop(default = None)]
    end_date: Option<NaiveDate>,
    /// Value the pickers start at while the field is empty (default now).
    #[prop(default = None)]
    default_date_time: Option<NaiveDateTime>,
    /// `chrono` format of the shown value; defaults to the mode's format.
    #[prop(default = None)]
    format: Option<String>,
    /// Label on the frame (katana_form's `labelText`).
    label: Option<Signal<String>>,
    /// Text shown while empty (katana_form's `hintText`).
    hint: Option<Signal<String>>,
    /// Error shown when empty while validating.
    empty_error_text: Option<Signal<String>>,
    /// Returns an error message for an invalid value, or `None`.
    validator: Option<Callback<Option<NaiveDateTime>, Option<String>>>,
    /// Called when a value is picked.
    on_changed: Option<Callback<Option<NaiveDateTime>>>,
    /// `false` disables the field.
    #[prop(default = Signal::from(true))]
    enabled: Signal<bool>,
    /// Trailing icon; `None` hides it.
    #[prop(default = Some(lucide::Calendar))]
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
        move |v: &Option<NaiveDateTime>| check(v, Option::is_none, empty_error_text, validator),
        error,
        move || value.set(reset_value),
    );
    let format = StoredValue::new(
        format
            .clone()
            .unwrap_or_else(|| mode.default_format().to_owned()),
    );
    let shown = computed(move || {
        value
            .get()
            .map(|v| v.format(&format.get()).to_string())
            .unwrap_or_default()
    });
    let picking = signal(false);
    let on_open = Callback::new(move |()| {
        if picking.get_untracked() {
            return;
        }
        picking.set(true);
        let current = value.get_untracked();
        let start_at = current
            .or(default_date_time)
            .unwrap_or_else(|| Local::now().naive_local());
        spawn_local(async move {
            let picked = pick(mode, current, start_at, start_date, end_date).await;
            let _ = picking.try_set(false);
            if let Some(picked) = picked
                && value.try_set(Some(picked))
                && let Some(on_changed) = on_changed
            {
                on_changed.run(Some(picked));
            }
        });
    });
    let disabled = computed(move || !enabled.get());
    render! {
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
    }
}

/// Runs the pickers of `mode`, starting at `start_at`. The part a mode does not pick comes
/// from `current` (midnight / today while empty). `None` when a picker is cancelled or fails.
async fn pick(
    mode: FormDateTimePickerMode,
    current: Option<NaiveDateTime>,
    start_at: NaiveDateTime,
    start: Option<NaiveDate>,
    end: Option<NaiveDate>,
) -> Option<NaiveDateTime> {
    let mut date = match mode {
        FormDateTimePickerMode::Time => current
            .map(|c| c.date())
            .unwrap_or_else(|| Local::now().date_naive()),
        _ => start_at.date(),
    };
    if mode != FormDateTimePickerMode::Time {
        let picked = pick_date(
            (date.year(), date.month(), date.day()),
            start.map(date_millis),
            end.map(date_millis),
        )
        .await
        .map_err(|error| log::warn!("pick_date failed: {error:?}"))
        .ok()??;
        date = NaiveDate::from_ymd_opt(picked.year, picked.month, picked.day)?;
    }
    let mut time = match mode {
        FormDateTimePickerMode::Date => current.map(|c| c.time()).unwrap_or(NaiveTime::MIN),
        _ => start_at.time(),
    };
    if mode != FormDateTimePickerMode::Date {
        let picked = pick_time(time.hour(), time.minute())
            .await
            .map_err(|error| log::warn!("pick_time failed: {error:?}"))
            .ok()??;
        time = NaiveTime::from_hms_opt(picked.hour, picked.minute, 0)?;
    }
    Some(date.and_time(time))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_bounds_are_utc_midnight() {
        let date = NaiveDate::from_ymd_opt(1970, 1, 2).unwrap();
        assert_eq!(date_millis(date), 86_400_000);
        assert_eq!(FormDateTimePickerMode::Date.default_format(), "%Y/%m/%d");
    }
}
