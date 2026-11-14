//! Date picked with year / month / day columns in a bottom sheet (katana_form's
//! `FormDateField`, extended with the year).

use chrono::{Datelike, Local, NaiveDate};
use masamune_whisker_ui::Slot;
use whisker::prelude::*;
use whisker_icons::lucide;

use crate::columns::{ColumnSheet, PickerColumn};
use crate::controller::{FormField, bind_field, check};
use crate::frame::{FormFieldFrame, PickedValue};
use crate::style::{FormStyle, resolve};

/// Days in `month` (1-12) of `year`.
pub(crate) fn days_in_month(year: i32, month: u32) -> u32 {
    let (next_year, next_month) = if month >= 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    NaiveDate::from_ymd_opt(next_year, next_month, 1)
        .and_then(|d| d.pred_opt())
        .map(|d| d.day())
        .unwrap_or(31)
}

/// The date of `year` / `month` / `day`, with the day kept inside the month and the date kept
/// between `start` and `end`.
pub(crate) fn clamp_date(
    year: i32,
    month: u32,
    day: u32,
    start: NaiveDate,
    end: NaiveDate,
) -> NaiveDate {
    let month = month.clamp(1, 12);
    let day = day.clamp(1, days_in_month(year, month));
    let date = NaiveDate::from_ymd_opt(year, month, day).unwrap_or(end);
    date.clamp(start.min(end), end)
}

/// Date field that opens a sheet with year, month and day columns (katana_form's
/// `FormDateField`, with years; the Flutter app's birthday field).
///
/// The sheet starts at the current value, or at `default_date` while empty, and keeps the
/// picked date between `start_date` and `end_date`. The value is shown with `format`
/// (`chrono` syntax).
#[component]
pub fn form_date_field(
    /// The form this field belongs to and its `on_saved` (see [`crate::FormController::field`]).
    form: Option<FormField<Option<NaiveDate>>>,
    /// Two-way bound date. Without it the field keeps its own date, starting at
    /// `initial_value`.
    value: Option<RwSignal<Option<NaiveDate>>>,
    /// Initial date, also restored by `FormController::reset`.
    #[prop(default = None)]
    initial_value: Option<NaiveDate>,
    /// Earliest date (default 1900-01-01).
    #[prop(default = None)]
    start_date: Option<NaiveDate>,
    /// Latest date (default today).
    #[prop(default = None)]
    end_date: Option<NaiveDate>,
    /// Date the sheet starts at while the field is empty (default 2000-01-01).
    #[prop(default = None)]
    default_date: Option<NaiveDate>,
    /// `chrono` format of the shown value.
    #[prop(default = "%Y/%m/%d".to_owned())]
    format: String,
    /// Text after each year in the sheet.
    #[prop(default = String::new())]
    year_suffix: String,
    /// Text after each month in the sheet.
    #[prop(default = String::new())]
    month_suffix: String,
    /// Text after each day in the sheet.
    #[prop(default = String::new())]
    day_suffix: String,
    /// Label of the button that applies the picked date.
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
    /// Returns an error message for an invalid date, or `None`.
    validator: Option<Callback<Option<NaiveDate>, Option<String>>>,
    /// Called when a date is applied.
    on_changed: Option<Callback<Option<NaiveDate>>>,
    /// `false` disables the field.
    #[prop(default = Signal::from(true))]
    enabled: Signal<bool>,
    /// Trailing icon; `None` hides it.
    #[prop(default = Some(lucide::Calendar))]
    icon: Option<&'static str>,
    /// Shown before the date.
    #[prop(default = Slot::default())]
    prefix: Slot,
    /// Shown after the date.
    #[prop(default = Slot::default())]
    suffix: Slot,
    /// Look of the field; defaults to the provided [`FormStyle`].
    style: Option<FormStyle>,
) -> Element {
    let style = resolve(style);
    let (prefix, suffix) = (prefix.clone(), suffix.clone());
    let today = Local::now().date_naive();
    let end = end_date.unwrap_or(today);
    let start = start_date.unwrap_or(NaiveDate::from_ymd_opt(1900, 1, 1).unwrap_or(end));
    let fallback = default_date
        .or(NaiveDate::from_ymd_opt(2000, 1, 1))
        .unwrap_or(end)
        .clamp(start.min(end), end);
    let reset_value = match value {
        Some(value) if initial_value.is_none() => value.get_untracked(),
        _ => initial_value,
    };
    let value = value.unwrap_or_else(|| signal(initial_value));
    let error = signal(None::<String>);
    bind_field(
        form,
        move || value.get_untracked(),
        move |v: &Option<NaiveDate>| check(v, Option::is_none, empty_error_text, validator),
        error,
        move || value.set(reset_value),
    );
    let open = signal(false);
    let year = signal(fallback.year() as u32);
    let month = signal(fallback.month());
    let day = signal(fallback.day());
    let format = StoredValue::new(format.clone());
    let shown = computed(move || {
        value
            .get()
            .map(|date| date.format(&format.get()).to_string())
            .unwrap_or_default()
    });
    let on_open = Callback::new(move |()| {
        let date = value.get_untracked().unwrap_or(fallback);
        year.set(date.year() as u32);
        month.set(date.month());
        day.set(date.day());
        open.set(true);
    });
    let on_submit = Callback::new(move |()| {
        let date = clamp_date(
            year.get_untracked() as i32,
            month.get_untracked(),
            day.get_untracked(),
            start,
            end,
        );
        value.set(Some(date));
        if let Some(on_changed) = on_changed {
            on_changed.run(Some(date));
        }
    });
    let years = (start.year().max(0) as u32..=end.year().max(0) as u32)
        .rev()
        .collect::<Vec<u32>>();
    let years = StoredValue::new(years);
    let picked = computed(move || {
        clamp_date(year.get() as i32, month.get(), day.get(), start, end)
            .format(&format.get())
            .to_string()
    });
    let disabled = computed(move || !enabled.get());
    let suffixes = StoredValue::new((
        year_suffix.clone(),
        month_suffix.clone(),
        day_suffix.clone(),
    ));
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
                PickerColumn(
                    values: years.get(),
                    selected: year,
                    suffix: suffixes.get().0,
                    style: style,
                )
                PickerColumn(
                    values: (1..=12).collect::<Vec<u32>>(),
                    selected: month,
                    suffix: suffixes.get().1,
                    style: style,
                )
                PickerColumn(
                    values: (1..=31).collect::<Vec<u32>>(),
                    selected: day,
                    suffix: suffixes.get().2,
                    style: style,
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamps_dates() {
        let start = NaiveDate::from_ymd_opt(1900, 1, 1).unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 11, 11).unwrap();
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(2025, 12), 31);
        assert_eq!(
            clamp_date(2023, 2, 31, start, today),
            NaiveDate::from_ymd_opt(2023, 2, 28).unwrap()
        );
        assert_eq!(clamp_date(2026, 12, 1, start, today), today);
        assert_eq!(clamp_date(1800, 1, 1, start, today), start);
    }
}
