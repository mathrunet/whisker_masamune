//! Material 3 date picker dialog (Flutter's `showDatePicker` / `DatePickerDialog`).
//!
//! Ported from Flutter's `packages/flutter/lib/src/material/date_picker.dart`,
//! `calendar_date_picker.dart` and `input_date_picker_form_field.dart` (the Material 3 portrait
//! layout and the `_DatePickerDefaultsM3` colors). The dialog draws everything in Whisker, so it
//! looks the same on iOS and Android.
//!
//! - Calendar mode: a month grid with previous / next buttons (or a horizontal swipe). Tapping
//!   the "month year" button switches to a grid of years.
//! - Input mode: type the date with the number keyboard; the separators are inserted while
//!   typing.
//!
//! ```ignore
//! let open = signal(false);
//! let date = signal(NaiveDate::from_ymd_opt(2001, 1, 1).unwrap());
//! render! {
//!     DatePickerDialog(
//!         open: open,
//!         initial_date: date,
//!         first_date: NaiveDate::from_ymd_opt(1900, 1, 1).unwrap(),
//!         last_date: NaiveDate::from_ymd_opt(2030, 12, 31).unwrap(),
//!         on_submit: move |picked| date.set(picked),
//!     )
//! }
//! ```

use std::cell::Cell;
use std::rc::Rc;

use chrono::{Datelike, Local, NaiveDate};
use masamune_whisker_appearance::{ColorExt, TextKind, bold, palette, shape, text_with};
use whisker::AnimConfig;
use whisker::AnimationController;
use whisker::ScrollViewHandle;
use whisker::css::{Border, JustifyContent, PositionKind};
use whisker::event::TouchEvent;
use whisker::prelude::*;
use whisker_icons::{Icon, lucide};
use whisker_input::{Input, KeyboardType};
use whisker_keyboard::keyboard_height;

use crate::style::{center, column, css_color, fill, row};

/// Width of the dialog (`_calendarPortraitDialogSizeM3.width`).
const DIALOG_WIDTH: f32 = 328.0;
/// Height of the "month year" row with the previous / next buttons (`_subHeaderHeight`).
const SUB_HEADER_HEIGHT: f32 = 52.0;
/// Height of a row of the day grid (`_dayPickerRowHeight`).
const DAY_ROW_HEIGHT: f32 = 42.0;
/// Diameter of the circle behind a day.
const DAY_CIRCLE: f32 = 40.0;
/// Rows of the day grid: the weekday names and up to six weeks.
const DAY_ROWS: usize = 6;
/// Height of a row of the year grid (`_yearPickerRowHeight`).
const YEAR_ROW_HEIGHT: f32 = 52.0;
/// Years per row of the year grid (`_yearPickerColumnCount`).
const YEAR_COLUMNS: usize = 3;
/// Fade of the dialog (Flutter's `showDialog` transition).
const FADE_MS: u32 = 150;
/// Horizontal movement (px) after which a touch on the day grid turns the month.
const SWIPE_DISTANCE: f32 = 48.0;

/// Which picker the dialog opens with (Flutter's `DatePickerEntryMode`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum DatePickerEntryMode {
    /// Month grid (and the year grid).
    #[default]
    Calendar,
    /// Text field with the number keyboard.
    Input,
}

/// Order of the year, month and day in the input field.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum DateInputOrder {
    /// `yyyy/mm/dd` (ja, zh, ko).
    YearMonthDay,
    /// `mm/dd/yyyy` (en-US).
    #[default]
    MonthDayYear,
    /// `dd/mm/yyyy` (most European locales).
    DayMonthYear,
}

impl DateInputOrder {
    /// Digit count of each part, in order.
    const fn widths(self) -> [usize; 3] {
        match self {
            Self::YearMonthDay => [4, 2, 2],
            Self::MonthDayYear | Self::DayMonthYear => [2, 2, 4],
        }
    }

    /// `(year, month, day)` from the three parts in this order.
    fn arrange(self, parts: [u32; 3]) -> (u32, u32, u32) {
        match self {
            Self::YearMonthDay => (parts[0], parts[1], parts[2]),
            Self::MonthDayYear => (parts[2], parts[0], parts[1]),
            Self::DayMonthYear => (parts[2], parts[1], parts[0]),
        }
    }
}

