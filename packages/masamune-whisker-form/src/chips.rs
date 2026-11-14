//! Lists of short texts shown as chips (katana_form's `FormChipsField` and
//! `FormMapTagDropdownField`).

use masamune_whisker_appearance::{TextKind, text_with};
use masamune_whisker_ui::Slot;
use masamune_whisker_ui::style::css_color;
use whisker::css::{AlignItems, FlexWrap};
use whisker::prelude::*;
use whisker_icons::{Icon, lucide};
use whisker_input::{AutoCapitalize, Input, KeyboardType, ReturnKey};

use crate::controller::{FormField, bind_field, check};
use crate::frame::{FormFieldFrame, PickedValue};
use crate::select_sheet::SelectSheet;
use crate::style::{FormStyle, resolve};

/// Adds the trimmed `text` unless it is empty, already present or the list is full.
pub(crate) fn add_chip(chips: &mut Vec<String>, text: &str, max: Option<usize>) -> bool {
    let text = text.trim();
    if text.is_empty() || chips.iter().any(|c| c == text) || max.is_some_and(|m| chips.len() >= m) {
        return false;
    }
    chips.push(text.to_owned());
    true
}

/// Removes `chip`; returns whether it was there.
pub(crate) fn remove_chip(chips: &mut Vec<String>, chip: &str) -> bool {
    let before = chips.len();
    chips.retain(|c| c != chip);
    chips.len() != before
}

/// Adds `key` when absent (and the list is not full) or removes it when present.
pub(crate) fn toggle_chip(chips: &mut Vec<String>, key: &str, max: Option<usize>) -> bool {
    if remove_chip(chips, key) {
        return true;
    }
    add_chip(chips, key, max)
}

/// Suggestions that contain `text` (ignoring case) and are not chips yet, in their order.
/// Nothing is suggested while `text` is empty.
pub(crate) fn matching_suggestions(
    suggestions: &[String],
    chips: &[String],
    text: &str,
) -> Vec<String> {
    let text = text.trim().to_lowercase();
    if text.is_empty() {
        return Vec::new();
    }
    suggestions
        .iter()
        .filter(|s| s.to_lowercase().contains(&text) && !chips.contains(s))
        .cloned()
        .collect()
}

/// One chip: a pill with the text and, when `on_remove` is given, an X that removes it without
/// tapping what is behind the chip.
#[component]
fn chip(
    label: String,
    on_tap: Option<Callback<()>>,
    on_remove: Option<Callback<()>>,
    style: FormStyle,
) -> Element {
    let has_remove = on_remove.is_some();
    let icon_color = StoredValue::new(css_color(style.text_color));
    let accessibility = Accessibility::new()
        .label(label.clone())
        .role(AccessibilityRole::Button);
    let label = StoredValue::new(label.clone());
    render! {
        View(
            style: css!(
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                gap: px(4),
                padding: (px(4), px(if has_remove { 8 } else { 12 }), px(4), px(12)),
                border_radius: px(999),
                background_color: style.selected_color,
            ),
            accessibility: accessibility,
            on_tap_catch: move |_| {
                if let Some(on_tap) = on_tap {
                    on_tap.run(());
                }
            },
        ) {
            Text(value: label.get(), style: text_with(TextKind::BodyMedium, style.text_color))
            Show(when: move || has_remove) {
                View(
                    style: css!(display: Display::Flex, padding: px(2)),
                    accessibility: Accessibility::new()
                        .label("Remove")
                        .role(AccessibilityRole::Button),
                    on_tap_catch: move |_| {
                        if let Some(on_remove) = on_remove {
                            on_remove.run(());
                        }
                    },
                ) {
                    Icon(svg: lucide::X, color: icon_color.get(), size: "16")
                }
            }
        }
    }
}

/// A wrapping row of chips for `chips`.
#[component]
fn chip_row(
    chips: Signal<Vec<String>>,
    /// Label of a chip (map fields show labels for keys).
    label_of: Callback<String, String>,
    on_tap: Option<Callback<String>>,
    on_remove: Option<Callback<String>>,
    style: FormStyle,
) -> Element {
    render! {
        View(
            style: css!(
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                flex_wrap: FlexWrap::Wrap,
                align_items: AlignItems::Center,
                gap: px(8),
            ),
        ) {
            ForEach(
                each: move || chips.get(),
                key: |chip: &String| chip.clone(),
                children: move |chip: ReadSignal<String>| {
                    let chip = chip.get_untracked();
                    let tapped = chip.clone();
                    let removed = chip.clone();
                    render! {
                        Chip(
                            label: label_of.run(chip.clone()),
                            on_tap_option: on_tap.map(|f| {
                                let tapped = tapped.clone();
                                Callback::new(move |()| f.run(tapped.clone()))
                            }),
                            on_remove_option: on_remove.map(|f| {
                                let removed = removed.clone();
                                Callback::new(move |()| f.run(removed.clone()))
                            }),
                            style: style,
                        )
                    }
                },
            )
        }
    }
}

