//! Appearance of Whisker apps: theme palettes, typography and app-wide rebuilds.
//!
//! - [`Palette`] / [`palette`] / [`use_palette`]: semantic colors of the current theme.
//! - [`ThemeMode`] / [`set_theme_mode`]: light, dark or system, saved across launches.
//! - [`typography`]: Material 3 text styles.
//! - [`AppRoot`] / [`rebuild`] / [`spawn_app`]: app root that can recreate the whole UI.

mod palette;
mod root;
mod theme;
pub mod typography;

pub use palette::{Brightness, Palette, hex_string};
pub use root::{AppRoot, generation, rebuild, set_app_owner, spawn_app, with_app_owner};
pub use theme::{
    ThemeConfig, ThemeMode, brightness, init_theme, on_palette_changed, palette,
    set_system_brightness, set_theme_mode, theme_mode, use_palette,
};
pub use typography::{TextKind, bold, text, text_with};