/// Texts and date formats of the dialog. The defaults are English (US); pass the app's localized
/// strings (Flutter's `MaterialLocalizations`: `datePickerHelpText`, `dateInputLabel`, ...).
///
/// `month_year_format` and `headline_format` replace `{year}`, `{month}`, `{day}`,
/// `{month_name}`, `{month_short}` and `{weekday}` (the short weekday name).
#[derive(Clone, Debug, PartialEq)]
pub struct DatePickerLabels {
    /// Heading in calendar mode ("Select date").
    pub help_calendar: String,
    /// Heading in input mode ("Enter date").
    pub help_input: String,
    /// Label of the input field ("Date").
    pub field_label: String,
    /// Hint in the empty input field ("mm/dd/yyyy").
    pub input_hint: String,
    /// Order of the parts in the input field.
    pub input_order: DateInputOrder,
    /// Separator between the parts in the input field.
    pub input_separator: char,
    /// Error for text that is not a date ("Invalid format.").
    pub invalid_format: String,
    /// Error for a date outside the range ("Out of range.").
    pub out_of_range: String,
    pub cancel: String,
    pub confirm: String,
    /// January to December.
    pub month_names: [String; 12],
    /// Jan to Dec.
    pub month_short_names: [String; 12],
    /// Short weekday names from Sunday, used in the headline.
    pub weekday_short_names: [String; 7],
    /// Narrow weekday names from Sunday, shown above the day grid.
    pub weekday_narrow_names: [String; 7],
    /// First column of the day grid: 0 is Sunday, 1 Monday, ...
    pub first_day_of_week: u32,
    /// Title of the month ("{month_name} {year}").
    pub month_year_format: String,
    /// Selected date in the header ("{weekday}, {month_short} {day}").
    pub headline_format: String,
}

fn strings<const N: usize>(values: [&str; N]) -> [String; N] {
    values.map(str::to_owned)
}

impl Default for DatePickerLabels {
    fn default() -> Self {
        Self {
            help_calendar: "Select date".to_owned(),
            help_input: "Enter date".to_owned(),
            field_label: "Date".to_owned(),
            input_hint: "mm/dd/yyyy".to_owned(),
            input_order: DateInputOrder::MonthDayYear,
            input_separator: '/',
            invalid_format: "Invalid format.".to_owned(),
            out_of_range: "Out of range.".to_owned(),
            cancel: "Cancel".to_owned(),
            confirm: "OK".to_owned(),
            month_names: strings([
                "January",
                "February",
                "March",
                "April",
                "May",
                "June",
                "July",
                "August",
                "September",
                "October",
                "November",
                "December",
            ]),
            month_short_names: strings([
                "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
            ]),
            weekday_short_names: strings(["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"]),
            weekday_narrow_names: strings(["S", "M", "T", "W", "T", "F", "S"]),
            first_day_of_week: 0,
            month_year_format: "{month_name} {year}".to_owned(),
            headline_format: "{weekday}, {month_short} {day}".to_owned(),
        }
    }
}

impl DatePickerLabels {
    /// `template` with the placeholders of `date` replaced.
    pub fn format(&self, template: &str, date: NaiveDate) -> String {
        let month = date.month0() as usize;
        let weekday = date.weekday().num_days_from_sunday() as usize;
        template
            .replace("{year}", &date.year().to_string())
            .replace("{month_name}", &self.month_names[month])
            .replace("{month_short}", &self.month_short_names[month])
            .replace("{month}", &date.month().to_string())
            .replace("{day}", &date.day().to_string())
            .replace("{weekday}", &self.weekday_short_names[weekday])
    }

    /// `date` as written in the input field (zero padded).
    pub fn format_input(&self, date: NaiveDate) -> String {
        let (y, m, d) = (
            format!("{:04}", date.year()),
            format!("{:02}", date.month()),
            format!("{:02}", date.day()),
        );
        let parts = match self.input_order {
            DateInputOrder::YearMonthDay => [y, m, d],
            DateInputOrder::MonthDayYear => [m, d, y],
            DateInputOrder::DayMonthYear => [d, m, y],
        };
        parts.join(&self.input_separator.to_string())
    }
}

/// Days in `month` (1-12) of `year`.
fn days_in_month(year: i32, month: u32) -> u32 {
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

/// `(year, month)` moved by `delta` months.
fn add_months((year, month): (i32, u32), delta: i32) -> (i32, u32) {
    let index = year * 12 + month as i32 - 1 + delta;
    (index.div_euclid(12), index.rem_euclid(12) as u32 + 1)
}

/// The days of `(year, month)` laid out in weeks starting on `first_day_of_week`, padded with
/// `None` before the first day and after the last day.
fn month_weeks((year, month): (i32, u32), first_day_of_week: u32) -> Vec<[Option<u32>; 7]> {
    let Some(first) = NaiveDate::from_ymd_opt(year, month, 1) else {
        return Vec::new();
    };
    let offset = (first.weekday().num_days_from_sunday() + 7 - first_day_of_week % 7) % 7;
    let days = days_in_month(year, month);
    let mut weeks = Vec::new();
    let mut week = [None; 7];
    let mut column = offset as usize;
    for day in 1..=days {
        week[column] = Some(day);
        column += 1;
        if column == 7 {
            weeks.push(week);
            week = [None; 7];
            column = 0;
        }
    }
    if column > 0 {
        weeks.push(week);
    }
    weeks
}

/// Keeps only the digits of `text` (at most 8) and inserts `separator` between the parts of
/// `order` once the next part has started, so the number keyboard is enough.
fn mask_input(text: &str, order: DateInputOrder, separator: char) -> String {
    let digits: Vec<char> = text.chars().filter(char::is_ascii_digit).take(8).collect();
    let [first, second, _] = order.widths();
    let mut out = String::new();
    for (i, c) in digits.iter().enumerate() {
        if i == first || i == first + second {
            out.push(separator);
        }
        out.push(*c);
    }
    out
}

/// Parses the input field. `None` when the text is not a date in `order`.
fn parse_input(text: &str, order: DateInputOrder) -> Option<NaiveDate> {
    let groups: Vec<&str> = text
        .split(|c: char| !c.is_ascii_digit())
        .filter(|g| !g.is_empty())
        .collect();
    let widths = order.widths();
    let parts: Vec<String> = match groups.as_slice() {
        [a, b, c] => vec![(*a).to_owned(), (*b).to_owned(), (*c).to_owned()],
        [all] if all.len() == 8 => {
            let (a, rest) = all.split_at(widths[0]);
            let (b, c) = rest.split_at(widths[1]);
            vec![a.to_owned(), b.to_owned(), c.to_owned()]
        }
        _ => return None,
    };
    // The year needs all four digits, like Flutter's `yMd` parsing of a compact date.
    for (part, width) in parts.iter().zip(widths) {
        if part.is_empty() || part.len() > width || (width == 4 && part.len() != 4) {
            return None;
        }
    }
    let numbers: Vec<u32> = parts.iter().filter_map(|p| p.parse().ok()).collect();
    let [a, b, c] = numbers.as_slice() else {
        return None;
    };
    let (year, month, day) = order.arrange([*a, *b, *c]);
    NaiveDate::from_ymd_opt(year as i32, month, day)
}

/// Why the typed text was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InputError {
    Format,
    Range,
}

