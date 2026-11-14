//! Fields that pick one option from a bottom sheet (katana_form's `FormEnumModalField`,
//! `FormMapModalField`, `FormEnumDropdownField` and `FormMapDropdownField`).
//!
//! Whisker has no anchored dropdown menu, so the dropdown fields are the same widgets as the
//! modal ones: [`FormEnumDropdownField`] is [`FormEnumModalField`] and
//! [`FormMapDropdownField`] is [`FormMapModalField`].

use masamune_whisker_ui::Slot;
use whisker::prelude::*;
use whisker_icons::lucide;

use crate::controller::{FormField, bind_field, check};
use crate::frame::{FormFieldFrame, PickedValue};
use crate::select_sheet::SelectSheet;
use crate::style::{FormStyle, resolve};

/// Field that shows the selected value and opens a [`SelectSheet`] with `values` when tapped.
/// Shared by every select field of this crate.
#[component]
pub(crate) fn select_field<T: Clone + PartialEq + 'static>(
    values: Signal<Vec<T>>,
    label_of: Callback<T, String>,
    form: Option<FormField<Option<T>>>,
    value: Option<RwSignal<Option<T>>>,
    initial_value: Option<T>,
    label: Option<Signal<String>>,
    hint: Option<Signal<String>>,
    empty_error_text: Option<Signal<String>>,
    validator: Option<Callback<Option<T>, Option<String>>>,
    on_changed: Option<Callback<Option<T>>>,
    enabled: Signal<bool>,
    sheet_title: Option<Signal<String>>,
    icon: Option<&'static str>,
    prefix: Slot,
    suffix: Slot,
    style: Option<FormStyle>,
) -> Element {
    let style = resolve(style);
    let (prefix, suffix) = (prefix.clone(), suffix.clone());
    let reset_value = match value {
        Some(value) if initial_value.is_none() => value.get_untracked(),
        _ => initial_value.clone(),
    };
    let value = value.unwrap_or_else(|| signal(initial_value.clone()));
    let error = signal(None::<String>);
    let open = signal(false);
    bind_field(
        form,
        move || value.get_untracked(),
        move |v: &Option<T>| check(v, Option::is_none, empty_error_text, validator),
        error,
        move || value.set(reset_value.clone()),
    );
    let shown = computed(move || value.get().map(|v| label_of.run(v)).unwrap_or_default());
    let options = computed(move || {
        values
            .get()
            .into_iter()
            .map(|v| label_of.run(v))
            .collect::<Vec<String>>()
    });
    let selected = computed(move || {
        let current = value.get()?;
        values.get().iter().position(|v| *v == current)
    });
    let on_select = Callback::new(move |index: usize| {
        let Some(picked) = values
            .try_get_untracked()
            .and_then(|v| v.get(index).cloned())
        else {
            return;
        };
        value.set(Some(picked.clone()));
        if let Some(on_changed) = on_changed {
            on_changed.run(Some(picked));
        }
    });
    let disabled = computed(move || !enabled.get());
    let accessibility = computed(move || {
        let text = shown.get();
        let text = if text.is_empty() {
            hint.and_then(|h| h.try_get()).unwrap_or_default()
        } else {
            text
        };
        let text = match label.and_then(|l| l.try_get()) {
            Some(label) if !label.is_empty() => format!("{label}: {text}"),
            _ => text,
        };
        Accessibility::new()
            .label(text)
            .role(AccessibilityRole::Button)
            .state(AccessibilityState::new().disabled(disabled.get()))
    });
    render! {
        Fragment {
            FormFieldFrame(
                label_option: label,
                error: error.read_only(),
                disabled: disabled,
                prefix: prefix.clone(),
                suffix: suffix.clone(),
                style: style,
                accessibility: accessibility,
                on_tap: Callback::new(move |()| open.set(true)),
            ) {
                PickedValue(
                    value: shown,
                    hint: hint.unwrap_or_default(),
                    icon_option: icon,
                    disabled: disabled,
                    style: style,
                )
            }
            SelectSheet(
                open: open,
                options: options,
                selected: selected,
                on_select: on_select,
                title_option: sheet_title,
                style: style,
            )
        }
    }
}