/// Free text entered as chips (katana_form's `FormChipsField`).
///
/// Typing and pressing return adds the text as a chip (trimmed, no duplicates, at most
/// `max_chips`); each chip's X removes it. `suggestion` lists texts that are offered as chips
/// under the input while they contain the typed text. The value is the list of chips;
/// `empty_error_text` rejects an empty list. With `include_editing_text_on_save` the text still
/// being typed is saved as a chip too.
#[component]
pub fn form_chips_field(
    /// The form this field belongs to and its `on_saved` (see [`crate::FormController::field`]).
    form: Option<FormField<Vec<String>>>,
    /// Two-way bound chips. Without it the field keeps its own chips, starting at
    /// `initial_value`.
    value: Option<RwSignal<Vec<String>>>,
    /// Initial chips, also restored by `FormController::reset`.
    #[prop(default = Vec::new())]
    initial_value: Vec<String>,
    /// Texts offered while typing.
    #[prop(default = Signal::from(Vec::new()))]
    suggestion: Signal<Vec<String>>,
    /// Maximum number of chips; `None` for no limit.
    #[prop(default = None)]
    max_chips: Option<usize>,
    /// Adds the text being typed when the form is saved.
    #[prop(default = false)]
    include_editing_text_on_save: bool,
    /// Label on the frame (katana_form's `labelText`).
    label: Option<Signal<String>>,
    /// Placeholder of the input (katana_form's `hintText`).
    hint: Option<Signal<String>>,
    /// Error shown when there is no chip while validating.
    empty_error_text: Option<Signal<String>>,
    /// Returns an error message for invalid chips, or `None`.
    validator: Option<Callback<Vec<String>, Option<String>>>,
    /// Called with the chips after every change.
    on_changed: Option<Callback<Vec<String>>>,
    /// Called when a chip is tapped (katana_form's `onChipTapped`).
    on_chip_tapped: Option<Callback<String>>,
    /// On-screen keyboard layout.
    #[prop(default = KeyboardType::Default)]
    keyboard_type: KeyboardType,
    /// `false` disables the field.
    #[prop(default = true)]
    enabled: bool,
    /// Shown before the chips.
    #[prop(default = Slot::default())]
    prefix: Slot,
    /// Shown after the chips.
    #[prop(default = Slot::default())]
    suffix: Slot,
    /// Look of the field; defaults to the provided [`FormStyle`].
    style: Option<FormStyle>,
) -> Element {
    let style = resolve(style);
    let reset_value = match value {
        Some(value) if initial_value.is_empty() => value.get_untracked(),
        _ => initial_value.clone(),
    };
    let chips = value.unwrap_or_else(|| signal(initial_value.clone()));
    let editing = signal(String::new());
    let error = signal(None::<String>);
    let focused = signal(false);
    bind_field(
        form,
        move || {
            let mut list = chips.get_untracked();
            if include_editing_text_on_save {
                add_chip(&mut list, &editing.get_untracked(), max_chips);
            }
            list
        },
        move |v: &Vec<String>| check(v, Vec::is_empty, empty_error_text, validator),
        error,
        move || {
            chips.set(reset_value.clone());
            editing.set(String::new());
        },
    );
    let changed = move || {
        if let Some(on_changed) = on_changed {
            on_changed.run(chips.get_untracked());
        }
    };
    let add = Callback::new(move |text: String| {
        let mut list = chips.get_untracked();
        if add_chip(&mut list, &text, max_chips) {
            chips.set(list);
            editing.set(String::new());
            changed();
        }
    });
    let remove = Callback::new(move |chip: String| {
        let mut list = chips.get_untracked();
        if remove_chip(&mut list, &chip) {
            chips.set(list);
            changed();
        }
    });
    let suggestions =
        computed(move || matching_suggestions(&suggestion.get(), &chips.get(), &editing.get()));
    let full = computed(move || max_chips.is_some_and(|m| chips.get().len() >= m));
    let identity = Callback::new(|chip: String| chip);
    let input_style = style
        .text_style(style.text_color)
        .width(percent(100))
        .height(px(style.line_height()));
    let input_style = StoredValue::new(input_style);
    let placeholder_color = StoredValue::new(css_color(style.hint_color));
    render! {
        FormFieldFrame(
            label_option: label,
            error: error.read_only(),
            disabled: !enabled,
            focused: focused.read_only(),
            prefix: prefix.clone(),
            suffix: suffix.clone(),
            style: style,
        ) {
            View(
                style: css!(
                    display: Display::Flex,
                    flex_direction: FlexDirection::Column,
                    flex_grow: 1.0,
                    min_width: px(0),
                    gap: px(8),
                ),
            ) {
                Show(when: move || !chips.with(Vec::is_empty)) {
                    ChipRow(
                        chips: chips,
                        label_of: identity,
                        on_tap_option: on_chip_tapped,
                        on_remove_option: enabled.then_some(remove),
                        style: style,
                    )
                }
                Show(when: move || enabled && !full.get()) {
                    Input(
                        text: editing,
                        placeholder_option: hint,
                        placeholder_color: placeholder_color.get(),
                        caret_color: css_color(style.cursor_color),
                        keyboard_type: keyboard_type,
                        return_key: ReturnKey::Done,
                        auto_capitalize: AutoCapitalize::None,
                        on_submit: move |text: String| add.run(text),
                        on_focus: move |_| focused.set(true),
                        on_blur: move |_| focused.set(false),
                        style: input_style.get(),
                    )
                }
                Show(when: move || !suggestions.with(Vec::is_empty)) {
                    ChipRow(chips: suggestions, label_of: identity, on_tap: add, style: style)
                }
            }
        }
    }
}