/// The date of the input field, checked against the range.
fn read_input(
    text: &str,
    order: DateInputOrder,
    first: NaiveDate,
    last: NaiveDate,
) -> Result<NaiveDate, InputError> {
    let date = parse_input(text, order).ok_or(InputError::Format)?;
    if date < first || date > last {
        return Err(InputError::Range);
    }
    Ok(date)
}

/// Material 3 date picker dialog (Flutter's `showDatePicker`).
///
/// Setting `open` to true fades the dialog in, starting from `initial_date` (kept between
/// `first_date` and `last_date`). OK calls `on_submit` with the picked date and closes it; Cancel
/// and a tap outside close it without a value.
///
/// Put it in a layer that covers the whole screen (for example the `Scaffold` overlay).
#[component]
pub fn date_picker_dialog(
    open: RwSignal<bool>,
    /// Date selected when the dialog opens.
    initial_date: Signal<NaiveDate>,
    /// Earliest selectable date.
    first_date: NaiveDate,
    /// Latest selectable date.
    last_date: NaiveDate,
    #[prop(default = DatePickerEntryMode::Calendar)] initial_entry_mode: DatePickerEntryMode,
    #[prop(default = DatePickerLabels::default())] labels: DatePickerLabels,
    on_submit: Callback<NaiveDate>,
) -> Element {
    // Keep the dialog mounted until the fade-out finishes.
    let mounted = signal(false);
    let fade = AnimationController::new(AnimConfig::ease_out(FADE_MS));
    let progress = fade.value();
    fade.on_finish(move |finished| {
        if finished && progress.get_untracked() <= 0.0 {
            mounted.set(false);
        }
    });
    {
        let fade = fade.clone();
        effect(move || {
            if open.get() {
                mounted.set(true);
                fade.forward();
            } else {
                fade.reverse();
            }
        });
    }
    let _keep = StoredValue::new(fade);
    let labels = labels.clone();
    let (first, last) = (first_date.min(last_date), first_date.max(last_date));
    render! {
        Show(when: move || mounted.get()) {
            DatePickerFrame(
                open: open,
                progress: progress,
                initial_date: initial_date,
                first_date: first,
                last_date: last,
                initial_entry_mode: initial_entry_mode,
                labels: labels.clone(),
                on_submit: on_submit,
            )
        }
    }
}

