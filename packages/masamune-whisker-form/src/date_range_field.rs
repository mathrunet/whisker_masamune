//! Date range picked with the OS date picker (katana_form's `FormDateTimeRangeField`).
//!
//! Whisker has no native range picker, so the field opens the OS date picker twice
//! (`masamune-whisker-date-picker`): first for the start date, then for the end date, whose
//! earliest selectable day is the picked start. Cancelling either one keeps the value.

use chrono::{Datelike, Local, NaiveDate};
use masamune_whisker_date_picker::pick_date;
use masamune_whisker_ui::Slot;
use whisker::prelude::*;
use whisker_icons::lucide;

use crate::controller::{FormField, bind_field, check};
use crate::date_time_field::date_millis;
use crate::frame::{FormFieldFrame, PickedValue};
use crate::style::{FormStyle, resolve};

/// A range of days, both ends included (katana_form's `DateTimeRange`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FormDateRange {
    /// First day.
    pub start: NaiveDate,
    /// Last day (not before `start`).
    pub end: NaiveDate,
}

impl FormDateRange {
    /// The range between two days in either order.
    pub fn new(a: NaiveDate, b: NaiveDate) -> Self {
        Self {
            start: a.min(b),
            end: a.max(b),
        }
    }

    /// Number of days in the range, both ends included.
    pub fn days(&self) -> i64 {
        (self.end - self.start).num_days() + 1
    }
}

/// Text of a range: both ends in `format`, joined by `separator`.
pub(crate) fn format_range(range: FormDateRange, format: &str, separator: &str) -> String {
    format!(
        "{}{separator}{}",
        range.start.format(format),
        range.end.format(format)
    )
}

/// Date range field that opens the OS date picker for the start and then the end
/// (katana_form's `FormDateTimeRangeField`). See the module documentation.
#[component]
pub fn form_date_time_range_field(
    /// The form this field belongs to and its `on_saved` (see [`crate::FormController::field`]).
    form: Option<FormField<Option<FormDateRange>>>,
    /// Two-way bound value. Without it the field keeps its own value, starting at
    /// `initial_value`.
    value: Option<RwSignal<Option<FormDateRange>>>,
    /// Initial value, also restored by `FormController::reset`.
    #[prop(default = None)]
    initial_value: Option<FormDateRange>,
    /// Earliest selectable date.
    #[prop(default = None)]
    start_date: Option<NaiveDate>,
    /// Latest selectable date.
    #[prop(default = None)]
    end_date: Option<NaiveDate>,
    /// Range the pickers start at while the field is empty (default today to today).
    #[prop(default = None)]
    default_range: Option<FormDateRange>,
    /// `chrono` format of each end.
    #[prop(default = "%Y/%m/%d".to_owned())]
    format: String,
    /// Text between the two ends.
    #[prop(default = " - ".to_owned())]
    separator: String,
    /// Label on the frame (katana_form's `labelText`).
    label: Option<Signal<String>>,
    /// Text shown while empty (katana_form's `hintText`).
    hint: Option<Signal<String>>,
    /// Error shown when empty while validating.
    empty_error_text: Option<Signal<String>>,
    /// Returns an error message for an invalid range, or `None`.
    validator: Option<Callback<Option<FormDateRange>, Option<String>>>,
    /// Called when a range is picked.
    on_changed: Option<Callback<Option<FormDateRange>>>,
    /// `false` disables the field.
    #[prop(default = Signal::from(true))]
    enabled: Signal<bool>,
    /// Trailing icon; `None` hides it.
    #[prop(default = Some(lucide::CalendarRange))]
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
        move |v: &Option<FormDateRange>| check(v, Option::is_none, empty_error_text, validator),
        error,
        move || value.set(reset_value),
    );
    let texts = StoredValue::new((format.clone(), separator.clone()));
    let shown = computed(move || {
        let (format, separator) = texts.get();
        value
            .get()
            .map(|range| format_range(range, &format, &separator))
            .unwrap_or_default()
    });
    let picking = signal(false);
    let on_open = Callback::new(move |()| {
        if picking.get_untracked() {
            return;
        }
        picking.set(true);
        let today = Local::now().date_naive();
        let current = value
            .get_untracked()
            .or(default_range)
            .unwrap_or(FormDateRange::new(today, today));
        spawn_local(async move {
            let picked = pick_range(current, start_date, end_date).await;
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

/// Picks the start and then the end. `None` when either picker is cancelled or fails.
async fn pick_range(
    current: FormDateRange,
    min: Option<NaiveDate>,
    max: Option<NaiveDate>,
) -> Option<FormDateRange> {
    let start = pick_one(current.start, min, max).await?;
    let end = pick_one(current.end.max(start), Some(start), max).await?;
    Some(FormDateRange::new(start, end))
}

async fn pick_one(
    initial: NaiveDate,
    min: Option<NaiveDate>,
    max: Option<NaiveDate>,
) -> Option<NaiveDate> {
    let picked = pick_date(
        (initial.year(), initial.month(), initial.day()),
        min.map(date_millis),
        max.map(date_millis),
    )
    .await
    .map_err(|error| log::warn!("pick_date failed: {error:?}"))
    .ok()??;
    NaiveDate::from_ymd_opt(picked.year, picked.month, picked.day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_are_ordered_and_formatted() {
        let a = NaiveDate::from_ymd_opt(2026, 1, 10).unwrap();
        let b = NaiveDate::from_ymd_opt(2026, 1, 3).unwrap();
        let range = FormDateRange::new(a, b);
        assert_eq!(range.start, b);
        assert_eq!(range.days(), 8);
        assert_eq!(format_range(range, "%m/%d", " - "), "01/03 - 01/10");
    }
}
