//! Material 3 time picker dialog (Flutter's `showTimePicker` / `TimePickerDialog`).
//!
//! Ported from Flutter's `packages/flutter/lib/src/material/time_picker.dart` (the Material 3
//! portrait layout and the `_TimePickerDefaultsM3` colors). The dialog draws everything in Whisker,
//! so it looks the same on iOS and Android.
//!
//! - Dial mode: tap or drag on the dial to pick the hour, then the minute. 24-hour formats use the
//!   Material 3 double ring (`00`–`11` outside, `12`–`23` inside).
//! - Input mode: type the hour and minute with the number keyboard.
//!
//! ```ignore
//! let open = signal(false);
//! let time = signal(TimeOfDay::new(8, 0));
//! render! {
//!     TimePickerDialog(
//!         open: open,
//!         initial_time: time,
//!         hour_format: HourFormat::Hour24,
//!         on_submit: move |picked| time.set(picked),
//!     )
//! }
//! ```

use std::cell::Cell;
use std::f32::consts::TAU;
use std::rc::Rc;

use masamune_whisker_appearance::{ColorExt, TextKind, bold, palette, shape, text_with};
use whisker::AnimConfig;
use whisker::AnimationController;
use whisker::css::{AlignItems, Border, Color, JustifyContent, PositionKind, TextAlign};
use whisker::event::TouchEvent;
use whisker::prelude::*;
use whisker_icons::{Icon, lucide};
use whisker_input::{Input, KeyboardType};
use whisker_keyboard::keyboard_height;
use whisker_svg::Svg;

use crate::style::{center, column, css_color, fill, row};

/// Diameter of the dial.
const DIAL_SIZE: f32 = 256.0;
/// Center of the dial in its own coordinates.
const DIAL_CENTER: f32 = DIAL_SIZE / 2.0;
/// Radius of the (outer) label ring: the dial radius minus `_kTimePickerDialPadding`.
const LABEL_RADIUS: f32 = DIAL_CENTER - 28.0;
/// Distance between the outer and inner rings (`_kTimePickerInnerDialOffset`).
const RING_GAP: f32 = 28.0;
/// Radius of the inner ring of the 24-hour dial.
const INNER_RADIUS: f32 = LABEL_RADIUS - RING_GAP;
/// Radius of the selector circle at the end of the hand.
const SELECTOR_RADIUS: f32 = 24.0;
/// Size of the box each dial label is centered in.
const LABEL_BOX: f32 = 48.0;
/// Hand animation after a tap, a drag or a mode switch (`_kDialAnimateDuration`).
const DIAL_ANIMATE_MS: u32 = 200;
/// Fade of the dialog (Flutter's `showDialog` transition).
const FADE_MS: u32 = 150;
/// Movement (px) after which a touch on the dial counts as a drag instead of a tap.
const DRAG_SLOP: f32 = 6.0;
/// Two taps on the selected hour/minute box within this time switch to input mode.
const DOUBLE_TAP_MS: f64 = 300.0;

/// A time of day without a date (Flutter's `TimeOfDay`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TimeOfDay {
    /// `0..=23`.
    pub hour: u32,
    /// `0..=59`.
    pub minute: u32,
}

impl TimeOfDay {
    /// Wraps out-of-range values (`hour % 24`, `minute % 60`).
    pub const fn new(hour: u32, minute: u32) -> Self {
        Self {
            hour: hour % 24,
            minute: minute % 60,
        }
    }

    /// True from 12:00 to 23:59.
    pub const fn is_pm(&self) -> bool {
        self.hour >= 12
    }

    /// Hour within the half day (`0..=11`).
    pub const fn hour_of_period(&self) -> u32 {
        self.hour % 12
    }
}

/// How the hour is written (Flutter's `HourFormat`). Pick the one the app's locale uses: Flutter's
/// `ja` is `H:mm`, `en` is `h:mm a` and most European locales are `HH:mm`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum HourFormat {
    /// `HH`: 24-hour, zero-padded (`08`).
    #[default]
    Hour24Padded,
    /// `H`: 24-hour without padding (`8`).
    Hour24,
    /// `h`: 12-hour with an AM/PM switch (`8`, `12`).
    Hour12,
}

impl HourFormat {
    pub const fn is_24_hour(self) -> bool {
        !matches!(self, Self::Hour12)
    }

    /// The hour of `time` as shown in the header and the input field.
    pub fn format(self, time: TimeOfDay) -> String {
        match self {
            Self::Hour24Padded => format!("{:02}", time.hour),
            Self::Hour24 => time.hour.to_string(),
            Self::Hour12 => match time.hour_of_period() {
                0 => "12".to_owned(),
                hour => hour.to_string(),
            },
        }
    }
}

/// Which picker the dialog opens with (Flutter's `TimePickerEntryMode`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TimePickerEntryMode {
    /// Clock dial.
    #[default]
    Dial,
    /// Text fields with the number keyboard.
    Input,
}

/// Texts of the dialog. The defaults are English; pass the app's localized strings
/// (Flutter's `MaterialLocalizations`: `timePickerDialHelpText`, `timePickerInputHelpText`, ...).
#[derive(Clone, Debug, PartialEq)]
pub struct TimePickerLabels {
    /// Heading in dial mode ("Select time").
    pub help_dial: String,
    /// Heading in input mode ("Enter time").
    pub help_input: String,
    /// Caption under the hour field.
    pub hour: String,
    /// Caption under the minute field.
    pub minute: String,
    /// Error shown when the typed time is invalid.
    pub invalid: String,
    pub cancel: String,
    pub confirm: String,
    pub am: String,
    pub pm: String,
}

