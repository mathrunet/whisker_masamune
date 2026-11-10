use whisker::css::Color;

/// Light or dark appearance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Brightness {
    Light,
    Dark,
}

/// Semantic colors of one theme (the Masamune `ColorThemeData`, close to a Material
/// `ColorScheme`).
///
/// Widgets read these instead of hard-coded colors so the whole app follows the selected
/// [`crate::ThemeMode`]. Start from [`Palette::MATERIAL_LIGHT`] / [`Palette::MATERIAL_DARK`] and
/// override fields with struct update syntax, or use [`Palette::with_brand`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub brightness: Brightness,
    /// Brand color (buttons, selection, links).
    pub primary: Color,
    /// Content on `primary`.
    pub on_primary: Color,
    /// Tinted fill for less prominent primary elements.
    pub primary_container: Color,
    /// Content on `primary_container`.
    pub on_primary_container: Color,
    pub secondary: Color,
    pub on_secondary: Color,
    pub secondary_container: Color,
    pub on_secondary_container: Color,
    /// Contrasting accent that balances `primary` and `secondary`.
    pub tertiary: Color,
    pub on_tertiary: Color,
    pub tertiary_container: Color,
    pub on_tertiary_container: Color,
    /// Scaffold background.
    pub background: Color,
    /// Raised surfaces (sheets, dialogs, panels).
    pub surface: Color,
    /// Cards inside surfaces.
    pub card: Color,
    /// The most emphasized container on a surface (Material's `surfaceContainerHighest`), for
    /// example the time picker dial and text field fills.
    pub surface_container_highest: Color,
    /// App bars, navigation bars and input fields.
    pub bar: Color,
    /// Main text and icons.
    pub text: Color,
    /// Secondary text.
    pub sub_text: Color,
    /// Disabled content.
    pub disabled: Color,
    /// Borders of outlined controls.
    pub outline: Color,
    /// Divider lines.
    pub divider: Color,
    pub error: Color,
    pub on_error: Color,
    pub warning: Color,
    pub on_warning: Color,
    /// Completed or positive states.
    pub success: Color,
    pub on_success: Color,
    /// Neutral notices.
    pub info: Color,
    pub on_info: Color,
    /// Color of elevation shadows (see [`crate::elevation`]).
    pub shadow: Color,
    /// Scrim behind modal sheets and dialogs.
    pub barrier: Color,
}

impl Palette {
    /// Material 3 baseline dark colors.
    pub const MATERIAL_DARK: Palette = Palette {
        brightness: Brightness::Dark,
        primary: Color::hex(0xD0BCFF),
        on_primary: Color::hex(0x381E72),
        primary_container: Color::hex(0x4F378B),
        on_primary_container: Color::hex(0xEADDFF),
        secondary: Color::hex(0xCCC2DC),
        on_secondary: Color::hex(0x332D41),
        secondary_container: Color::hex(0x4A4458),
        on_secondary_container: Color::hex(0xE8DEF8),
        tertiary: Color::hex(0xEFB8C8),
        on_tertiary: Color::hex(0x492532),
        tertiary_container: Color::hex(0x633B48),
        on_tertiary_container: Color::hex(0xFFD8E4),
        background: Color::hex(0x141218),
        surface: Color::hex(0x211F26),
        card: Color::hex(0x2B2930),
        surface_container_highest: Color::hex(0x36343B),
        bar: Color::hex(0x211F26),
        text: Color::hex(0xE6E0E9),
        sub_text: Color::hex(0xCAC4D0),
        disabled: Color::hex(0x938F99),
        outline: Color::hex(0x938F99),
        divider: Color::hex(0x49454F),
        error: Color::hex(0xF2B8B5),
        on_error: Color::hex(0x601410),
        warning: Color::hex(0xFFB74D),
        on_warning: Color::hex(0x000000),
        success: Color::hex(0x81C784),
        on_success: Color::hex(0x000000),
        info: Color::hex(0x64B5F6),
        on_info: Color::hex(0x000000),
        shadow: Color::hex(0x000000),
        barrier: Color::rgba(0, 0, 0, 0.54),
    };