#[component]
fn date_picker_frame(
    open: RwSignal<bool>,
    progress: ReadSignal<f32>,
    initial_date: Signal<NaiveDate>,
    first_date: NaiveDate,
    last_date: NaiveDate,
    initial_entry_mode: DatePickerEntryMode,
    labels: DatePickerLabels,
    on_submit: Callback<NaiveDate>,
) -> Element {
    let palette = palette();
    let labels = Rc::new(labels.clone());
    let initial = move || {
        initial_date
            .try_get_untracked()
            .unwrap_or(last_date)
            .clamp(first_date, last_date)
    };
    let start = initial();
    let selected = signal(start);
    let shown_month = signal((start.year(), start.month()));
    let picking_year = signal(false);
    let entry = signal(initial_entry_mode);
    let input_text = signal(labels.format_input(start));
    let input_error = signal(None::<InputError>);
    // Start over each time the dialog opens (it stays mounted while fading out).
    {
        let labels = labels.clone();
        effect(move || {
            if open.get() {
                let date = initial();
                selected.set(date);
                shown_month.set((date.year(), date.month()));
                picking_year.set(false);
                entry.set(initial_entry_mode);
                input_text.set(labels.format_input(date));
                input_error.set(None);
            }
        });
    }

    let order = labels.input_order;
    let check_input = move || read_input(&input_text.get_untracked(), order, first_date, last_date);
    let toggle_entry = {
        let labels = labels.clone();
        move |_: TouchEvent| match entry.get_untracked() {
            DatePickerEntryMode::Calendar => {
                input_text.set(labels.format_input(selected.get_untracked()));
                input_error.set(None);
                entry.set(DatePickerEntryMode::Input);
            }
            DatePickerEntryMode::Input => match check_input() {
                Ok(date) => {
                    selected.set(date);
                    shown_month.set((date.year(), date.month()));
                    picking_year.set(false);
                    entry.set(DatePickerEntryMode::Calendar);
                }
                Err(error) => input_error.set(Some(error)),
            },
        }
    };
    let confirm = move |_: TouchEvent| {
        let date = if entry.get_untracked() == DatePickerEntryMode::Input {
            match check_input() {
                Ok(date) => date,
                Err(error) => {
                    input_error.set(Some(error));
                    return;
                }
            }
        } else {
            selected.get_untracked()
        };
        open.set(false);
        on_submit.run(date);
    };
    let cancel = move |_: TouchEvent| open.set(false);

    let keyboard = keyboard_height();
    let barrier_style = computed(move || {
        fill()
            .z_index(1000)
            .merge(center())
            .padding_left(px(16))
            .padding_right(px(16))
            .padding_bottom(px(keyboard.get() as f32))
            .background_color(palette.barrier)
            .opacity(progress.get())
    });
    let is_calendar = move || entry.get() == DatePickerEntryMode::Calendar;
    let help = {
        let labels = labels.clone();
        computed(move || {
            if entry.get() == DatePickerEntryMode::Calendar {
                labels.help_calendar.clone()
            } else {
                labels.help_input.clone()
            }
        })
    };
    let headline = {
        let labels = labels.clone();
        computed(move || {
            let date = if entry.get() == DatePickerEntryMode::Input {
                // Follow the typed date while it is valid (Flutter's header does the same).
                parse_input(&input_text.get(), order).unwrap_or_else(|| selected.get())
            } else {
                selected.get()
            };
            labels.format(&labels.headline_format, date)
        })
    };
    let entry_icon = computed(move || {
        if entry.get() == DatePickerEntryMode::Calendar {
            lucide::Pencil.to_owned()
        } else {
            lucide::Calendar.to_owned()
        }
    });
    let cancel_text: Signal<String> = labels.cancel.clone().into();
    let confirm_text: Signal<String> = labels.confirm.clone().into();
    let frame_labels = StoredValue::new(labels.clone());
    render! {
        View(style: barrier_style, on_tap: cancel) {
            View(
                style: column()
                    .width(percent(100))
                    .max_width(px(DIALOG_WIDTH))
                    .border_radius(px(shape().extra_large))
                    .background_color(palette.surface),
                on_tap_catch: |_| {},
            ) {
                // Header: help text, the selected date and the entry mode switch.
                View(
                    style: column()
                        .padding_top(px(16))
                        .padding_left(px(24))
                        .padding_right(px(12))
                        .padding_bottom(px(8)),
                ) {
                    Text(value: help, style: text_with(TextKind::LabelLarge, palette.sub_text))
                    View(
                        style: row()
                            .justify_content(JustifyContent::SpaceBetween)
                            .margin_top(px(12))
                            .min_height(px(48)),
                    ) {
                        Text(
                            value: headline,
                            style: text_with(TextKind::HeadlineLarge, palette.text),
                        )
                        View(style: center().width(px(48)).height(px(48)), on_tap: toggle_entry) {
                            Icon(svg: entry_icon, color: css_color(palette.sub_text), size: "24")
                        }
                    }
                }
                View(style: css!(height: px(1), background_color: palette.divider)) {}
                Show(
                    when: is_calendar,
                    fallback: move || render! {
                        DateInput(
                            text: input_text,
                            error: input_error,
                            labels: frame_labels.with(|l| (**l).clone()),
                        )
                    },
                ) {
                    CalendarPicker(
                        selected: selected,
                        shown_month: shown_month,
                        picking_year: picking_year,
                        first_date: first_date,
                        last_date: last_date,
                        labels: frame_labels.with(|l| (**l).clone()),
                    )
                }
                View(
                    style: row()
                        .justify_content(JustifyContent::FlexEnd)
                        .gap(px(8))
                        .padding_left(px(12))
                        .padding_right(px(12))
                        .padding_bottom(px(12))
                        .min_height(px(52)),
                ) {
                    View(style: css!(padding: (px(10), px(12))), on_tap: cancel) {
                        Text(value: cancel_text, style: bold(TextKind::LabelLarge, palette.primary))
                    }
                    View(style: css!(padding: (px(10), px(12))), on_tap: confirm) {
                        Text(value: confirm_text, style: bold(TextKind::LabelLarge, palette.primary))
                    }
                }
            }
        }
    }
}

