//! PIN / one-time code input (katana_form's `FormPinField`).

use masamune_whisker_appearance::text_with;
use masamune_whisker_ui::style::fill;
use whisker::css::{AlignItems, Border, JustifyContent, PositionKind, TextAlign};
use whisker::prelude::*;
use whisker_input::{AutoCapitalize, Input, KeyboardType, ReturnKey};

use crate::controller::{FormField, bind_field, check};
use crate::style::{FormStyle, resolve};

/// The digits of `text`, at most `length` of them.
pub(crate) fn pin_digits(text: &str, length: usize) -> String {
    text.chars()
        .filter(char::is_ascii_digit)
        .take(length)
        .collect()
}

/// What each of the `length` boxes shows: its digit (or `mask`), or nothing.
pub(crate) fn pin_cells(text: &str, length: usize, mask: Option<char>) -> Vec<String> {
    let mut chars = text.chars();
    (0..length)
        .map(|_| {
            chars
                .next()
                .map(|c| mask.unwrap_or(c).to_string())
                .unwrap_or_default()
        })
        .collect()
}

/// PIN field: `length` boxes over one transparent numeric input (katana_form's
/// `FormPinField`). Only digits are kept. `on_completed` is called once every box is filled;
/// validation fails while the PIN is shorter than `length` (with `length_error_text`).
#[component]
pub fn form_pin_field(
    /// The form this field belongs to and its `on_saved` (see [`crate::FormController::field`]).
    form: Option<FormField<String>>,
    /// Two-way bound PIN. Without it the field keeps its own PIN, starting at `initial_value`.
    text: Option<RwSignal<String>>,
    /// Initial PIN, also restored by `FormController::reset`.
    #[prop(default = String::new())]
    initial_value: String,
    /// Number of digits.
    #[prop(default = 4)]
    length: usize,
    /// Shows `●` instead of the digits.
    #[prop(default = false)]
    obscure: bool,
    /// Error shown when empty while validating.
    empty_error_text: Option<Signal<String>>,
    /// Error shown when some boxes are empty while validating.
    length_error_text: Option<Signal<String>>,
    /// Returns an error message for an invalid PIN, or `None`.
    validator: Option<Callback<String, Option<String>>>,
    /// Called with the PIN after every change.
    on_changed: Option<Callback<String>>,
    /// Called with the PIN once all `length` digits are entered (katana_form's `onSubmit`).
    on_completed: Option<Callback<String>>,
    /// Focuses the field and raises the keyboard when it is shown.
    #[prop(default = false)]
    auto_focus: bool,
    /// `false` disables the field (katana_form's `readOnly`).
    #[prop(default = true)]
    enabled: bool,
    /// Side of each box.
    #[prop(default = 48.0)]
    box_size: f32,
    /// Space between the boxes.
    #[prop(default = 8.0)]
    gap: f32,
    /// Look of the boxes; defaults to the provided [`FormStyle`].
    style: Option<FormStyle>,
) -> Element {
    let style = resolve(style);
    let reset_value = match text {
        Some(text) if initial_value.is_empty() => text.get_untracked(),
        _ => initial_value.clone(),
    };
    let pin = text.unwrap_or_else(|| signal(pin_digits(&initial_value, length)));
    let error = signal(None::<String>);
    let focused = signal(false);
    bind_field(
        form,
        move || pin.get_untracked(),
        move |v: &String| {
            if !v.is_empty()
                && v.chars().count() < length
                && let Some(text) = length_error_text
            {
                return Some(text.try_get_untracked().unwrap_or_default());
            }
            check(v, String::is_empty, empty_error_text, validator)
        },
        error,
        move || pin.set(reset_value.clone()),
    );
    let on_input = move |value: String| {
        let digits = pin_digits(&value, length);
        if digits != value {
            pin.set(digits.clone());
        }
        error.set(None);
        if let Some(on_changed) = on_changed {
            on_changed.run(digits.clone());
        }
        if digits.chars().count() == length
            && let Some(on_completed) = on_completed
        {
            on_completed.run(digits);
        }
    };
    let mask = obscure.then_some('●');
    let indexes = StoredValue::new((0..length).collect::<Vec<usize>>());
    let transparent: Signal<String> = "#00000000".into();
    let error_value = computed(move || error.get().unwrap_or_default());
    render! {
        View(
            style: css!(
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                gap: px(4),
            ),
        ) {
            View(
                style: css!(
                    position: PositionKind::Relative,
                    display: Display::Flex,
                    flex_direction: FlexDirection::Row,
                    gap: px(gap),
                    opacity: if enabled { 1.0 } else { style.disabled_opacity },
                ),
            ) {
                ForEach(
                    each: move || indexes.get(),
                    key: |i: &usize| *i,
                    children: move |i: ReadSignal<usize>| {
                        let i = i.get_untracked();
                        let cell = computed(move || pin_cells(&pin.get(), length, mask).swap_remove(i));
                        let frame = computed(move || {
                            let current = pin
                                .with(|p| p.chars().count())
                                .min(length.saturating_sub(1));
                            let color = if error.get().is_some() {
                                style.error_color
                            } else if focused.get() && current == i {
                                style.focused_border_color.unwrap_or(style.active_color)
                            } else {
                                style.border_color
                            };
                            css!(
                                display: Display::Flex,
                                align_items: AlignItems::Center,
                                justify_content: JustifyContent::Center,
                                width: px(box_size),
                                height: px(box_size),
                                border_radius: px(style.radius),
                                background_color: style.background_color,
                            )
                            .border(
                                Border::new()
                                    .width(px(style.border_width))
                                    .solid()
                                    .color(color),
                            )
                        });
                        render! {
                            View(style: frame) {
                                Text(
                                    value: cell,
                                    style: style
                                        .text_style(style.text_color)
                                        .text_align(TextAlign::Center),
                                )
                            }
                        }
                    },
                )
                Input(
                    text: pin,
                    on_input: on_input,
                    on_focus: move |_| focused.set(true),
                    on_blur: move |_| focused.set(false),
                    max_length: length as u32,
                    keyboard_type: KeyboardType::Number,
                    return_key: ReturnKey::Done,
                    auto_capitalize: AutoCapitalize::None,
                    autocorrect: false,
                    spell_check: false,
                    editable: enabled,
                    auto_focus: auto_focus,
                    caret_color: transparent,
                    selection_color: transparent,
                    placeholder_color: transparent,
                    style: fill().color(whisker::css::Color::rgba(0, 0, 0, 0.0)),
                )
            }
            Show(when: move || error.get().is_some()) {
                Text(value: error_value, style: text_with(style.error_kind, style.error_color))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_digits_up_to_the_length() {
        assert_eq!(pin_digits("12a3-45", 4), "1234");
        assert_eq!(pin_digits("", 4), "");
    }

    #[test]
    fn cells_show_digits_or_masks() {
        assert_eq!(pin_cells("12", 4, None), vec!["1", "2", "", ""]);
        assert_eq!(pin_cells("12", 3, Some('●')), vec!["●", "●", ""]);
    }
}
