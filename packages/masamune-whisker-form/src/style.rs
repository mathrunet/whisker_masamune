//! Shared look of the form fields (katana_form's `FormStyle` and `FormStyleScope`).
//!
//! Provide one [`FormStyle`] for the app (or a subtree) with [`provide_form_style`]; every field
//! reads it with [`use_form_style`] unless it gets its own `style` prop. Without a provided style
//! the fields use [`FormStyle::from_theme`], built from the `masamune-whisker-appearance` palette.

use masamune_whisker_appearance::{ColorExt, TextKind, palette, shape};
use whisker::css::{Color, FontWeight};
use whisker::prelude::*;

/// How the field frame is drawn (katana_form's `FormInputBorderStyle`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FormInputBorderStyle {
    /// No border; only the background.
    None,
    /// A border around the field with rounded corners.
    #[default]
    Outline,
    /// A line under the field.
    Underline,
}

/// Where a field's label is shown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FormLabelPosition {
    /// On the top border of the frame (Material's floating label of an outlined field).
    #[default]
    Floating,
    /// Above the frame.
    Top,
}

/// Look of the form fields (katana_form's `FormStyle`).
///
/// Paddings are `(vertical, horizontal)` in logical pixels. Start from
/// [`FormStyle::from_theme`] and override fields with struct update syntax.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FormStyle {
    /// How the frame is drawn.
    pub border_style: FormInputBorderStyle,
    /// Border width.
    pub border_width: f32,
    /// Border color.
    pub border_color: Color,
    /// Border color while the field has focus; `None` keeps `border_color`.
    pub focused_border_color: Option<Color>,
    /// Border, label and error text color of a field with an error.
    pub error_color: Color,
    /// Corner radius of an outlined frame.
    pub radius: f32,
    /// Space around the frame (the error text is inside it too).
    pub padding: (f32, f32),
    /// Space between the frame and its content.
    pub content_padding: (f32, f32),
    /// Minimum height of the frame (including `content_padding`).
    pub min_height: Option<f32>,
    /// Space between the prefix, the content and the suffix.
    pub gap: f32,
    /// Background of the frame.
    pub background_color: Color,
    /// Text role of the value. Text fields size themselves from its line height.
    pub text_kind: TextKind,
    /// Font weight of the value; `None` uses the weight of `text_kind`.
    pub text_weight: Option<u16>,
    /// Color of the value.
    pub text_color: Color,
    /// Color of the hint (placeholder).
    pub hint_color: Color,
    /// Cursor color.
    pub cursor_color: Color,
    /// Text and hint color of a disabled field.
    pub disabled_color: Color,
    /// Opacity of a disabled field.
    pub disabled_opacity: f32,
    /// Color of the dropdown and calendar icons.
    pub icon_color: Color,
    /// Size of the dropdown and calendar icons.
    pub icon_size: f32,
    /// Where the label is shown.
    pub label_position: FormLabelPosition,
    /// Text role of the label.
    pub label_kind: TextKind,
    /// Color of the label.
    pub label_color: Color,
    /// Text role of the error text under the frame.
    pub error_kind: TextKind,
    /// Space above and beside the error text.
    pub error_padding: (f32, f32),
    /// Text role of the options in a select sheet.
    pub option_kind: TextKind,
    /// Space inside each option row of a select sheet.
    pub option_padding: (f32, f32),
    /// Minimum height of an option row.
    pub option_min_height: f32,
    /// Corner radius of an option row.
    pub option_radius: f32,
    /// Background of the selected option.
    pub selected_color: Color,
    /// Accent color (checked checkboxes, switches, the selected date in the date sheet).
    pub active_color: Color,
}

impl FormStyle {
    /// Outlined fields in the colors and text styles of the current theme.
    pub fn from_theme() -> Self {
        let palette = palette();
        Self {
            border_style: FormInputBorderStyle::Outline,
            border_width: 1.0,
            border_color: palette.outline,
            focused_border_color: Some(palette.primary),
            error_color: palette.error,
            radius: shape().extra_small,
            padding: (0.0, 0.0),
            content_padding: (12.0, 16.0),
            min_height: None,
            gap: 8.0,
            background_color: palette.surface,
            text_kind: TextKind::BodyLarge,
            text_weight: None,
            text_color: palette.text,
            hint_color: palette.disabled,
            cursor_color: palette.text,
            disabled_color: palette.disabled,
            disabled_opacity: 0.5,
            icon_color: palette.text,
            icon_size: 24.0,
            label_position: FormLabelPosition::Floating,
            label_kind: TextKind::BodySmall,
            label_color: palette.sub_text,
            error_kind: TextKind::BodySmall,
            error_padding: (4.0, 16.0),
            option_kind: TextKind::BodyLarge,
            option_padding: (12.0, 16.0),
            option_min_height: 48.0,
            option_radius: shape().extra_small,
            selected_color: palette.primary.with_alpha(0.1),
            active_color: palette.primary,
        }
    }

    /// Line height of the value text in logical pixels.
    pub fn line_height(&self) -> f32 {
        self.text_kind.line_height()
    }

    /// Text style of the value in `color`.
    pub fn text_style(&self, color: Color) -> Css {
        let css = masamune_whisker_appearance::text_with(self.text_kind, color);
        match self.text_weight {
            Some(weight) => css.font_weight(FontWeight::Numeric(weight)),
            None => css,
        }
    }
}

impl Default for FormStyle {
    fn default() -> Self {
        Self::from_theme()
    }
}

/// Provides `style` to the fields below the current component (katana_form's
/// `FormStyleScope`). Provide it inside `AppRoot` so a theme change rebuilds it.
pub fn provide_form_style(style: FormStyle) {
    provide_context(style);
}

/// The nearest provided [`FormStyle`], or [`FormStyle::from_theme`].
pub fn use_form_style() -> FormStyle {
    use_context::<FormStyle>().unwrap_or_else(FormStyle::from_theme)
}

/// `style`, or the provided one.
pub(crate) fn resolve(style: Option<FormStyle>) -> FormStyle {
    style.unwrap_or_else(use_form_style)
}
