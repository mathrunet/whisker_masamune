//! Appearance of Whisker apps: theme palettes, languages, typography and app-wide rebuilds.
//!
//! - [`Palette`] / [`palette`] / [`use_palette`]: semantic colors of the current theme.
//! - [`ThemeMode`] / [`change_theme_mode`]: light, dark or system, saved across launches.
//! - [`locale`] / [`change_language`]: supported languages, system or pinned, saved across launches.
//! - [`follow_system`]: follows system dark mode and languages and styles the system bars.
//! - [`typography`]: Material 3 text styles.
//! - [`AppRoot`] / [`rebuild`] / [`spawn_app`]: app root that can recreate the whole UI.
//!
//! Screens read [`palette`] and the generated `l()` while rendering, so a change is applied by
//! rebuilding everything under [`AppRoot`] (the router starts again from `/`).

pub mod locale;
mod palette;
mod root;
mod system;
mod theme;
pub mod typography;

pub use locale::{LanguagePref, init_locale, language_pref, set_language_pref};
pub use palette::{Brightness, Palette, hex_string};
pub use root::{AppRoot, generation, rebuild, set_app_owner, spawn_app, with_app_owner};
pub use system::follow_system;
pub use theme::{
    ThemeConfig, ThemeMode, brightness, init_theme, on_palette_changed, palette,
    set_system_brightness, set_theme_mode, theme_mode, use_palette,
};
pub use typography::{TextKind, bold, text, text_with};

/// Selects and saves a theme mode, rebuilding the UI when the palette changes.
pub fn change_theme_mode(mode: ThemeMode) {
    if set_theme_mode(mode) {
        rebuild();
    }
}

/// Selects and saves a language preference, rebuilding the UI when the language changes.
pub fn change_language(pref: LanguagePref) {
    if set_language_pref(pref) {
        rebuild();
    }
}
