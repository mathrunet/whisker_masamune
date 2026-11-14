//! Year and month picked with two columns in a bottom sheet (katana_form's
//! `FormMonthModalField`).

use chrono::{Datelike, Local, NaiveDate};
use masamune_whisker_ui::Slot;
use whisker::prelude::*;
use whisker_icons::lucide;

use crate::columns::{ColumnSheet, PickerColumn};
use crate::controller::{FormField, bind_field, check};
use crate::date_field::days_in_month;
use crate::frame::{FormFieldFrame, PickedValue};
use crate::style::{FormStyle, resolve};

/// Which day of the picked month the value has (katana_form's `FormMonthModalFieldPicker(day:,
/// lastDayOfMonth:)`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FormMonthDay {
    /// The 1st.
    #[default]
    First,
    /// This day, or the last day of shorter months.
    Day(u32),
    /// The last day of the month.
    Last,
}

/// The first day of the month of `date`.
pub(crate) fn month_start(date: NaiveDate) -> NaiveDate {
    date.with_day(1).unwrap_or(date)
}

/// The years from `start` to `end`, newest first.
pub(crate) fn month_years(start: NaiveDate, end: NaiveDate) -> Vec<u32> {
    let (from, to) = (start.year().max(0) as u32, end.year().max(0) as u32);
    (from.min(to)..=to).rev().collect()
}

/// The month `year` / `month` kept between the months of `start` and `end`, on the day `day`.
pub(crate) fn clamp_month(
    year: i32,
    month: u32,
    day: FormMonthDay,
    start: NaiveDate,
    end: NaiveDate,
) -> NaiveDate {
    let (start, end) = (month_start(start.min(end)), month_start(end));
    let first = NaiveDate::from_ymd_opt(year, month.clamp(1, 12), 1)
        .unwrap_or(end)
        .clamp(start, end);
    let last = days_in_month(first.year(), first.month());
    let day = match day {
        FormMonthDay::First => 1,
        FormMonthDay::Day(day) => day.clamp(1, last),
        FormMonthDay::Last => last,
    };
    first.with_day(day).unwrap_or(first)
}

/// Month field that opens a sheet with year and month columns (katana_form's
/// `FormMonthModalField`).
///
/// The value is a date in the picked month on `day` (the 1st by default). The sheet starts at
/// the current value, or at `default_date` (this month) while empty, and keeps the month between
/// `start_date` (default 1900-01) and `end_date` (default this month).
#[component]
pub fn form_month_modal_field(
    /// The form this field belongs to and its `on_saved` (see [`crate::FormController::field`]).
    form: Option<FormField<Option<NaiveDate>>>,
    /// Two-way bound value. Without it the field keeps its own value, starting at
    /// `initial_value`.
    value: Option<RwSignal<Option<NaiveDate>>>,
    /// Initial value, also restored by `FormController::reset`.
    #[prop(default = None)]
    initial_value: Option<NaiveDate>,
    /// Earliest month (default 1900-01).
    #[prop(default = None)]
    start_date: Option<NaiveDate>,
    /// Latest month (default this month).
    #[prop(default = None)]
    end_date: Option<NaiveDate>,
    /// Month the sheet starts at while the field is empty (default this month).
    #[prop(default = None)]
    default_date: Option<NaiveDate>,
    /// Day of the month of the value.
    #[prop(default = FormMonthDay::First)]
    day: FormMonthDay,
    /// `chrono` format of the shown value.
    #[prop(default = "%Y/%m".to_owned())]
    format: String,
    /// Text after each year in the sheet.
    #[prop(default = String::new())]
    year_suffix: String,
    /// Text after each month in the sheet.
    #[prop(default = String::new())]
    month_suffix: String,
    /// Label of the button that applies the picked month.
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
    validator: Option<Callback<Option<NaiveDate>, Option<String>>>,
    /// Called when a month is applied.
    on_changed: Option<Callback<Option<NaiveDate>>>,
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
    let today = Local::now().date_naive();
    let end = end_date.unwrap_or(today);
    let start = start_date.unwrap_or(NaiveDate::from_ymd_opt(1900, 1, 1).unwrap_or(end));
    let fallback = clamp_month(
        default_date.unwrap_or(today).year(),
        default_date.unwrap_or(today).month(),
        day,
        start,
        end,
    );
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
        open.set(true);
    });
    let on_submit = Callback::new(move |()| {
        let date = clamp_month(
            year.get_untracked() as i32,
            month.get_untracked(),
            day,
            start,
            end,
        );
        value.set(Some(date));
        if let Some(on_changed) = on_changed {
            on_changed.run(Some(date));
        }
    });
    let years = StoredValue::new(month_years(start, end));
    let picked = computed(move || {
        clamp_month(year.get() as i32, month.get(), day, start, end)
            .format(&format.get())
            .to_string()
    });
    let disabled = computed(move || !enabled.get());
    let suffixes = StoredValue::new((year_suffix.clone(), month_suffix.clone()));
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
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn years_run_newest_first() {
        assert_eq!(
            month_years(date(2020, 5, 3), date(2023, 1, 1)),
            vec![2023, 2022, 2021, 2020]
        );
        assert_eq!(
            month_years(date(2024, 1, 1), date(2024, 12, 31)),
            vec![2024]
        );
    }

    #[test]
    fn months_stay_in_range_on_the_chosen_day() {
        let (start, end) = (date(2020, 3, 15), date(2024, 2, 10));
        assert_eq!(
            clamp_month(2022, 7, FormMonthDay::First, start, end),
            date(2022, 7, 1)
        );
        assert_eq!(
            clamp_month(2019, 7, FormMonthDay::First, start, end),
            date(2020, 3, 1)
        );
        assert_eq!(
            clamp_month(2024, 6, FormMonthDay::First, start, end),
            date(2024, 2, 1)
        );
        assert_eq!(
            clamp_month(2024, 2, FormMonthDay::Last, start, end),
            date(2024, 2, 29)
        );
        assert_eq!(
            clamp_month(2023, 4, FormMonthDay::Day(31), start, end),
            date(2023, 4, 30)
        );
    }
}
