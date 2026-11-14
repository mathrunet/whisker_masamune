//! Text field (katana_form's `FormTextField`).

use masamune_whisker_ui::Slot;
use masamune_whisker_ui::style::css_color;
use whisker::prelude::*;
use whisker_input::{AutoCapitalize, Input, InputRef, KeyboardType, ReturnKey};

use crate::controller::{FormField, bind_field, check};
use crate::frame::FormFieldFrame;
use crate::style::{FormStyle, resolve};

/// Height rule of the input, derived from the line height of the style's text.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum InputHeight {
    /// One line: `height = line height`.
    Single(f32),
    /// Fixed number of lines: `height = lines × line height`.
    Fixed { lines: u32, height: f32 },
    /// Grows with the text between `min` and `max` (no upper bound when `max` is `None`).
    Grow { min: f32, max: Option<f32> },
    /// Fills the frame, which fills its parent.
    Expand { min: f32 },
}

impl InputHeight {
    /// `max_lines == 0` means no upper bound.
    pub(crate) fn of(line_height: f32, min_lines: u32, max_lines: u32, expands: bool) -> Self {
        let min_lines = min_lines.max(1);
        if expands {
            return Self::Expand { min: line_height };
        }
        if max_lines == 1 && min_lines == 1 {
            return Self::Single(line_height);
        }
        if max_lines != 0 && max_lines <= min_lines {
            return Self::Fixed {
                lines: min_lines,
                height: line_height * min_lines as f32,
            };
        }
        Self::Grow {
            min: line_height * min_lines as f32,
            max: (max_lines != 0).then_some(line_height * max_lines as f32),
        }
    }

    pub(crate) fn multiline(self) -> bool {
        !matches!(self, Self::Single(_))
    }

    fn apply(self, css: Css) -> Css {
        match self {
            Self::Single(height) | Self::Fixed { height, .. } => css.height(px(height)),
            Self::Grow { min, max } => {
                let css = css.min_height(px(min));
                match max {
                    Some(max) => css.max_height(px(max)),
                    None => css,
                }
            }
            Self::Expand { min } => css.min_height(px(min)).height(percent(100)),
        }
    }
}