/// The "month year" row and either the day grid or the year grid.
#[component]
fn calendar_picker(
    selected: RwSignal<NaiveDate>,
    shown_month: RwSignal<(i32, u32)>,
    picking_year: RwSignal<bool>,
    first_date: NaiveDate,
    last_date: NaiveDate,
    labels: DatePickerLabels,
) -> Element {
    let palette = palette();
    let labels = Rc::new(labels.clone());
    let first_month = (first_date.year(), first_date.month());
    let last_month = (last_date.year(), last_date.month());
    let title = {
        let labels = labels.clone();
        computed(move || {
            let (year, month) = shown_month.get();
            NaiveDate::from_ymd_opt(year, month, 1)
                .map(|date| labels.format(&labels.month_year_format, date))
                .unwrap_or_default()
        })
    };
    let title_icon = computed(move || {
        if picking_year.get() {
            lucide::ChevronUp.to_owned()
        } else {
            lucide::ChevronDown.to_owned()
        }
    });
    let can_go = move |delta: i32| {
        let target = add_months(shown_month.get(), delta);
        target >= first_month && target <= last_month
    };
    let go = move |delta: i32| {
        let target = add_months(shown_month.get_untracked(), delta);
        if target >= first_month && target <= last_month {
            shown_month.set(target);
        }
    };
    let arrow_color = move |delta: i32| {
        computed(move || {
            let color = if can_go(delta) {
                palette.sub_text
            } else {
                palette.text.with_alpha(0.38)
            };
            css_color(color)
        })
    };
    let toggle_years = move |_: TouchEvent| picking_year.set(!picking_year.get_untracked());
    let on_year = Callback::new(move |year: i32| {
        // Keep the month, but stay inside the range (Flutter's `_handleYearChanged`).
        let (_, month) = shown_month.get_untracked();
        let target = (year, month).clamp(first_month, last_month);
        shown_month.set(target);
        picking_year.set(false);
    });
    let day_labels = StoredValue::new(labels.clone());
    render! {
        View(style: column().width(percent(100))) {
            View(
                style: row()
                    .justify_content(JustifyContent::SpaceBetween)
                    .height(px(SUB_HEADER_HEIGHT))
                    .padding_left(px(16))
                    .padding_right(px(4)),
            ) {
                View(
                    style: row()
                        .gap(px(4))
                        .padding_left(px(8))
                        .padding_right(px(4))
                        .height(px(40))
                        .border_radius(px(20)),
                    on_tap: toggle_years,
                ) {
                    Text(value: title, style: bold(TextKind::TitleSmall, palette.sub_text))
                    Icon(svg: title_icon, color: css_color(palette.sub_text), size: "18")
                }
                Show(when: move || !picking_year.get()) {
                    View(style: row()) {
                        View(
                            style: center().width(px(48)).height(px(48)),
                            on_tap: move |_| go(-1),
                        ) {
                            Icon(svg: lucide::ChevronLeft, color: arrow_color(-1), size: "24")
                        }
                        View(
                            style: center().width(px(48)).height(px(48)),
                            on_tap: move |_| go(1),
                        ) {
                            Icon(svg: lucide::ChevronRight, color: arrow_color(1), size: "24")
                        }
                    }
                }
            }
            Show(
                when: move || !picking_year.get(),
                fallback: move || render! {
                    YearGrid(
                        selected_year: computed(move || shown_month.get().0),
                        first_date: first_date,
                        last_date: last_date,
                        on_year: on_year,
                    )
                },
            ) {
                DayGrid(
                    selected: selected,
                    shown_month: shown_month,
                    first_date: first_date,
                    last_date: last_date,
                    on_swipe: Callback::new(go),
                    labels: day_labels.with(|l| (**l).clone()),
                )
            }
        }
    }
}

/// One week of the day grid.
#[derive(Clone, Debug, PartialEq)]
struct Week {
    month: (i32, u32),
    index: usize,
    days: [Option<u32>; 7],
}