impl Default for TimePickerLabels {
    fn default() -> Self {
        Self {
            help_dial: "Select time".to_owned(),
            help_input: "Enter time".to_owned(),
            hour: "Hour".to_owned(),
            minute: "Minute".to_owned(),
            invalid: "Enter a valid time".to_owned(),
            cancel: "Cancel".to_owned(),
            confirm: "OK".to_owned(),
            am: "AM".to_owned(),
            pm: "PM".to_owned(),
        }
    }
}

/// Whether the dial picks the hour or the minute.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Mode {
    Hour,
    Minute,
}

/// Position of the hand for `time`: the clockwise fraction of a turn from 12 o'clock.
fn dial_fraction(time: TimeOfDay, mode: Mode) -> f32 {
    match mode {
        Mode::Hour => time.hour_of_period() as f32 / 12.0,
        Mode::Minute => time.minute as f32 / 60.0,
    }
}

/// Ring of `time`: `1.0` is the outer ring, `0.0` the inner ring of the 24-hour dial.
fn dial_ring(time: TimeOfDay, mode: Mode, is_24_hour: bool) -> f32 {
    if mode == Mode::Hour && is_24_hour && time.is_pm() {
        0.0
    } else {
        1.0
    }
}

/// Distance of the hand's end from the center for `ring`.
fn hand_length(ring: f32) -> f32 {
    INNER_RADIUS + ring * RING_GAP
}

/// Clockwise fraction of a turn from 12 o'clock for a point relative to the dial center
/// (`y` grows downwards).
fn fraction_from_point(dx: f32, dy: f32) -> f32 {
    (dx.atan2(-dy) / TAU).rem_euclid(1.0)
}

/// Ring for a touch at `distance` from the center: below `0.5` picks the inner ring.
fn ring_from_distance(distance: f32) -> f32 {
    ((distance - INNER_RADIUS) / RING_GAP).clamp(0.0, 1.0)
}

/// The time the hand at `fraction` / `ring` points to (Flutter's `_getTimeForTheta`). Minutes snap
/// to 5 when `round_minutes` is set (taps).
fn time_for_fraction(
    selected: TimeOfDay,
    mode: Mode,
    is_24_hour: bool,
    fraction: f32,
    ring: f32,
    round_minutes: bool,
) -> TimeOfDay {
    match mode {
        Mode::Hour => {
            let hour = (fraction * 12.0).round() as u32 % 12;
            let offset = if is_24_hour {
                if ring < 0.5 { 12 } else { 0 }
            } else if selected.is_pm() {
                12
            } else {
                0
            };
            TimeOfDay::new(hour + offset, selected.minute)
        }
        Mode::Minute => {
            let mut minute = (fraction * 60.0).round() as u32 % 60;
            if round_minutes {
                minute = (minute + 2) / 5 * 5 % 60;
            }
            TimeOfDay::new(selected.hour, minute)
        }
    }
}

/// Interpolates between two fractions of a turn along the shorter way around.
fn lerp_fraction(from: f32, to: f32, t: f32) -> f32 {
    let delta = (to - from + 0.5).rem_euclid(1.0) - 0.5;
    (from + delta * t).rem_euclid(1.0)
}

/// Point on the dial at `fraction` / `radius`, in dial coordinates.
fn dial_point(fraction: f32, radius: f32) -> (f32, f32) {
    let angle = fraction * TAU;
    (
        DIAL_CENTER + radius * angle.sin(),
        DIAL_CENTER - radius * angle.cos(),
    )
}

/// A number on the dial.
#[derive(Clone, Debug, PartialEq)]
struct DialLabel {
    /// Hour or minute the label stands for.
    value: u32,
    text: String,
    /// Clockwise fraction of a turn from 12 o'clock.
    fraction: f32,
    radius: f32,
}

/// Labels of the dial for `mode`.
fn dial_labels(mode: Mode, is_24_hour: bool) -> Vec<DialLabel> {
    match mode {
        Mode::Minute => (0..12)
            .map(|i| DialLabel {
                value: i * 5,
                text: format!("{:02}", i * 5),
                fraction: i as f32 / 12.0,
                radius: LABEL_RADIUS,
            })
            .collect(),
        // Material 3 shows hour 0 as `00` but 1 to 23 without padding.
        Mode::Hour if is_24_hour => (0..24)
            .map(|hour| DialLabel {
                value: hour,
                text: if hour == 0 {
                    "00".to_owned()
                } else {
                    hour.to_string()
                },
                fraction: (hour % 12) as f32 / 12.0,
                radius: if hour >= 12 {
                    INNER_RADIUS
                } else {
                    LABEL_RADIUS
                },
            })
            .collect(),
        Mode::Hour => (0..12)
            .map(|hour| DialLabel {
                value: hour,
                text: if hour == 0 {
                    "12".to_owned()
                } else {
                    hour.to_string()
                },
                fraction: hour as f32 / 12.0,
                radius: LABEL_RADIUS,
            })
            .collect(),
    }
}

/// Parses the input fields. `None` when a field is empty, not a number or out of range.
fn parse_input(hour: &str, minute: &str, format: HourFormat, is_pm: bool) -> Option<TimeOfDay> {
    let parse = |text: &str| -> Option<u32> {
        let text = text.trim();
        if text.is_empty() || !text.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        text.parse().ok()
    };
    let hour = parse(hour)?;
    let minute = parse(minute)?;
    if minute > 59 {
        return None;
    }
    let hour = if format.is_24_hour() {
        if hour > 23 {
            return None;
        }
        hour
    } else {
        if !(1..=12).contains(&hour) {
            return None;
        }
        hour % 12 + if is_pm { 12 } else { 0 }
    };
    Some(TimeOfDay::new(hour, minute))
}

