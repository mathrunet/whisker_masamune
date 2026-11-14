//! Checkbox and switch (katana_form's `FormCheckbox` and `FormSwitch`).

use masamune_whisker_appearance::{palette, text_with};
use masamune_whisker_ui::InkWell;
use masamune_whisker_ui::style::css_color;
use whisker::css::{AlignItems, JustifyContent, PositionKind};
use whisker::prelude::*;
use whisker_icons::{Icon, lucide};

use crate::controller::{FormField, bind_field, check};
use crate::style::{FormStyle, resolve};

/// How a boolean field draws its control.
#[derive(Clone, Copy, PartialEq)]
enum Toggle {
    Checkbox,
    Switch,
}

/// The checkbox or the switch track with its thumb.
#[component]
fn toggle_mark(kind: Toggle, checked: RwSignal<bool>, style: FormStyle) -> Element {
    let icon = computed(move || {
        if checked.get() {
            lucide::SquareCheckBig
        } else {
            lucide::Square
        }
        .to_owned()
    });
    let icon_color = computed(move || {
        css_color(if checked.get() {
            style.active_color
        } else {
            style.text_color
        })
    });
    let track = computed(move || {
        css!(
            position: PositionKind::Relative,
            flex_shrink: 0.0,
            width: px(52),
            height: px(32),
            border_radius: px(16),
            background_color: if checked.get() { style.active_color } else { palette().outline },
        )
    });
    let thumb = computed(move || {
        css!(
            position: PositionKind::Absolute,
            top: px(4),
            left: px(if checked.get() { 24 } else { 4 }),
            width: px(24),
            height: px(24),
            border_radius: px(12),
            background_color: palette().surface,
        )
    });
    render! {
        Show(
            when: move || kind == Toggle::Checkbox,
            fallback: move || render! {
                View(style: track) {
                    View(style: thumb)
                }
            },
        ) {
            Icon(svg: icon, color: icon_color, size: "22")
        }
    }
}

/// Shared body of [`FormCheckbox`] and [`FormSwitch`].
#[component]
fn toggle_field(
    kind: Toggle,
    form: Option<FormField<bool>>,
    value: Option<RwSignal<bool>>,
    initial_value: bool,
    label: Signal<String>,
    empty_error_text: Option<Signal<String>>,
    validator: Option<Callback<bool, Option<String>>>,
    on_changed: Option<Callback<bool>>,
    enabled: Signal<bool>,
    justify: JustifyContent,
    style: Option<FormStyle>,
) -> Element {
    let style = resolve(style);
    let reset_value = match value {
        Some(value) if !initial_value => value.get_untracked(),
        _ => initial_value,
    };
    let checked = value.unwrap_or_else(|| signal(initial_value));
    let error = signal(None::<String>);
    bind_field(
        form,
        move || checked.get_untracked(),
        move |v: &bool| check(v, |v| !*v, empty_error_text, validator),
        error,
        move || checked.set(reset_value),
    );
    let disabled = computed(move || !enabled.get());
    let accessibility = computed(move || {
        Accessibility::new()
            .label(label.get())
            .role(match kind {
                Toggle::Checkbox => AccessibilityRole::Checkbox,
                Toggle::Switch => AccessibilityRole::Switch,
            })
            .state(
                AccessibilityState::new()
                    .checked(if checked.get() {
                        whisker::AccessibilityChecked::Checked
                    } else {
                        whisker::AccessibilityChecked::Unchecked
                    })
                    .disabled(disabled.get()),
            )
    });
    let row = computed(move || {
        let css = css!(
            display: Display::Flex,
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            justify_content: justify,
            gap: px(8),
            padding: (px(8), px(0)),
            opacity: if disabled.get() { style.disabled_opacity } else { 1.0 },
        );
        if kind == Toggle::Switch {
            css.justify_content(JustifyContent::SpaceBetween)
        } else {
            css
        }
    });
    let text_style = style.text_style(style.text_color);
    let error_value = computed(move || error.get().unwrap_or_default());
    let error_style = text_with(style.error_kind, style.error_color);
    let switch_first = kind == Toggle::Switch;
    render! {
        View(style: css!(display: Display::Flex, flex_direction: FlexDirection::Column)) {
            InkWell(
                style: row,
                accessibility: accessibility,
                disabled: disabled,
                on_tap: Callback::new(move |()| {
                    let next = !checked.get_untracked();
                    checked.set(next);
                    error.set(None);
                    if let Some(on_changed) = on_changed {
                        on_changed.run(next);
                    }
                }),
            ) {
                Show(when: move || !switch_first) {
                    ToggleMark(kind: kind, checked: checked, style: style)
                }
                Text(value: label, style: text_style.clone())
                Show(when: move || switch_first) {
                    ToggleMark(kind: kind, checked: checked, style: style)
                }
            }
            Show(when: move || error.get().is_some()) {
                Text(
                    value: error_value,
                    style: error_style
                        .clone()
                        .text_align(if justify == JustifyContent::Center {
                            whisker::css::TextAlign::Center
                        } else {
                            whisker::css::TextAlign::Start
                        }),
                )
            }
        }
    }
}