/// Picks one of `values` from a bottom sheet (katana_form's `FormEnumModalField`).
///
/// `label_builder` turns a value into the text of the field and of its option (katana_form's
/// `FormEnumModalFieldPicker(values:, labelBuilder:)`). The value is `None` until something is
/// picked; `empty_error_text` rejects that when the form validates.
#[component]
pub fn form_enum_modal_field<T: Clone + PartialEq + 'static>(
    /// The options, in order.
    values: Vec<T>,
    /// Text of a value.
    label_builder: Callback<T, String>,
    /// The form this field belongs to and its `on_saved` (see [`crate::FormController::field`]).
    form: Option<FormField<Option<T>>>,
    /// Two-way bound value. Without it the field keeps its own value, starting at
    /// `initial_value`.
    value: Option<RwSignal<Option<T>>>,
    /// Initial value, also restored by `FormController::reset`.
    #[prop(default = None)]
    initial_value: Option<T>,
    /// Label on the frame (katana_form's `labelText`).
    label: Option<Signal<String>>,
    /// Text shown while nothing is picked (katana_form's `hintText`).
    hint: Option<Signal<String>>,
    /// Error shown when nothing is picked while validating.
    empty_error_text: Option<Signal<String>>,
    /// Returns an error message for an invalid value, or `None`.
    validator: Option<Callback<Option<T>, Option<String>>>,
    /// Called when a value is picked.
    on_changed: Option<Callback<Option<T>>>,
    /// `false` disables the field.
    #[prop(default = Signal::from(true))]
    enabled: Signal<bool>,
    /// Title of the sheet.
    sheet_title: Option<Signal<String>>,
    /// Trailing icon (katana_form's `dropdownIcon`); `None` hides it (`showDropdownIcon: false`).
    #[prop(default = Some(lucide::ChevronDown))]
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
    let values = StoredValue::new(values.clone());
    render! {
        SelectField(
            values: computed(move || values.get()),
            label_of: label_builder,
            form_option: form,
            value_option: value,
            initial_value_option: initial_value.clone(),
            label_option: label,
            hint_option: hint,
            empty_error_text_option: empty_error_text,
            validator_option: validator,
            on_changed_option: on_changed,
            enabled: enabled,
            sheet_title_option: sheet_title,
            icon_option: icon,
            prefix: prefix.clone(),
            suffix: suffix.clone(),
            style_option: style,
        )
    }
}

/// Picks one key of `options` from a bottom sheet (katana_form's `FormMapModalField`).
///
/// `options` are `(key, label)` pairs in display order; the value is the key.
#[component]
pub fn form_map_modal_field(
    /// `(key, label)` pairs, in order.
    options: Signal<Vec<(String, String)>>,
    /// The form this field belongs to and its `on_saved` (see [`crate::FormController::field`]).
    form: Option<FormField<Option<String>>>,
    /// Two-way bound key. Without it the field keeps its own key, starting at `initial_value`.
    value: Option<RwSignal<Option<String>>>,
    /// Initial key, also restored by `FormController::reset`.
    #[prop(default = None)]
    initial_value: Option<String>,
    /// Label on the frame (katana_form's `labelText`).
    label: Option<Signal<String>>,
    /// Text shown while nothing is picked (katana_form's `hintText`).
    hint: Option<Signal<String>>,
    /// Error shown when nothing is picked while validating.
    empty_error_text: Option<Signal<String>>,
    /// Returns an error message for an invalid key, or `None`.
    validator: Option<Callback<Option<String>, Option<String>>>,
    /// Called when a key is picked.
    on_changed: Option<Callback<Option<String>>>,
    /// `false` disables the field.
    #[prop(default = Signal::from(true))]
    enabled: Signal<bool>,
    /// Title of the sheet.
    sheet_title: Option<Signal<String>>,
    /// Trailing icon (katana_form's `dropdownIcon`); `None` hides it.
    #[prop(default = Some(lucide::ChevronDown))]
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
    let keys = computed(move || {
        options
            .get()
            .into_iter()
            .map(|(key, _)| key)
            .collect::<Vec<String>>()
    });
    let label_of = Callback::new(move |key: String| {
        options
            .get()
            .into_iter()
            .find(|(k, _)| *k == key)
            .map(|(_, label)| label)
            .unwrap_or(key)
    });
    render! {
        SelectField(
            values: keys,
            label_of: label_of,
            form_option: form,
            value_option: value,
            initial_value_option: initial_value.clone(),
            label_option: label,
            hint_option: hint,
            empty_error_text_option: empty_error_text,
            validator_option: validator,
            on_changed_option: on_changed,
            enabled: enabled,
            sheet_title_option: sheet_title,
            icon_option: icon,
            prefix: prefix.clone(),
            suffix: suffix.clone(),
            style_option: style,
        )
    }
}

/// katana_form's `FormEnumDropdownField`: the same widget as [`FormEnumModalField`].
pub use FormEnumModalField as FormEnumDropdownField;

/// katana_form's `FormMapDropdownField`: the same widget as [`FormMapModalField`].
pub use FormMapModalField as FormMapDropdownField;