/// Touch position in dial coordinates. Whisker reports touch positions in surface coordinates
/// (`Touch::x` too, although its docs say element-local), so the dial's surface origin is
/// subtracted.
fn dial_local(event: &TouchEvent, origin: (f32, f32)) -> (f32, f32) {
    (
        event.detail.x as f32 - origin.0,
        event.detail.y as f32 - origin.1,
    )
}

/// Keeps `offset` at the border-box position of the element bound to `element_ref` inside its
/// parent. Whisker has no public layout event for views yet, so this uses the runtime's layout
/// observer (the same one `whisker-input` uses for `on_size_change`).
fn track_offset(element_ref: ElementRef, offset: RwSignal<(f32, f32)>) {
    let bound = element_ref.bound();
    effect(move || {
        if !bound.get() {
            return;
        }
        if let Some(element) = element_ref.element() {
            whisker::runtime::view::observe_layout(
                element,
                Box::new(move |layout| {
                    let rect = layout.geometry.border_box;
                    if offset.get_untracked() != (rect.x, rect.y) {
                        offset.set((rect.x, rect.y));
                    }
                }),
            );
        }
    });
}

/// Material 3 time picker dialog (Flutter's `showTimePicker`).
///
/// Setting `open` to true fades the dialog in, starting from `initial_time`. OK calls `on_submit`
/// with the picked time and closes it; Cancel and a tap outside close it without a value.
///
/// Put it in a layer that covers the whole screen (for example the `Scaffold` overlay): the dial
/// maps touches with its position on the screen.
#[component]
pub fn time_picker_dialog(
    open: RwSignal<bool>,
    /// Time selected when the dialog opens.
    initial_time: Signal<TimeOfDay>,
    #[prop(default = HourFormat::Hour24Padded)] hour_format: HourFormat,
    #[prop(default = TimePickerEntryMode::Dial)] initial_entry_mode: TimePickerEntryMode,
    #[prop(default = TimePickerLabels::default())] labels: TimePickerLabels,
    on_submit: Callback<TimeOfDay>,
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
    render! {
        Show(when: move || mounted.get()) {
            TimePickerFrame(
                open: open,
                progress: progress,
                initial_time: initial_time,
                hour_format: hour_format,
                initial_entry_mode: initial_entry_mode,
                labels: labels.clone(),
                on_submit: on_submit,
            )
        }
    }
}

/// Animates the hand to a new fraction and ring along the shorter way.
#[derive(Clone)]
struct HandAnimator {
    controller: AnimationController,
    from: Rc<Cell<(f32, f32)>>,
    to: Rc<Cell<(f32, f32)>>,
    active: Rc<Cell<bool>>,
}

impl HandAnimator {
    fn new(hand: RwSignal<f32>, ring: RwSignal<f32>) -> Self {
        // Flutter's `standardEasing`.
        let controller = AnimationController::new(AnimConfig::cubic_bezier(
            DIAL_ANIMATE_MS,
            0.4,
            0.0,
            0.2,
            1.0,
        ));
        let from = Rc::new(Cell::new((0.0, 1.0)));
        let to = Rc::new(Cell::new((0.0, 1.0)));
        let active = Rc::new(Cell::new(false));
        let progress = controller.value();
        {
            let (from, to, active) = (from.clone(), to.clone(), active.clone());
            effect(move || {
                let t = progress.get();
                if active.get() {
                    let ((from_hand, from_ring), (to_hand, to_ring)) = (from.get(), to.get());
                    hand.set(lerp_fraction(from_hand, to_hand, t));
                    ring.set(from_ring + (to_ring - from_ring) * t);
                }
            });
        }
        {
            let active = active.clone();
            controller.on_finish(move |_| active.set(false));
        }
        Self {
            controller,
            from,
            to,
            active,
        }
    }

    fn animate(&self, hand: RwSignal<f32>, ring: RwSignal<f32>, target: (f32, f32)) {
        self.controller.stop();
        self.from.set((hand.get_untracked(), ring.get_untracked()));
        self.to.set(target);
        self.active.set(true);
        self.controller.set_value(0.0);
        self.controller.forward();
    }

    /// Stops the animation where it is (a new touch takes over).
    fn stop(&self) {
        self.active.set(false);
        self.controller.stop();
    }
}