/// Weekday names and the days of the shown month.
#[component]
fn day_grid(
    selected: RwSignal<NaiveDate>,
    shown_month: RwSignal<(i32, u32)>,
    first_date: NaiveDate,
    last_date: NaiveDate,
    /// Called with -1 / 1 after a swipe to the right / left.
    on_swipe: Callback<i32>,
    labels: DatePickerLabels,
) -> Element {
    let palette = palette();
    let first_day_of_week = labels.first_day_of_week;
    let weekdays: Vec<(usize, String)> = (0..7)
        .map(|i| {
            let index = (first_day_of_week as usize + i) % 7;
            (i, labels.weekday_narrow_names[index].clone())
        })
        .collect();
    let weeks = move || {
        let month = shown_month.get();
        month_weeks(month, first_day_of_week)
            .into_iter()
            .enumerate()
            .map(|(index, days)| Week { month, index, days })
            .collect::<Vec<_>>()
    };
    let today = Local::now().date_naive();

    // A horizontal swipe turns the month (Flutter's month `PageView`).
    let touch_start = Rc::new(Cell::new(None::<(f32, f32)>));
    let touch_last = Rc::new(Cell::new(None::<(f32, f32)>));
    let on_start = {
        let (start, last) = (touch_start.clone(), touch_last.clone());
        move |event: TouchEvent| {
            let point = Some((event.detail.x as f32, event.detail.y as f32));
            start.set(point);
            last.set(point);
        }
    };
    let on_move = {
        let last = touch_last.clone();
        move |event: TouchEvent| last.set(Some((event.detail.x as f32, event.detail.y as f32)))
    };
    let on_end = {
        let (start, last) = (touch_start.clone(), touch_last.clone());
        move |_: TouchEvent| {
            let (Some((x0, y0)), Some((x1, y1))) = (start.take(), last.take()) else {
                return;
            };
            let (dx, dy) = (x1 - x0, y1 - y0);
            if dx.abs() >= SWIPE_DISTANCE && dx.abs() > dy.abs() {
                on_swipe.run(if dx < 0.0 { 1 } else { -1 });
            }
        }
    };
    let weekdays = StoredValue::new(weekdays);
    render! {
        View(
            style: column()
                .width(percent(100))
                .height(px(DAY_ROW_HEIGHT * (DAY_ROWS + 1) as f32))
                .padding_left(px(12))
                .padding_right(px(12)),
            on_capture_touchstart: on_start,
            on_capture_touchmove: on_move,
            on_capture_touchend: on_end,
        ) {
            View(style: row().height(px(DAY_ROW_HEIGHT))) {
                ForEach(
                    each: move || weekdays.get(),
                    key: |(i, _): &(usize, String)| *i,
                    children: move |day: ReadSignal<(usize, String)>| {
                        let (_, name) = day.get_untracked();
                        render! {
                            View(style: center().flex_grow(1.0).flex_basis(px(0))) {
                                Text(value: name, style: text_with(TextKind::BodyLarge, palette.text))
                            }
                        }
                    },
                )
            }
            ForEach(
                each: weeks,
                key: |week: &Week| (week.month, week.index),
                children: move |week: ReadSignal<Week>| {
                    let week = week.get_untracked();
                    let cells: Vec<(usize, Option<u32>)> =
                        week.days.iter().copied().enumerate().collect();
                    let (year, month) = week.month;
                    render! {
                        View(style: row().height(px(DAY_ROW_HEIGHT))) {
                            ForEach(
                                each: move || cells.clone(),
                                key: |(i, _): &(usize, Option<u32>)| *i,
                                children: move |cell: ReadSignal<(usize, Option<u32>)>| {
                                    let (_, day) = cell.get_untracked();
                                    match day.and_then(|d| NaiveDate::from_ymd_opt(year, month, d)) {
                                        Some(date) => render! {
                                            DayCell(
                                                date: date,
                                                selected: selected,
                                                enabled: date >= first_date && date <= last_date,
                                                today: date == today,
                                            )
                                        },
                                        None => render! {
                                            View(style: css!(flex_grow: 1.0, flex_basis: px(0))) {}
                                        },
                                    }
                                },
                            )
                        }
                    }
                },
            )
        }
    }
}

/// One day of the grid: filled when selected, outlined when it is today.
#[component]
fn day_cell(date: NaiveDate, selected: RwSignal<NaiveDate>, enabled: bool, today: bool) -> Element {
    let palette = palette();
    let circle = computed(move || {
        let style = center()
            .width(px(DAY_CIRCLE))
            .height(px(DAY_CIRCLE))
            .border_radius(px(DAY_CIRCLE / 2.0));
        if selected.get() == date {
            style.background_color(palette.primary)
        } else if today {
            style.border(Border::new().width(px(1)).solid().color(palette.primary))
        } else {
            style
        }
    });
    let label = computed(move || {
        let color = if selected.get() == date {
            palette.on_primary
        } else if !enabled {
            palette.text.with_alpha(0.38)
        } else if today {
            palette.primary
        } else {
            palette.text
        };
        text_with(TextKind::BodyLarge, color)
    });
    let pick = move |_: TouchEvent| {
        if enabled {
            selected.set(date);
        }
    };
    render! {
        View(style: center().flex_grow(1.0).flex_basis(px(0)).height(px(DAY_ROW_HEIGHT)), on_tap: pick) {
            View(style: circle) {
                Text(value: date.day().to_string(), style: label)
            }
        }
    }
}

/// One row of the year grid.
#[derive(Clone, Debug, PartialEq)]
struct YearRow {
    index: usize,
    years: Vec<i32>,
}