/// Checkbox with a label (katana_form's `FormCheckbox`). Tapping anywhere on the row toggles it.
///
/// `empty_error_text` is shown when it is unchecked while the form validates (for example
/// "Agree to the terms"); the error clears when it is toggled.
#[component]
pub fn form_checkbox(
    /// Text next to the box.
    label: Signal<String>,
    /// The form this field belongs to and its `on_saved` (see [`crate::FormController::field`]).
    form: Option<FormField<bool>>,
    /// Two-way bound state. Without it the field keeps its own state.
    value: Option<RwSignal<bool>>,
    /// Initial state, also restored by `FormController::reset`.
    #[prop(default = false)]
    initial_value: bool,
    /// Error shown when unchecked while validating.
    empty_error_text: Option<Signal<String>>,
    /// Returns an error message for an invalid state, or `None`.
    validator: Option<Callback<bool, Option<String>>>,
    /// Called with the new state.
    on_changed: Option<Callback<bool>>,
    /// `false` disables the checkbox.
    #[prop(default = Signal::from(true))]
    enabled: Signal<bool>,
    /// Horizontal placement of the box and the label.
    #[prop(default = JustifyContent::FlexStart)]
    justify: JustifyContent,
    /// Look of the field; defaults to the provided [`FormStyle`].
    style: Option<FormStyle>,
) -> Element {
    render! {
        ToggleField(
            kind: Toggle::Checkbox,
            form_option: form,
            value_option: value,
            initial_value: initial_value,
            label: label,
            empty_error_text_option: empty_error_text,
            validator_option: validator,
            on_changed_option: on_changed,
            enabled: enabled,
            justify: justify,
            style_option: style,
        )
    }
}

/// Switch with a label (katana_form's `FormSwitch`): the label on the left, the switch on the
/// right. Tapping anywhere on the row toggles it.
#[component]
pub fn form_switch(
    /// Text before the switch.
    label: Signal<String>,
    /// The form this field belongs to and its `on_saved` (see [`crate::FormController::field`]).
    form: Option<FormField<bool>>,
    /// Two-way bound state. Without it the field keeps its own state.
    value: Option<RwSignal<bool>>,
    /// Initial state, also restored by `FormController::reset`.
    #[prop(default = false)]
    initial_value: bool,
    /// Error shown when off while validating.
    empty_error_text: Option<Signal<String>>,
    /// Returns an error message for an invalid state, or `None`.
    validator: Option<Callback<bool, Option<String>>>,
    /// Called with the new state.
    on_changed: Option<Callback<bool>>,
    /// `false` disables the switch.
    #[prop(default = Signal::from(true))]
    enabled: Signal<bool>,
    /// Look of the field; defaults to the provided [`FormStyle`].
    style: Option<FormStyle>,
) -> Element {
    render! {
        ToggleField(
            kind: Toggle::Switch,
            form_option: form,
            value_option: value,
            initial_value: initial_value,
            label: label,
            empty_error_text_option: empty_error_text,
            validator_option: validator,
            on_changed_option: on_changed,
            enabled: enabled,
            justify: JustifyContent::SpaceBetween,
            style_option: style,
        )
    }
}