/// Text field (katana_form's `FormTextField`).
///
/// One line by default. With `max_lines` above `min_lines` the field starts at `min_lines`
/// lines, grows with the text up to `max_lines` and then scrolls inside; `max_lines: 0` grows
/// without a limit and `expands` fills the parent. The height always follows the line height of
/// [`FormStyle::text_kind`], so callers never set it.
///
/// Bind it to a [`crate::FormController`] with `form`; `empty_error_text` and `validator` run
/// when the form validates and their message is shown under the field.
#[component]
pub fn form_text_field(
    /// The form this field belongs to and its `on_saved` (see [`crate::FormController::field`]).
    form: Option<FormField<String>>,
    /// Two-way bound text. Without it the field keeps its own text, starting at `initial_value`.
    text: Option<RwSignal<String>>,
    /// Initial text (katana_form's `initialValue`), also restored by `FormController::reset`.
    #[prop(default = String::new())]
    initial_value: String,
    /// Label on the frame (katana_form's `labelText`).
    label: Option<Signal<String>>,
    /// Placeholder shown while empty (katana_form's `hintText`).
    hint: Option<Signal<String>>,
    /// Lines shown when empty.
    #[prop(default = 1)]
    min_lines: u32,
    /// Lines the field grows to before it scrolls; `0` grows without a limit.
    #[prop(default = 1)]
    max_lines: u32,
    /// Fills the parent column (multi-line).
    #[prop(default = false)]
    expands: bool,
    /// Hides the text (passwords; katana_form's `obscureText`).
    #[prop(default = false)]
    obscure: bool,
    /// On-screen keyboard layout.
    #[prop(default = KeyboardType::Default)]
    keyboard_type: KeyboardType,
    /// Return-key label / action.
    #[prop(default = ReturnKey::Default)]
    return_key: ReturnKey,
    /// Keyboard auto-capitalization (`None` for emails, ids and URLs).
    #[prop(default = AutoCapitalize::Sentences)]
    auto_capitalize: AutoCapitalize,
    /// Automatic typo replacement and spelling suggestions.
    #[prop(default = true)]
    autocorrect: bool,
    /// Maximum number of characters.
    max_length: Option<u32>,
    /// Error shown when the text is empty or only spaces while validating.
    empty_error_text: Option<Signal<String>>,
    /// Returns an error message for an invalid text, or `None`.
    validator: Option<Callback<String, Option<String>>>,
    /// Called on every change with the whole text.
    on_changed: Option<Callback<String>>,
    /// Called when the return key is pressed.
    on_submit: Option<Callback<String>>,
    /// Called with `true` when the field gains focus and `false` when it loses it.
    on_focus_changed: Option<Callback<bool>>,
    /// `false` disables the field: it cannot be edited and is drawn disabled.
    #[prop(default = true)]
    enabled: bool,
    /// Cannot be edited but keeps the normal look.
    #[prop(default = false)]
    read_only: bool,
    /// Focuses the field and raises the keyboard when it is shown.
    #[prop(default = false)]
    auto_focus: bool,
    /// Shown before the text (for example a currency sign).
    #[prop(default = Slot::default())]
    prefix: Slot,
    /// Shown after the text.
    #[prop(default = Slot::default())]
    suffix: Slot,
    /// Look of the field; defaults to the provided [`FormStyle`].
    style: Option<FormStyle>,
    /// Imperative handle of the input (focus, blur, clear).
    input_ref: Option<InputRef>,
) -> Element {
    let style = resolve(style);
    let reset_value = match text {
        Some(text) if initial_value.is_empty() => text.get_untracked(),
        _ => initial_value.clone(),
    };
    let text = text.unwrap_or_else(|| signal(initial_value.clone()));
    let error = signal(None::<String>);
    let focused = signal(false);
    bind_field(
        form,
        move || text.get_untracked(),
        move |value: &String| check(value, |v| v.trim().is_empty(), empty_error_text, validator),
        error,
        move || text.set(reset_value.clone()),
    );
    let height = InputHeight::of(style.line_height(), min_lines, max_lines, expands);
    let color = if enabled {
        style.text_color
    } else {
        style.disabled_color
    };
    let input_style = height.apply(
        style
            .text_style(color)
            .flex_grow(1.0)
            .flex_basis(px(0))
            .min_width(px(0)),
    );
    let placeholder_color = css_color(if enabled {
        style.hint_color
    } else {
        style.disabled_color
    });
    let lines = match height {
        InputHeight::Fixed { lines, .. } => Some(lines),
        _ => None,
    };
    let auto_size = matches!(height, InputHeight::Grow { .. });
    render! {
        FormFieldFrame(
            label_option: label,
            error: error.read_only(),
            disabled: !enabled,
            focused: focused.read_only(),
            expand: expands,
            prefix: prefix.clone(),
            suffix: suffix.clone(),
            style: style,
        ) {
            Input(
                text: text,
                placeholder_option: hint,
                placeholder_color: placeholder_color.clone(),
                caret_color: css_color(style.cursor_color),
                multiline: height.multiline(),
                auto_size: auto_size,
                lines_option: lines,
                secure: obscure,
                editable: enabled && !read_only,
                auto_focus: auto_focus,
                max_length_option: max_length,
                keyboard_type: keyboard_type,
                return_key: return_key,
                auto_capitalize: auto_capitalize,
                autocorrect: autocorrect,
                spell_check: autocorrect,
                on_input: move |value: String| {
                    if let Some(on_changed) = on_changed {
                        on_changed.run(value);
                    }
                },
                on_submit: move |value: String| {
                    if let Some(on_submit) = on_submit {
                        on_submit.run(value);
                    }
                },
                on_focus: move |_| {
                    focused.set(true);
                    if let Some(on_focus_changed) = on_focus_changed {
                        on_focus_changed.run(true);
                    }
                },
                on_blur: move |_| {
                    focused.set(false);
                    if let Some(on_focus_changed) = on_focus_changed {
                        on_focus_changed.run(false);
                    }
                },
                style: input_style.clone(),
                input_ref_option: input_ref,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heights_follow_the_line_height() {
        assert_eq!(
            InputHeight::of(24.0, 1, 1, false),
            InputHeight::Single(24.0)
        );
        assert_eq!(
            InputHeight::of(24.0, 1, 5, false),
            InputHeight::Grow {
                min: 24.0,
                max: Some(120.0)
            }
        );
        assert_eq!(
            InputHeight::of(20.0, 2, 0, false),
            InputHeight::Grow {
                min: 40.0,
                max: None
            }
        );
        assert_eq!(
            InputHeight::of(20.0, 3, 3, false),
            InputHeight::Fixed {
                lines: 3,
                height: 60.0
            }
        );
        assert_eq!(
            InputHeight::of(24.0, 1, 1, true),
            InputHeight::Expand { min: 24.0 }
        );
        assert!(!InputHeight::of(24.0, 0, 1, false).multiline());
    }
}