/// Scrolling grid of the years between `first_date` and `last_date`, opened at the shown year.
#[component]
fn year_grid(
    selected_year: ReadSignal<i32>,
    first_date: NaiveDate,
    last_date: NaiveDate,
    on_year: Callback<i32>,
) -> Element {
    let palette = palette();
    let years: Vec<i32> = (first_date.year()..=last_date.year()).collect();
    let rows: Vec<YearRow> = years
        .chunks(YEAR_COLUMNS)
        .enumerate()
        .map(|(index, years)| YearRow {
            index,
            years: years.to_vec(),
        })
        .collect();
    let this_year = Local::now().year();
    let handle = ScrollViewHandle::new();
    let initial_row =
        (selected_year.get_untracked() - first_date.year()).max(0) as usize / YEAR_COLUMNS;
    // Two rows above the shown year stay visible (Flutter's `_scrollOffsetForYear`).
    let offset = initial_row.saturating_sub(2) as f64 * f64::from(YEAR_ROW_HEIGHT);
    let element = render! {
        ScrollView(
            element_ref: handle.r(),
            style: css!(height: px(DAY_ROW_HEIGHT * (DAY_ROWS + 1) as f32), width: percent(100)),
        ) {
            View(style: column().padding_left(px(12)).padding_right(px(12))) {
                ForEach(
                    each: move || rows.clone(),
                    key: |row: &YearRow| row.index,
                    children: move |year_row: ReadSignal<YearRow>| {
                        let years = year_row.get_untracked().years;
                        let filler = YEAR_COLUMNS - years.len();
                        render! {
                            View(style: row().height(px(YEAR_ROW_HEIGHT))) {
                                ForEach(
                                    each: move || years.clone(),
                                    key: |year: &i32| *year,
                                    children: move |year: ReadSignal<i32>| {
                                        let year = year.get_untracked();
                                        let chip = computed(move || {
                                            let style = center()
                                                .width(px(72))
                                                .height(px(36))
                                                .border_radius(px(18));
                                            if selected_year.get() == year {
                                                style.background_color(palette.primary)
                                            } else if year == this_year {
                                                style.border(Border::new().width(px(1)).solid().color(palette.primary))
                                            } else {
                                                style
                                            }
                                        });
                                        let label = computed(move || {
                                            let color = if selected_year.get() == year {
                                                palette.on_primary
                                            } else if year == this_year {
                                                palette.primary
                                            } else {
                                                palette.sub_text
                                            };
                                            text_with(TextKind::BodyLarge, color)
                                        });
                                        render! {
                                            View(
                                                style: center().flex_grow(1.0).flex_basis(px(0)),
                                                on_tap: move |_| on_year.run(year),
                                            ) {
                                                View(style: chip) {
                                                    Text(value: year.to_string(), style: label)
                                                }
                                            }
                                        }
                                    },
                                )
                                // Keep the columns of a short last row in place.
                                Show(when: move || filler > 0) {
                                    View(style: css!(flex_grow: filler as f32, flex_basis: px(0))) {}
                                }
                            }
                        }
                    },
                )
            }
        }
    };
    // Scroll to the shown year after the first layout.
    let scrolled = Cell::new(false);
    whisker::runtime::view::observe_layout(
        element,
        Box::new(move |layout| {
            if !scrolled.get() && layout.geometry.border_box.height > 0.0 {
                scrolled.set(true);
                handle.scroll_to(offset, false);
            }
        }),
    );
    element
}