#[component]
fn time_picker_frame(
    open: RwSignal<bool>,
    progress: ReadSignal<f32>,
    initial_time: Signal<TimeOfDay>,
    hour_format: HourFormat,
    initial_entry_mode: TimePickerEntryMode,
    labels: TimePickerLabels,
    on_submit: Callback<TimeOfDay>,
) -> Element {
    let palette = palette();
    let is_24_hour = hour_format.is_24_hour();
    let initial = initial_time.try_get_untracked().unwrap_or_default();
    let selected = signal(initial);
    let mode = signal(Mode::Hour);
    let entry = signal(initial_entry_mode);
    let hand = signal(dial_fraction(initial, Mode::Hour));
    let ring = signal(dial_ring(initial, Mode::Hour, is_24_hour));
    let hour_text = signal(hour_format.format(initial));
    let minute_text = signal(format!("{:02}", initial.minute));
    let invalid = signal(false);
    let animator = HandAnimator::new(hand, ring);
    let stored_animator = StoredValue::new(animator.clone());
    // Surface position of the dial: the barrier fills the screen, so the card and the dial frame
    // offsets add up to the dial frame's origin (the dial adds its own offset).
    let card_ref = ElementRef::new();
    let frame_ref = ElementRef::new();
    let card_offset = signal((0.0, 0.0));
    let frame_offset = signal((0.0, 0.0));
    track_offset(card_ref, card_offset);
    track_offset(frame_ref, frame_offset);
    let frame_origin = computed(move || {
        let (card, frame) = (card_offset.get(), frame_offset.get());
        (card.0 + frame.0, card.1 + frame.1)
    });

    let help_dial: Signal<String> = labels.help_dial.clone().into();
    let help_input: Signal<String> = labels.help_input.clone().into();
    let hour_caption: Signal<String> = labels.hour.clone().into();
    let minute_caption: Signal<String> = labels.minute.clone().into();
    let invalid_text: Signal<String> = labels.invalid.clone().into();
    let cancel_text: Signal<String> = labels.cancel.clone().into();
    let confirm_text: Signal<String> = labels.confirm.clone().into();
    let am_text: Signal<String> = labels.am.clone().into();
    let pm_text: Signal<String> = labels.pm.clone().into();

    // Switches the dial between hours and minutes, moving the hand to the selected value.
    let switch_mode = {
        let animator = animator.clone();
        Callback::new(move |next: Mode| {
            mode.set(next);
            let time = selected.get_untracked();
            animator.animate(
                hand,
                ring,
                (dial_fraction(time, next), dial_ring(time, next, is_24_hour)),
            );
        })
    };
    let read_input = move || {
        parse_input(
            &hour_text.get_untracked(),
            &minute_text.get_untracked(),
            hour_format,
            selected.get_untracked().is_pm(),
        )
    };
    let toggle_entry = move |_: TouchEvent| match entry.get_untracked() {
        TimePickerEntryMode::Dial => {
            let time = selected.get_untracked();
            hour_text.set(hour_format.format(time));
            minute_text.set(format!("{:02}", time.minute));
            invalid.set(false);
            entry.set(TimePickerEntryMode::Input);
        }
        TimePickerEntryMode::Input => {
            let Some(time) = read_input() else {
                invalid.set(true);
                return;
            };
            selected.set(time);
            mode.set(Mode::Hour);
            hand.set(dial_fraction(time, Mode::Hour));
            ring.set(dial_ring(time, Mode::Hour, is_24_hour));
            entry.set(TimePickerEntryMode::Dial);
        }
    };
    let confirm = move |_: TouchEvent| {
        let time = if entry.get_untracked() == TimePickerEntryMode::Input {
            match read_input() {
                Some(time) => time,
                None => {
                    invalid.set(true);
                    return;
                }
            }
        } else {
            selected.get_untracked()
        };
        open.set(false);
        on_submit.run(time);
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
    let is_dial = move || entry.get() == TimePickerEntryMode::Dial;
    let help = computed(move || {
        if entry.get() == TimePickerEntryMode::Dial {
            help_dial.get()
        } else {
            help_input.get()
        }
    });
    let entry_icon = computed(move || {
        if entry.get() == TimePickerEntryMode::Dial {
            lucide::Keyboard.to_owned()
        } else {
            lucide::Clock.to_owned()
        }
    });
    render! {
        View(style: barrier_style, on_tap: cancel) {
            View(
                style: column()
                    .width(percent(100))
                    .max_width(px(358))
                    .padding_top(px(24))
                    .padding_left(px(24))
                    .padding_right(px(24))
                    .padding_bottom(px(16))
                    .border_radius(px(shape().extra_large))
                    .background_color(palette.surface),
                element_ref: card_ref,
                on_tap_catch: |_| {},
            ) {
                Text(
                    value: help,
                    style: text_with(TextKind::LabelMedium, palette.sub_text).margin_bottom(px(20)),
                )
                Show(
                    when: is_dial,
                    fallback: move || render! {
                        TimeInput(
                            selected: selected,
                            hour_format: hour_format,
                            hour_text: hour_text,
                            minute_text: minute_text,
                            invalid: invalid,
                            hour_caption: hour_caption,
                            minute_caption: minute_caption,
                            invalid_text: invalid_text,
                            am_text: am_text,
                            pm_text: pm_text,
                        )
                    },
                ) {
                    DialHeader(
                        selected: selected,
                        mode: mode,
                        entry: entry,
                        hour_format: hour_format,
                        switch_mode: switch_mode,
                        hour_text: hour_text,
                        minute_text: minute_text,
                        am_text: am_text,
                        pm_text: pm_text,
                    )
                    View(
                        style: center().padding_top(px(28)).padding_bottom(px(12)),
                        element_ref: frame_ref,
                    ) {
                        Dial(
                            origin: frame_origin,
                            selected: selected,
                            mode: mode,
                            hand: hand,
                            ring: ring,
                            is_24_hour: is_24_hour,
                            animator: stored_animator,
                            switch_mode: switch_mode,
                        )
                    }
                }
                View(
                    style: row()
                        .justify_content(JustifyContent::SpaceBetween)
                        .min_height(px(48)),
                ) {
                    View(
                        style: center().width(px(48)).height(px(48)).margin_left(px(-12)),
                        on_tap: toggle_entry,
                    ) {
                        Icon(svg: entry_icon, color: css_color(palette.text), size: "24")
                    }
                    View(style: row().gap(px(8))) {
                        View(style: css!(padding: (px(10), px(12))), on_tap: cancel) {
                            Text(
                                value: cancel_text,
                                style: bold(TextKind::LabelLarge, palette.primary),
                            )
                        }
                        View(style: css!(padding: (px(10), px(12))), on_tap: confirm) {
                            Text(
                                value: confirm_text,
                                style: bold(TextKind::LabelLarge, palette.primary),
                            )
                        }
                    }
                }
            }
        }
    }
}

/// Hour and minute boxes (and the AM/PM switch) above the dial.
#[component]
fn dial_header(
    selected: RwSignal<TimeOfDay>,
    mode: RwSignal<Mode>,
    entry: RwSignal<TimePickerEntryMode>,
    hour_format: HourFormat,
    switch_mode: Callback<Mode>,
    hour_text: RwSignal<String>,
    minute_text: RwSignal<String>,
    am_text: Signal<String>,
    pm_text: Signal<String>,
) -> Element {
    let palette = palette();
    let last_tap = Rc::new(Cell::new((None::<Mode>, f64::MIN)));
    // A tap selects the box; a second tap on the selected box opens input mode (Flutter's
    // double tap on the hour/minute control).
    let on_box = move |target: Mode, event: &TouchEvent| {
        let (previous, at) = last_tap.get();
        last_tap.set((Some(target), event.timestamp));
        if mode.get_untracked() == target {
            if previous == Some(target) && event.timestamp - at < DOUBLE_TAP_MS {
                let time = selected.get_untracked();
                hour_text.set(hour_format.format(time));
                minute_text.set(format!("{:02}", time.minute));
                entry.set(TimePickerEntryMode::Input);
            }
        } else {
            switch_mode.run(target);
        }
    };
    let on_hour = {
        let on_box = on_box.clone();
        move |event: TouchEvent| on_box(Mode::Hour, &event)
    };
    let on_minute = move |event: TouchEvent| on_box(Mode::Minute, &event);
    let box_style = move |target: Mode| {
        computed(move || {
            let active = mode.get() == target;
            center()
                .flex_grow(1.0)
                .flex_basis(px(0))
                .height(px(80))
                .border_radius(px(shape().small))
                .background_color(if active {
                    palette.primary_container
                } else {
                    palette.surface_container_highest
                })
        })
    };
    let box_text = move |target: Mode| {
        computed(move || {
            let color = if mode.get() == target {
                palette.on_primary_container
            } else {
                palette.text
            };
            text_with(TextKind::DisplayLarge, color)
        })
    };
    let hour_label = computed(move || hour_format.format(selected.get()));
    let minute_label = computed(move || format!("{:02}", selected.get().minute));
    render! {
        View(style: row().width(percent(100)).height(px(80))) {
            View(style: box_style(Mode::Hour), on_tap: on_hour) {
                Text(value: hour_label, style: box_text(Mode::Hour))
            }
            View(style: center().width(px(24)).height(px(80))) {
                Text(value: ":", style: text_with(TextKind::DisplayLarge, palette.text))
            }
            View(style: box_style(Mode::Minute), on_tap: on_minute) {
                Text(value: minute_label, style: box_text(Mode::Minute))
            }
            Show(when: move || !hour_format.is_24_hour()) {
                DayPeriodToggle(
                    selected: selected,
                    height: 80.0,
                    am_text: am_text,
                    pm_text: pm_text,
                )
            }
        }
    }
}

/// Vertical AM/PM switch of the 12-hour formats.
#[component]
fn day_period_toggle(
    selected: RwSignal<TimeOfDay>,
    height: f32,
    am_text: Signal<String>,
    pm_text: Signal<String>,
) -> Element {
    let palette = palette();
    let radius = shape().small;
    let half = move |pm: bool| {
        computed(move || {
            let active = selected.get().is_pm() == pm;
            let style = center().flex_grow(1.0).background_color(if active {
                palette.tertiary_container
            } else {
                Color::Transparent
            });
            if pm {
                style
                    .border_top(Border::new().width(px(1)).solid().color(palette.outline))
                    .border_bottom_left_radius(px(radius - 1.0))
                    .border_bottom_right_radius(px(radius - 1.0))
            } else {
                style
                    .border_top_left_radius(px(radius - 1.0))
                    .border_top_right_radius(px(radius - 1.0))
            }
        })
    };
    let label = move |pm: bool| {
        computed(move || {
            let color = if selected.get().is_pm() == pm {
                palette.on_tertiary_container
            } else {
                palette.sub_text
            };
            bold(TextKind::TitleMedium, color)
        })
    };
    let pick = move |pm: bool| {
        let time = selected.get_untracked();
        if time.is_pm() != pm {
            selected.set(TimeOfDay::new(
                time.hour_of_period() + if pm { 12 } else { 0 },
                time.minute,
            ));
        }
    };
    render! {
        View(
            style: column()
                .width(px(52))
                .height(px(height))
                .margin_left(px(12))
                .border_radius(px(radius))
                .border(Border::new().width(px(1)).solid().color(palette.outline)),
        ) {
            View(style: half(false), on_tap: move |_| pick(false)) {
                Text(value: am_text, style: label(false))
            }
            View(style: half(true), on_tap: move |_| pick(true)) {
                Text(value: pm_text, style: label(true))
            }
        }
    }
}

/// The clock dial with its hand and labels.
#[component]
fn dial(
    /// Surface position of the dial's parent.
    origin: ReadSignal<(f32, f32)>,
    selected: RwSignal<TimeOfDay>,
    mode: RwSignal<Mode>,
    hand: RwSignal<f32>,
    ring: RwSignal<f32>,
    is_24_hour: bool,
    animator: StoredValue<HandAnimator>,
    switch_mode: Callback<Mode>,
) -> Element {
    let palette = palette();
    let primary = css_color(palette.primary);
    let on_primary = css_color(palette.on_primary);
    let hand_color = primary.clone();
    let svg = computed(move || {
        let primary = &hand_color;
        let (x, y) = dial_point(hand.get(), hand_length(ring.get()));
        // A dot marks minutes between two labels.
        let between = mode.get() == Mode::Minute && selected.get().minute % 5 != 0;
        let dot = if between {
            format!(r#"<circle cx="{x}" cy="{y}" r="2" fill="{on_primary}"/>"#)
        } else {
            String::new()
        };
        format!(
            r#"<svg viewBox="0 0 {size} {size}"><line x1="{c}" y1="{c}" x2="{x}" y2="{y}" stroke="{primary}" stroke-width="2"/><circle cx="{c}" cy="{c}" r="4" fill="{primary}"/><circle cx="{x}" cy="{y}" r="{r}" fill="{primary}"/>{dot}</svg>"#,
            size = DIAL_SIZE,
            c = DIAL_CENTER,
            r = SELECTOR_RADIUS,
        )
    });

    let dial_ref = ElementRef::new();
    let dial_offset = signal((0.0, 0.0));
    track_offset(dial_ref, dial_offset);
    let origin = move || {
        let (parent, own) = (origin.get_untracked(), dial_offset.get_untracked());
        (parent.0 + own.0, parent.1 + own.1)
    };
    let start = Rc::new(Cell::new(None::<(f32, f32)>));
    let last = Rc::new(Cell::new(None::<(f32, f32)>));
    let dragging = Rc::new(Cell::new(false));
    // Moves the hand to `point` and selects the time it points to.
    let select = move |(x, y): (f32, f32), round_minutes: bool| -> TimeOfDay {
        let (dx, dy) = (x - DIAL_CENTER, y - DIAL_CENTER);
        let current = mode.get_untracked();
        let fraction = fraction_from_point(dx, dy);
        let new_ring = if current == Mode::Hour && is_24_hour {
            ring_from_distance((dx * dx + dy * dy).sqrt())
        } else {
            1.0
        };
        let time = time_for_fraction(
            selected.get_untracked(),
            current,
            is_24_hour,
            fraction,
            new_ring,
            round_minutes,
        );
        if !round_minutes {
            hand.set(fraction);
            ring.set(new_ring);
        }
        if time != selected.get_untracked() {
            selected.set(time);
        }
        time
    };
    // After a tap or a drag: settle on the selected value, then go on to the minutes.
    let settle = move |time: TimeOfDay| {
        let current = mode.get_untracked();
        if current == Mode::Hour {
            switch_mode.run(Mode::Minute);
        } else {
            animator.with(|a| {
                a.animate(
                    hand,
                    ring,
                    (
                        dial_fraction(time, current),
                        dial_ring(time, current, is_24_hour),
                    ),
                )
            });
        }
    };
    let on_touchstart = {
        let (start, last, dragging) = (start.clone(), last.clone(), dragging.clone());
        move |event: TouchEvent| {
            animator.with(|a| a.stop());
            let point = Some(dial_local(&event, origin()));
            start.set(point);
            last.set(point);
            dragging.set(false);
        }
    };
    let on_touchmove = {
        let (start, last, dragging) = (start.clone(), last.clone(), dragging.clone());
        move |event: TouchEvent| {
            let point = dial_local(&event, origin());
            last.set(Some(point));
            if !dragging.get() {
                let Some((x, y)) = start.get() else {
                    return;
                };
                if (point.0 - x).abs() < DRAG_SLOP && (point.1 - y).abs() < DRAG_SLOP {
                    return;
                }
                dragging.set(true);
            }
            select(point, false);
        }
    };
    let on_touchend = {
        let (start, last, dragging) = (start.clone(), last.clone(), dragging.clone());
        move |_: TouchEvent| {
            let tap_point = start.get();
            start.set(None);
            if dragging.get() {
                dragging.set(false);
                settle(selected.get_untracked());
            } else if let Some(point) = tap_point.or(last.get()) {
                settle(select(point, true));
            }
        }
    };
    let on_touchcancel = {
        let start = start.clone();
        move |_: TouchEvent| {
            start.set(None);
            if dragging.get() {
                dragging.set(false);
                settle(selected.get_untracked());
            }
        }
    };

    let hour_labels = dial_labels(Mode::Hour, is_24_hour);
    let minute_labels = dial_labels(Mode::Minute, is_24_hour);
    render! {
        View(
            style: css!(
                position: PositionKind::Relative,
                width: px(DIAL_SIZE),
                height: px(DIAL_SIZE),
                flex_shrink: 0.0,
                border_radius: px(DIAL_CENTER),
                background_color: palette.surface_container_highest,
            ),
            element_ref: dial_ref,
        ) {
            Svg(
                content: svg,
                color: primary.clone(),
                style: fill().width(px(DIAL_SIZE)).height(px(DIAL_SIZE)),
            )
            Show(
                when: move || mode.get() == Mode::Hour,
                fallback: move || {
                    let labels = minute_labels.clone();
                    render! { DialNumbers(labels: labels, hand: hand, ring: ring) }
                },
            ) {
                DialNumbers(labels: hour_labels.clone(), hand: hand, ring: ring)
            }
            // On top of the labels so every touch lands on this view.
            View(
                style: fill(),
                on_touchstart_catch: on_touchstart,
                on_touchmove_catch: on_touchmove,
                on_touchend_catch: on_touchend,
                on_touchcancel_catch: on_touchcancel,
                on_tap_catch: |_| {},
            ) {
            }
        }
    }
}

/// Numbers around the dial. A number under the selector circle takes the `on_primary` color.
#[component]
fn dial_numbers(labels: Vec<DialLabel>, hand: RwSignal<f32>, ring: RwSignal<f32>) -> Element {
    let palette = palette();
    let labels = labels.clone();
    render! {
        ForEach(
            each: move || labels.clone(),
            key: |label: &DialLabel| label.value,
            children: move |label: ReadSignal<DialLabel>| {
                let label = label.get_untracked();
                let (x, y) = dial_point(label.fraction, label.radius);
                let style = computed(move || {
                    let (sx, sy) = dial_point(hand.get(), hand_length(ring.get()));
                    let under = ((sx - x).powi(2) + (sy - y).powi(2)).sqrt() < SELECTOR_RADIUS / 2.0;
                    text_with(
                        TextKind::BodyLarge,
                        if under {
                            palette.on_primary
                        } else {
                            palette.text
                        },
                    )
                });
                render! {
                    View(
                        style: center()
                            .position(PositionKind::Absolute)
                            .left(px(x - LABEL_BOX / 2.0))
                            .top(px(y - LABEL_BOX / 2.0))
                            .width(px(LABEL_BOX))
                            .height(px(LABEL_BOX)),
                    ) {
                        Text(value: label.text.clone(), style: style)
                    }
                }
            },
        )
    }
}

/// Hour and minute text fields of input mode.
#[component]
fn time_input(
    selected: RwSignal<TimeOfDay>,
    hour_format: HourFormat,
    hour_text: RwSignal<String>,
    minute_text: RwSignal<String>,
    invalid: RwSignal<bool>,
    hour_caption: Signal<String>,
    minute_caption: Signal<String>,
    invalid_text: Signal<String>,
    am_text: Signal<String>,
    pm_text: Signal<String>,
) -> Element {
    let palette = palette();
    let field_width = if hour_format.is_24_hour() {
        114.0
    } else {
        96.0
    };
    // Keeps `selected` in step with valid input so the AM/PM switch and OK see it.
    let sync = move |_: String| {
        let time = parse_input(
            &hour_text.get_untracked(),
            &minute_text.get_untracked(),
            hour_format,
            selected.get_untracked().is_pm(),
        );
        if let Some(time) = time {
            selected.set(time);
            invalid.set(false);
        }
    };
    let hour_focused = signal(false);
    let minute_focused = signal(false);
    let field_style = move |focused: RwSignal<bool>| {
        computed(move || {
            let border = if invalid.get() {
                palette.error
            } else if focused.get() {
                palette.primary
            } else {
                Color::Transparent
            };
            text_with(TextKind::DisplayMedium, palette.text)
                .text_align(TextAlign::Center)
                .width(px(field_width))
                .height(px(72))
                .border_radius(px(shape().small))
                .border(Border::new().width(px(2)).solid().color(border))
                .background_color(palette.surface_container_highest)
        })
    };
    let caption = move || text_with(TextKind::BodySmall, palette.sub_text).margin_top(px(7));
    let caret: Signal<String> = css_color(palette.primary).into();
    let placeholder: Signal<String> = css_color(palette.text.with_alpha(0.36)).into();
    let selection: Signal<String> = css_color(palette.primary.with_alpha(0.4)).into();
    render! {
        View(style: column().width(percent(100)).padding_bottom(px(12))) {
            View(style: row().align_items(AlignItems::FlexStart)) {
                View(style: column()) {
                    Input(
                        text: hour_text,
                        on_input: sync,
                        on_focus: move |_| hour_focused.set(true),
                        on_blur: move |_| hour_focused.set(false),
                        max_length: 2u32,
                        keyboard_type: KeyboardType::Number,
                        auto_focus: true,
                        caret_color: caret,
                        placeholder_color: placeholder,
                        selection_color: selection,
                        style: field_style(hour_focused),
                    )
                    Text(value: hour_caption, style: caption())
                }
                View(style: center().width(px(24)).height(px(72))) {
                    Text(value: ":", style: text_with(TextKind::DisplayMedium, palette.text))
                }
                View(style: column()) {
                    Input(
                        text: minute_text,
                        on_input: sync,
                        on_focus: move |_| minute_focused.set(true),
                        on_blur: move |_| minute_focused.set(false),
                        max_length: 2u32,
                        keyboard_type: KeyboardType::Number,
                        caret_color: caret,
                        placeholder_color: placeholder,
                        selection_color: selection,
                        style: field_style(minute_focused),
                    )
                    Text(value: minute_caption, style: caption())
                }
                Show(when: move || !hour_format.is_24_hour()) {
                    DayPeriodToggle(
                        selected: selected,
                        height: 72.0,
                        am_text: am_text,
                        pm_text: pm_text,
                    )
                }
            }
            Show(when: move || invalid.get()) {
                Text(
                    value: invalid_text,
                    style: text_with(TextKind::BodySmall, palette.error).margin_top(px(8)),
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn hour_format_matches_flutter() {
        let time = TimeOfDay::new(8, 5);
        assert_eq!(HourFormat::Hour24Padded.format(time), "08");
        assert_eq!(HourFormat::Hour24.format(time), "8");
        assert_eq!(HourFormat::Hour12.format(TimeOfDay::new(0, 0)), "12");
        assert_eq!(HourFormat::Hour12.format(TimeOfDay::new(13, 0)), "1");
    }

    #[test]
    fn fraction_from_point_runs_clockwise_from_the_top() {
        assert!(close(fraction_from_point(0.0, -10.0), 0.0));
        assert!(close(fraction_from_point(10.0, 0.0), 0.25));
        assert!(close(fraction_from_point(0.0, 10.0), 0.5));
        assert!(close(fraction_from_point(-10.0, 0.0), 0.75));
    }

    #[test]
    fn dial_point_round_trips() {
        for fraction in [0.0, 0.1, 0.25, 0.5, 0.9] {
            let (x, y) = dial_point(fraction, LABEL_RADIUS);
            let back = fraction_from_point(x - DIAL_CENTER, y - DIAL_CENTER);
            assert!(close(back, fraction), "{fraction} -> {back}");
        }
    }

    #[test]
    fn twenty_four_hour_rings() {
        let base = TimeOfDay::new(8, 30);
        // 3 o'clock on the outer ring is 3, on the inner ring 15.
        let outer = time_for_fraction(
            base,
            Mode::Hour,
            true,
            0.25,
            ring_from_distance(LABEL_RADIUS),
            false,
        );
        let inner = time_for_fraction(
            base,
            Mode::Hour,
            true,
            0.25,
            ring_from_distance(INNER_RADIUS),
            false,
        );
        assert_eq!(outer, TimeOfDay::new(3, 30));
        assert_eq!(inner, TimeOfDay::new(15, 30));
        // 12 o'clock: 00 outside, 12 inside.
        assert_eq!(
            time_for_fraction(base, Mode::Hour, true, 0.0, 1.0, false).hour,
            0
        );
        assert_eq!(
            time_for_fraction(base, Mode::Hour, true, 0.0, 0.0, false).hour,
            12
        );
        assert_eq!(dial_ring(TimeOfDay::new(15, 0), Mode::Hour, true), 0.0);
        assert_eq!(dial_ring(TimeOfDay::new(15, 0), Mode::Minute, true), 1.0);
    }

    #[test]
    fn twelve_hour_keeps_the_period() {
        let pm = TimeOfDay::new(20, 0);
        assert_eq!(
            time_for_fraction(pm, Mode::Hour, false, 0.25, 0.0, false).hour,
            15
        );
        let am = TimeOfDay::new(8, 0);
        assert_eq!(
            time_for_fraction(am, Mode::Hour, false, 0.0, 1.0, false).hour,
            0
        );
    }

    #[test]
    fn minutes_round_to_five_on_tap_only() {
        let base = TimeOfDay::new(8, 0);
        let fraction = 7.0 / 60.0;
        assert_eq!(
            time_for_fraction(base, Mode::Minute, true, fraction, 1.0, false).minute,
            7
        );
        assert_eq!(
            time_for_fraction(base, Mode::Minute, true, fraction, 1.0, true).minute,
            5
        );
        let fraction = 58.0 / 60.0;
        assert_eq!(
            time_for_fraction(base, Mode::Minute, true, fraction, 1.0, true).minute,
            0
        );
    }

    #[test]
    fn lerp_takes_the_short_way() {
        // Compare on the circle: 0.0 and 0.99999 are the same position.
        let same = |a: f32, b: f32| {
            let d = (a - b).rem_euclid(1.0);
            d.min(1.0 - d) < 1e-4
        };
        assert!(same(lerp_fraction(0.9, 0.1, 0.5), 0.0));
        assert!(same(lerp_fraction(0.1, 0.9, 0.5), 0.0));
        assert!(same(lerp_fraction(0.2, 0.4, 0.5), 0.3));
    }

    #[test]
    fn labels_follow_material3() {
        let hours = dial_labels(Mode::Hour, true);
        assert_eq!(hours.len(), 24);
        assert_eq!(hours[0].text, "00");
        assert_eq!(hours[1].text, "1");
        assert_eq!(hours[12].radius, INNER_RADIUS);
        let twelve = dial_labels(Mode::Hour, false);
        assert_eq!(twelve[0].text, "12");
        let minutes = dial_labels(Mode::Minute, true);
        assert_eq!(minutes[1].text, "05");
        assert!(close(minutes[3].fraction, 0.25));
    }

    #[test]
    fn parse_input_validates_ranges() {
        assert_eq!(
            parse_input("8", "05", HourFormat::Hour24, false),
            Some(TimeOfDay::new(8, 5))
        );
        assert_eq!(
            parse_input("23", "59", HourFormat::Hour24Padded, false),
            Some(TimeOfDay::new(23, 59))
        );
        assert_eq!(parse_input("24", "00", HourFormat::Hour24, false), None);
        assert_eq!(parse_input("8", "60", HourFormat::Hour24, false), None);
        assert_eq!(parse_input("", "00", HourFormat::Hour24, false), None);
        assert_eq!(parse_input("1a", "00", HourFormat::Hour24, false), None);
        assert_eq!(
            parse_input("12", "00", HourFormat::Hour12, false),
            Some(TimeOfDay::new(0, 0))
        );
        assert_eq!(
            parse_input("12", "00", HourFormat::Hour12, true),
            Some(TimeOfDay::new(12, 0))
        );
        assert_eq!(parse_input("0", "00", HourFormat::Hour12, true), None);
    }
}
