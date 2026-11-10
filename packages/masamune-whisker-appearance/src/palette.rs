use whisker::css::Color;

/// Light or dark appearance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Brightness {
    Light,
    Dark,
}

/// Semantic colors of one theme (a reduced Material `ColorScheme`).
///
/// Widgets read these instead of hard-coded colors so the whole app follows the selected
/// [`crate::ThemeMode`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub brightness: Brightness,
    /// Brand color (buttons, selection, links).
    pub primary: Color,
    /// Content on `primary`.
    pub on_primary: Color,
    pub secondary: Color,
    pub on_secondary: Color,
    /// Scaffold background.
    pub background: Color,
    /// Raised surfaces (sheets, dialogs, panels).
    pub surface: Color,
    /// Cards inside surfaces.
    pub card: Color,
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
    pub warning: Color,
    /// Scrim behind modal sheets and dialogs.
    pub barrier: Color,
}

impl Palette {
    /// Material 3 baseline dark colors.
    pub const MATERIAL_DARK: Palette = Palette {
        brightness: Brightness::Dark,
        primary: Color::hex(0xD0BCFF),
        on_primary: Color::hex(0x381E72),
        secondary: Color::hex(0xCCC2DC),
        on_secondary: Color::hex(0x332D41),
        background: Color::hex(0x141218),
        surface: Color::hex(0x211F26),
        card: Color::hex(0x2B2930),
        bar: Color::hex(0x211F26),
        text: Color::hex(0xE6E0E9),
        sub_text: Color::hex(0xCAC4D0),
        disabled: Color::hex(0x938F99),
        outline: Color::hex(0x938F99),
        divider: Color::hex(0x49454F),
        error: Color::hex(0xF2B8B5),
        warning: Color::hex(0xFFB74D),
        barrier: Color::rgba(0, 0, 0, 0.54),
    };

    /// Material 3 baseline light colors.
    pub const MATERIAL_LIGHT: Palette = Palette {
        brightness: Brightness::Light,
        primary: Color::hex(0x6750A4),
        on_primary: Color::hex(0xFFFFFF),
        secondary: Color::hex(0x625B71),
        on_secondary: Color::hex(0xFFFFFF),
        background: Color::hex(0xFEF7FF),
        surface: Color::hex(0xF7F2FA),
        card: Color::hex(0xF3EDF7),
        bar: Color::hex(0xF3EDF7),
        text: Color::hex(0x1D1B20),
        sub_text: Color::hex(0x49454F),
        disabled: Color::hex(0x79747E),
        outline: Color::hex(0x79747E),
        divider: Color::hex(0xCAC4D0),
        error: Color::hex(0xB3261E),
        warning: Color::hex(0xE65100),
        barrier: Color::rgba(0, 0, 0, 0.32),
    };

    pub fn is_dark(&self) -> bool {
        self.brightness == Brightness::Dark
    }
}

/// `#RRGGBB` string of an opaque 24-bit color (for SVG fills).
pub fn hex_string(rgb: u32) -> String {
    format!("#{rgb:06X}")
}