/// Outlined date field of input mode with its floating label and error.
#[component]
fn date_input(
    text: RwSignal<String>,
    error: RwSignal<Option<InputError>>,
    labels: DatePickerLabels,
) -> Element {
    let palette = palette();
    let order = labels.input_order;
    let separator = labels.input_separator;
    let focused = signal(false);
    let on_input = move |value: String| {
        let masked = mask_input(&value, order, separator);
        if masked != value {
            text.set(masked);
        }
        error.set(None);
    };
    let border = computed(move || {
        let (width, color) = if error.get().is_some() {
            (2, palette.error)
        } else if focused.get() {
            (2, palette.primary)
        } else {
            (1, palette.outline)
        };
        Border::new().width(px(width)).solid().color(color)
    });
    let field_style = computed(move || {
        text_with(TextKind::BodyLarge, palette.text)
            .width(percent(100))
            .height(px(56))
            .padding_left(px(16))
            .padding_right(px(16))
            .border_radius(px(shape().extra_small))
            .border(border.get())
    });
    let label_style = computed(move || {
        let color = if error.get().is_some() {
            palette.error
        } else if focused.get() {
            palette.primary
        } else {
            palette.sub_text
        };
        text_with(TextKind::BodySmall, color)
            .position(PositionKind::Absolute)
            .top(px(-8))
            .left(px(12))
            .padding_left(px(4))
            .padding_right(px(4))
            .background_color(palette.surface)
    });
    let (invalid_format, out_of_range) =
        (labels.invalid_format.clone(), labels.out_of_range.clone());
    let error_text = computed(move || match error.get() {
        Some(InputError::Format) => invalid_format.clone(),
        Some(InputError::Range) => out_of_range.clone(),
        None => String::new(),
    });
    let label: Signal<String> = labels.field_label.clone().into();
    let hint: Signal<String> = labels.input_hint.clone().into();
    let caret: Signal<String> = css_color(palette.primary).into();
    let placeholder: Signal<String> = css_color(palette.text.with_alpha(0.36)).into();
    let selection: Signal<String> = css_color(palette.primary.with_alpha(0.4)).into();
    render! {
        View(
            style: column()
                .width(percent(100))
                .padding_top(px(24))
                .padding_left(px(24))
                .padding_right(px(24))
                .padding_bottom(px(16)),
        ) {
            View(style: css!(position: PositionKind::Relative, width: percent(100))) {
                Input(
                    text: text,
                    on_input: on_input,
                    on_focus: move |_| focused.set(true),
                    on_blur: move |_| focused.set(false),
                    placeholder: hint,
                    max_length: 10u32,
                    keyboard_type: KeyboardType::Number,
                    auto_focus: true,
                    caret_color: caret,
                    placeholder_color: placeholder,
                    selection_color: selection,
                    style: field_style,
                )
                Text(value: label, style: label_style)
            }
            Show(when: move || error.get().is_some()) {
                Text(
                    value: error_text,
                    style: text_with(TextKind::BodySmall, palette.error)
                        .margin_top(px(4))
                        .margin_left(px(16)),
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
    fn days_in_month_handles_leap_years() {
        assert_eq!(days_in_month(2000, 2), 29);
        assert_eq!(days_in_month(1900, 2), 28);
        assert_eq!(days_in_month(2001, 12), 31);
        assert_eq!(days_in_month(2001, 4), 30);
    }

    #[test]
    fn add_months_wraps_years() {
        assert_eq!(add_months((2001, 1), -1), (2000, 12));
        assert_eq!(add_months((2001, 12), 1), (2002, 1));
        assert_eq!(add_months((2001, 5), 0), (2001, 5));
        assert_eq!(add_months((2001, 5), -17), (1999, 12));
    }

    #[test]
    fn month_weeks_start_on_the_first_weekday() {
        // 2001-01-01 is a Monday.
        let sunday = month_weeks((2001, 1), 0);
        assert_eq!(
            sunday[0],
            [None, Some(1), Some(2), Some(3), Some(4), Some(5), Some(6)]
        );
        assert_eq!(sunday.len(), 5);
        let monday = month_weeks((2001, 1), 1);
        assert_eq!(monday[0][0], Some(1));
        // 2026-08 starts on a Saturday and needs six weeks from Sunday.
        assert_eq!(month_weeks((2026, 8), 0).len(), 6);
        let last = month_weeks((2026, 8), 0).pop().unwrap();
        assert_eq!(last[0], Some(30));
        assert_eq!(last[1], Some(31));
        assert_eq!(last[2], None);
    }

    #[test]
    fn mask_inserts_separators_while_typing() {
        let ymd = DateInputOrder::YearMonthDay;
        assert_eq!(mask_input("1990", ymd, '/'), "1990");
        assert_eq!(mask_input("19905", ymd, '/'), "1990/5");
        assert_eq!(mask_input("19900512", ymd, '/'), "1990/05/12");
        assert_eq!(mask_input("1990/05/12", ymd, '/'), "1990/05/12");
        assert_eq!(mask_input("1990/05/123", ymd, '/'), "1990/05/12");
        let mdy = DateInputOrder::MonthDayYear;
        assert_eq!(mask_input("0512", mdy, '/'), "05/12");
        assert_eq!(mask_input("05121990", mdy, '/'), "05/12/1990");
    }

    #[test]
    fn parse_input_follows_the_order() {
        let ymd = DateInputOrder::YearMonthDay;
        assert_eq!(parse_input("1990/05/12", ymd), Some(date(1990, 5, 12)));
        assert_eq!(parse_input("1990/5/2", ymd), Some(date(1990, 5, 2)));
        assert_eq!(parse_input("19900512", ymd), Some(date(1990, 5, 12)));
        assert_eq!(parse_input("1990/02/30", ymd), None);
        assert_eq!(parse_input("90/05/12", ymd), None);
        assert_eq!(parse_input("1990/05", ymd), None);
        assert_eq!(parse_input("", ymd), None);
        assert_eq!(
            parse_input("05/12/1990", DateInputOrder::MonthDayYear),
            Some(date(1990, 5, 12))
        );
        assert_eq!(
            parse_input("12/05/1990", DateInputOrder::DayMonthYear),
            Some(date(1990, 5, 12))
        );
    }

    #[test]
    fn read_input_checks_the_range() {
        let (first, last) = (date(1900, 1, 1), date(2026, 1, 1));
        let ymd = DateInputOrder::YearMonthDay;
        assert_eq!(
            read_input("2001/01/01", ymd, first, last),
            Ok(date(2001, 1, 1))
        );
        assert_eq!(
            read_input("2026/02/01", ymd, first, last),
            Err(InputError::Range)
        );
        assert_eq!(
            read_input("1899/12/31", ymd, first, last),
            Err(InputError::Range)
        );
        assert_eq!(
            read_input("2001/13/01", ymd, first, last),
            Err(InputError::Format)
        );
    }

    #[test]
    fn labels_format_dates() {
        let labels = DatePickerLabels::default();
        let day = date(2001, 1, 1);
        assert_eq!(labels.format(&labels.headline_format, day), "Mon, Jan 1");
        assert_eq!(
            labels.format(&labels.month_year_format, day),
            "January 2001"
        );
        assert_eq!(labels.format_input(day), "01/01/2001");
        let ja = DatePickerLabels {
            input_order: DateInputOrder::YearMonthDay,
            weekday_short_names: strings(["日", "月", "火", "水", "木", "金", "土"]),
            month_year_format: "{year}年{month}月".to_owned(),
            headline_format: "{month}月{day}日({weekday})".to_owned(),
            ..DatePickerLabels::default()
        };
        assert_eq!(ja.format(&ja.headline_format, day), "1月1日(月)");
        assert_eq!(ja.format(&ja.month_year_format, day), "2001年1月");
        assert_eq!(ja.format_input(day), "2001/01/01");
    }
}