    /// Material 3 baseline light colors.
    pub const MATERIAL_LIGHT: Palette = Palette {
        brightness: Brightness::Light,
        primary: Color::hex(0x6750A4),
        on_primary: Color::hex(0xFFFFFF),
        primary_container: Color::hex(0xEADDFF),
        on_primary_container: Color::hex(0x21005D),
        secondary: Color::hex(0x625B71),
        on_secondary: Color::hex(0xFFFFFF),
        secondary_container: Color::hex(0xE8DEF8),
        on_secondary_container: Color::hex(0x1D192B),
        tertiary: Color::hex(0x7D5260),
        on_tertiary: Color::hex(0xFFFFFF),
        tertiary_container: Color::hex(0xFFD8E4),
        on_tertiary_container: Color::hex(0x31111D),
        background: Color::hex(0xFEF7FF),
        surface: Color::hex(0xF7F2FA),
        card: Color::hex(0xF3EDF7),
        surface_container_highest: Color::hex(0xE6E0E9),
        bar: Color::hex(0xF3EDF7),
        text: Color::hex(0x1D1B20),
        sub_text: Color::hex(0x49454F),
        disabled: Color::hex(0x79747E),
        outline: Color::hex(0x79747E),
        divider: Color::hex(0xCAC4D0),
        error: Color::hex(0xB3261E),
        on_error: Color::hex(0xFFFFFF),
        warning: Color::hex(0xE65100),
        on_warning: Color::hex(0xFFFFFF),
        success: Color::hex(0x2E7D32),
        on_success: Color::hex(0xFFFFFF),
        info: Color::hex(0x1565C0),
        on_info: Color::hex(0xFFFFFF),
        shadow: Color::hex(0x000000),
        barrier: Color::rgba(0, 0, 0, 0.32),
    };

    /// This palette with the brand colors replaced, like
    /// `AppThemeData(primary:, onPrimary:, secondary:, onSecondary:)` in Masamune.
    ///
    /// ```ignore
    /// const DARK: Palette = Palette::MATERIAL_DARK.with_brand(
    ///     Color::hex(0x536DFE), Color::hex(0xFFFFFF), Color::hex(0x673AB7), Color::hex(0xFFFFFF),
    /// );
    /// ```
    pub const fn with_brand(
        self,
        primary: Color,
        on_primary: Color,
        secondary: Color,
        on_secondary: Color,
    ) -> Palette {
        Palette {
            primary,
            on_primary,
            secondary,
            on_secondary,
            ..self
        }
    }

    pub fn is_dark(&self) -> bool {
        self.brightness == Brightness::Dark
    }
}

/// Color helpers for theme colors.
pub trait ColorExt {
    /// The same color with its alpha replaced (Flutter's `withOpacity`).
    ///
    /// `rgba` and `hsla` colors keep their channels; named colors and `transparent` are returned
    /// unchanged because they have no channels to keep.
    fn with_alpha(self, alpha: f32) -> Color;
}

impl ColorExt for Color {
    fn with_alpha(self, alpha: f32) -> Color {
        let alpha = alpha.clamp(0.0, 1.0);
        match self {
            Color::Rgba(r, g, b, _) => Color::Rgba(r, g, b, alpha),
            Color::Hsla { h, s, l, .. } => Color::Hsla { h, s, l, a: alpha },
            other => other,
        }
    }
}

/// `#RRGGBB` string of an opaque 24-bit color (for SVG fills).
pub fn hex_string(rgb: u32) -> String {
    format!("#{rgb:06X}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_alpha_replaces_only_alpha() {
        assert_eq!(
            Color::hex(0x536DFE).with_alpha(0.5),
            Color::rgba(0x53, 0x6D, 0xFE, 0.5)
        );
        assert_eq!(
            Color::rgba(1, 2, 3, 0.2).with_alpha(2.0),
            Color::rgba(1, 2, 3, 1.0)
        );
        assert_eq!(Color::Transparent.with_alpha(0.5), Color::Transparent);
    }

    #[test]
    fn with_brand_keeps_the_rest() {
        const DARK: Palette = Palette::MATERIAL_DARK.with_brand(
            Color::hex(0x536DFE),
            Color::hex(0xFFFFFF),
            Color::hex(0x673AB7),
            Color::hex(0xFFFFFF),
        );
        assert_eq!(DARK.primary, Color::hex(0x536DFE));
        assert_eq!(DARK.on_secondary, Color::hex(0xFFFFFF));
        assert_eq!(DARK.surface, Palette::MATERIAL_DARK.surface);
        assert!(DARK.is_dark());
    }
}