/// Several keys of a map picked from a bottom sheet and shown as chips (katana_form's
/// `FormMapTagDropdownField`).
///
/// `options` are `(key, label)` pairs; the value is the list of picked keys in picking order.
/// Tapping the field opens a multi-select [`SelectSheet`] (at most `max_chips` keys) closed with
/// `done_text`; each chip's X removes its key.
#[component]
pub fn form_map_tag_dropdown_field(
    /// `(key, label)` pairs, in order.
    options: Signal<Vec<(String, String)>>,
    /// The form this field belongs to and its `on_saved` (see [`crate::FormController::field`]).
    form: Option<FormField<Vec<String>>>,
    /// Two-way bound keys. Without it the field keeps its own keys, starting at `initial_value`.
    value: Option<RwSignal<Vec<String>>>,
    /// Initial keys, also restored by `FormController::reset`.
    #[prop(default = Vec::new())]
    initial_value: Vec<String>,
    /// Maximum number of keys; `None` for no limit.
    #[prop(default = None)]
    max_chips: Option<usize>,
    /// Label on the frame (katana_form's `labelText`).
    label: Option<Signal<String>>,
    /// Text shown while nothing is picked (katana_form's `hintText`).
    hint: Option<Signal<String>>,
    /// Error shown when nothing is picked while validating.
    empty_error_text: Option<Signal<String>>,
    /// Returns an error message for invalid keys, or `None`.
    validator: Option<Callback<Vec<String>, Option<String>>>,
    /// Called with the keys after every change.
    on_changed: Option<Callback<Vec<String>>>,
    /// Title of the sheet.
    sheet_title: Option<Signal<String>>,
    /// Label of the button that closes the sheet.
    #[prop(default = Signal::from("Done"))]
    done_text: Signal<String>,
    /// `false` disables the field.
    #[prop(default = Signal::from(true))]
    enabled: Signal<bool>,
    /// Trailing icon; `None` hides it.
    #[prop(default = Some(lucide::ChevronDown))]
    icon: Option<&'static str>,
    /// Shown before the chips.
    #[prop(default = Slot::default())]
    prefix: Slot,
    /// Shown after the chips.
    #[prop(default = Slot::default())]
    suffix: Slot,
    /// Look of the field; defaults to the provided [`FormStyle`].
    style: Option<FormStyle>,
) -> Element {
    let style = resolve(style);
    let (prefix, suffix) = (prefix.clone(), suffix.clone());
    let reset_value = match value {
        Some(value) if initial_value.is_empty() => value.get_untracked(),
        _ => initial_value.clone(),
    };
    let keys = value.unwrap_or_else(|| signal(initial_value.clone()));
    let error = signal(None::<String>);
    let open = signal(false);
    bind_field(
        form,
        move || keys.get_untracked(),
        move |v: &Vec<String>| check(v, Vec::is_empty, empty_error_text, validator),
        error,
        move || keys.set(reset_value.clone()),
    );
    let toggle = move |key: String| {
        let mut list = keys.get_untracked();
        if toggle_chip(&mut list, &key, max_chips) {
            keys.set(list);
            if let Some(on_changed) = on_changed {
                on_changed.run(keys.get_untracked());
            }
        }
    };
    let labels = computed(move || {
        options
            .get()
            .into_iter()
            .map(|(_, label)| label)
            .collect::<Vec<String>>()
    });
    let selected = computed(move || {
        let keys = keys.get();
        options
            .get()
            .iter()
            .enumerate()
            .filter(|(_, (key, _))| keys.contains(key))
            .map(|(i, _)| i)
            .collect::<Vec<usize>>()
    });
    let on_select = Callback::new(move |index: usize| {
        if let Some((key, _)) = options
            .try_get_untracked()
            .and_then(|o| o.get(index).cloned())
        {
            toggle(key);
        }
    });
    let remove = Callback::new(move |key: String| toggle(key));
    let label_of = Callback::new(move |key: String| {
        options
            .get()
            .into_iter()
            .find(|(k, _)| *k == key)
            .map(|(_, label)| label)
            .unwrap_or(key)
    });
    let disabled = computed(move || !enabled.get());
    let empty = computed(move || keys.with(Vec::is_empty));
    let icon_color = StoredValue::new(css_color(style.icon_color));
    let icon_size = StoredValue::new(format!("{}", style.icon_size));
    render! {
        Fragment {
            FormFieldFrame(
                label_option: label,
                error: error.read_only(),
                disabled: disabled,
                prefix: prefix.clone(),
                suffix: suffix.clone(),
                style: style,
                on_tap: Callback::new(move |()| open.set(true)),
            ) {
                Show(
                    when: move || empty.get(),
                    fallback: move || render! {
                        Fragment {
                            View(
                                style: css!(display: Display::Flex, flex_grow: 1.0, min_width: px(0)),
                            ) {
                                ChipRow(
                                    chips: keys,
                                    label_of: label_of,
                                    on_remove_option: (!disabled.get_untracked()).then_some(remove),
                                    style: style,
                                )
                            }
                            Show(when: move || icon.is_some()) {
                                Icon(
                                    svg: icon.unwrap_or_default(),
                                    color: icon_color.get(),
                                    size: icon_size.get(),
                                )
                            }
                        }
                    },
                ) {
                    PickedValue(
                        value: String::new(),
                        hint: hint.unwrap_or_default(),
                        icon_option: icon,
                        disabled: disabled,
                        style: style,
                    )
                }
            }
            SelectSheet(
                open: open,
                options: labels,
                multi_selected: selected,
                done_text: done_text,
                on_select: on_select,
                title_option: sheet_title,
                style: style,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn adds_trimmed_unique_chips_up_to_the_limit() {
        let mut chips = Vec::new();
        assert!(add_chip(&mut chips, " rust ", Some(2)));
        assert!(!add_chip(&mut chips, "rust", Some(2)));
        assert!(!add_chip(&mut chips, "   ", Some(2)));
        assert!(add_chip(&mut chips, "dart", Some(2)));
        assert!(!add_chip(&mut chips, "swift", Some(2)));
        assert_eq!(chips, list(&["rust", "dart"]));
        assert!(add_chip(&mut chips, "swift", None));
    }

    #[test]
    fn removes_and_toggles_chips() {
        let mut chips = list(&["a", "b"]);
        assert!(remove_chip(&mut chips, "a"));
        assert!(!remove_chip(&mut chips, "a"));
        assert!(toggle_chip(&mut chips, "c", Some(2)));
        assert_eq!(chips, list(&["b", "c"]));
        assert!(!toggle_chip(&mut chips, "d", Some(2)));
        assert!(toggle_chip(&mut chips, "b", Some(2)));
        assert_eq!(chips, list(&["c"]));
    }

    #[test]
    fn suggests_matching_texts_that_are_not_chips() {
        let suggestions = list(&["Rust", "Ruby", "Dart"]);
        assert!(matching_suggestions(&suggestions, &[], "").is_empty());
        assert_eq!(
            matching_suggestions(&suggestions, &[], "ru"),
            list(&["Rust", "Ruby"])
        );
        assert_eq!(
            matching_suggestions(&suggestions, &list(&["Ruby"]), "R"),
            list(&["Rust", "Dart"])
        );
    }
}
