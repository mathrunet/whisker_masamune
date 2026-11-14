//! Form widgets for Whisker apps: the Whisker version of Masamune's `katana_form`.
//!
//! A [`FormController`] holds the form value; fields bound to it with
//! [`FormController::field`] validate together and hand their values to `on_saved` when
//! [`FormController::validate`] passes. Every field draws its frame from one [`FormStyle`],
//! provided once with [`provide_form_style`]. Text fields size themselves from the line height
//! of [`FormStyle::text_kind`], so callers never set heights.
//!
//! | katana_form | masamune-whisker-form |
//! |---|---|
//! | `FormController` | [`FormController`] (`field(on_saved)`, `validate`, `reset`, `value`, `set_value`) |
//! | `form:` + `onSaved:` | `form: form.field(move \|v\| …)` ([`FormField`]); `form: form` validates only |
//! | `FormStyle` / `FormStyleScope` | [`FormStyle`] / [`provide_form_style`], [`use_form_style`] |
//! | `FormInputBorderStyle` | [`FormInputBorderStyle`] |
//! | `InputDecoration` (label, prefix, suffix, error) | [`FormFieldFrame`] |
//! | `FormTextField` | [`FormTextField`] |
//! | `FormEnumModalField` / `FormEnumDropdownField` | [`FormEnumModalField`] / [`FormEnumDropdownField`] (same widget) |
//! | `FormMapModalField` / `FormMapDropdownField` | [`FormMapModalField`] / [`FormMapDropdownField`] (same widget) |
//! | `FormNumModalField` | [`FormNumModalField`] |
//! | `FormDateField` | [`FormDateField`] (year / month / day columns) |
//! | `FormDateTimeField` | [`FormDateTimeField`] (OS pickers, [`FormDateTimePickerMode`]) |
//! | `FormDateTimeRangeField` | [`FormDateTimeRangeField`] (OS date picker twice, [`FormDateRange`]) |
//! | `FormMonthModalField` | [`FormMonthModalField`] (year / month columns, [`FormMonthDay`]) |
//! | `FormDurationField` | [`FormDurationField`] (day / hour / minute / second columns) |
//! | `FormChipsField` | [`FormChipsField`] (free-text chips, `max_chips`, `suggestion`) |
//! | `FormMapTagDropdownField` | [`FormMapTagDropdownField`] (multi-select [`SelectSheet`], chips) |
//! | `FormMedia` / `FormMultiMedia` | [`FormMedia`] / [`FormMultiMedia`] ([`FormMediaValue`], [`FormMediaType`], [`FormMediaRef`], [`FormMultiMediaRef`]) |
//! | `FormRatingBar` | [`FormRatingBar`] |
//! | `FormPinField` | [`FormPinField`] |
//! | `FormFutureField` | [`FormFutureField`] ([`FormFutureTap`]) |
//! | `FormBuilder` | [`FormBuilder`] ([`FormBuilderRef`]) |
//! | `FormContainer` | [`FormContainer`] |
//! | `FormListBuilder` | [`FormListBuilder`] ([`FormListRef`]) |
//! | `FormCheckbox` / `FormSwitch` | [`FormCheckbox`] / [`FormSwitch`] |
//! | `FormButton` | [`FormButton`] |
//! | `FormLabel` | [`FormLabel`] |
//! | picker of the modal fields | [`SelectSheet`] |
//!
//! Common props follow katana_form: `label` (`labelText`), `hint` (`hintText`),
//! `initial_value`, `empty_error_text`, `validator`, `on_changed`, `enabled`, `prefix`,
//! `suffix` and `style`. Each field also takes a `value` / `text` signal for two-way binding.

pub mod builder;
pub mod button;
pub mod checkbox;
pub mod chips;
pub(crate) mod columns;
pub mod controller;
pub mod date_field;
pub mod date_range_field;
pub mod date_time_field;
pub mod duration_field;
pub mod frame;
pub mod label;
pub mod list_builder;
pub mod media;
pub mod month_field;
pub mod num_field;
pub mod pin_field;
pub mod rating_bar;
pub mod select_field;
pub mod select_sheet;
pub mod style;
pub mod text_field;

pub use builder::{FormBuilder, FormBuilderRef, FormContainer, FormFutureField, FormFutureTap};
pub use button::FormButton;
pub use checkbox::{FormCheckbox, FormSwitch};
pub use chips::{FormChipsField, FormMapTagDropdownField};
pub use controller::{FormController, FormField};
pub use date_field::FormDateField;
pub use date_range_field::{FormDateRange, FormDateTimeRangeField};
pub use date_time_field::{FormDateTimeField, FormDateTimePickerMode};
pub use duration_field::FormDurationField;
pub use frame::FormFieldFrame;
pub use label::FormLabel;
pub use list_builder::{FormListBuilder, FormListRef};
pub use media::{
    FormMedia, FormMediaRef, FormMediaType, FormMediaValue, FormMultiMedia, FormMultiMediaRef,
};
pub use month_field::{FormMonthDay, FormMonthModalField};
pub use num_field::FormNumModalField;
pub use pin_field::FormPinField;
pub use rating_bar::FormRatingBar;
pub use select_field::{
    FormEnumDropdownField, FormEnumModalField, FormMapDropdownField, FormMapModalField,
};
pub use select_sheet::SelectSheet;
pub use style::{
    FormInputBorderStyle, FormLabelPosition, FormStyle, provide_form_style, use_form_style,
};
pub use text_field::FormTextField;
