//! Number picked from a range (katana_form's `FormNumModalField`).

use masamune_whisker_ui::Slot;
use whisker::prelude::*;
use whisker_icons::lucide;

use crate::controller::FormField;
use crate::select_field::SelectField;
use crate::style::FormStyle;

/// The numbers from `begin` to `end` (inclusive) every `interval`. Empty when `end < begin`.
pub(crate) fn num_range(begin: i64, end: i64, interval: i64) -> Vec<i64> {
    if end < begin {
        return Vec::new();
    }
    (begin..=end).step_by(interval.max(1) as usize).collect()
}

/// Picks a number between `begin` and `end` from a bottom sheet (katana_form's
/// `FormNumModalField` with `FormNumModalFieldPicker(begin:, end:, interval:, suffix:)`).
///
/// `begin` and `end` may be signals (for example a limit that is loaded later). The number is
/// shown as `"{number}{unit_suffix}"`, or with `format` when given.
#[component]
pub fn form_num_modal_field(
    /// The form this field belongs to and its `on_saved` (see [`crate::FormController::field`]).
    form: Option<FormField<Option<i64>>>,
    /// Two-way bound number. Without it the field keeps its own number, starting at
    /// `initial_value`.
    value: Option<RwSignal<Option<i64>>>,
    /// Initial number, also restored by `FormController::reset`.
    #[prop(default = None)]
    initial_value: Option<i64>,
    /// Smallest option.
    #[prop(default = Signal::from(0))]
    begin: Signal<i64>,
    /// Largest option.
    #[prop(default = Signal::from(100))]
    end: Signal<i64>,
    /// Step between options.
    #[prop(default = 1)]
    interval: i64,
    /// Text after the number (katana_form's picker `suffix`).
    #[prop(default = Signal::from(String::new()))]
    unit_suffix: Signal<String>,
    /// Text of a number; replaces `unit_suffix`.
    format: Option<Callback<i64, String>>,
    /// Label on the frame (katana_form's `labelText`).
    label: Option<Signal<String>>,
    /// Text shown while nothing is picked (katana_form's `hintText`).
    hint: Option<Signal<String>>,
    /// Error shown when nothing is picked while validating.
    empty_error_text: Option<Signal<String>>,
    /// Returns an error message for an invalid number, or `None`.
    validator: Option<Callback<Option<i64>, Option<String>>>,
    /// Called when a number is picked.
    on_changed: Option<Callback<Option<i64>>>,
    /// `false` disables the field.
    #[prop(default = Signal::from(true))]
    enabled: Signal<bool>,
    /// Title of the sheet.
    sheet_title: Option<Signal<String>>,
    /// Trailing icon; `None` hides it.
    #[prop(default = Some(lucide::ChevronDown))]
    icon: Option<&'static str>,
    /// Shown before the number.
    #[prop(default = Slot::default())]
    prefix: Slot,
    /// Shown after the number.
    #[prop(default = Slot::default())]
    suffix: Slot,
    /// Look of the field; defaults to the provided [`FormStyle`].
    style: Option<FormStyle>,
) -> Element {
    let values = computed(move || num_range(begin.get(), end.get(), interval));
    let label_of = Callback::new(move |number: i64| match format {
        Some(format) => format.run(number),
        None => format!("{number}{}", unit_suffix.get()),
    });
    render! {
        SelectField(
            values: values,
            label_of: label_of,
            form_option: form,
            value_option: value,
            initial_value_option: initial_value,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_include_both_ends() {
        assert_eq!(num_range(1, 5, 1), vec![1, 2, 3, 4, 5]);
        assert_eq!(num_range(0, 10, 5), vec![0, 5, 10]);
        assert_eq!(num_range(0, 9, 5), vec![0, 5]);
        assert_eq!(num_range(3, 3, 0), vec![3]);
        assert!(num_range(1, 0, 1).is_empty());
    }
}
