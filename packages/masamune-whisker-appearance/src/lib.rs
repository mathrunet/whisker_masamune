//! Appearance of Whisker apps: the app-wide theme and app-wide rebuilds.
//!
//! - [`ThemeConfig`] / [`init_theme`]: the app-wide theme (palettes, typography, default font
//!   family, shapes, spacing, default mode). `ThemeConfig::default()` is the Material 3 baseline.
//! - [`Palette`] / [`palette`] / [`use_palette`]: semantic colors of the current theme.
//!   [`Palette::with_brand`] swaps the brand colors and [`ColorExt::with_alpha`] makes
//!   translucent variants.
//! - [`Typography`] / [`text`] / [`bold`]: Material 3 text styles.
//! - [`Shape`] / [`shape`], [`Spacing`] / [`spacing`], [`elevation`]: corner radii, gaps and
//!   shadows.
//! - [`ThemeMode`] / [`change_theme_mode`]: light, dark or system, saved across launches.
//! - [`follow_system`]: follows system dark mode and styles the system bars.
//! - [`AppRoot`] / [`rebuild`] / [`spawn_app`]: app root that can recreate the whole UI.
//!
//! Screens read [`palette`] (and masamune-whisker-localize's `l()`) while rendering, so a change
//! is applied by rebuilding everything under [`AppRoot`] (the router starts again from `/`).
//!
//! | Flutter (Masamune) | Whisker |
//! |---|---|
//! | `AppThemeData(primary: ..)` | `ThemeConfig { light: Palette::MATERIAL_LIGHT.with_brand(..), .. }` |
//! | `theme.color.primary` | `palette().primary` |
//! | `theme.text.bodyMedium` | `text(TextKind::BodyMedium)` |
//! | `defaultFontFamily` / `defaultFontFamilyResolver` | `ThemeConfig::font_family` |
//! | `TextStyle(fontFamily: ..)` in the text theme | `TextStyle::with_family` |
//! | `color.withOpacity(0.5)` | `color.with_alpha(0.5)` |
//! | `BorderRadius.circular(28)` | `px(shape().extra_large)` |

mod palette;
mod root;
mod system;
mod theme;
mod tokens;
pub mod typography;

pub use palette::{Brightness, ColorExt, Palette, hex_string};
pub use root::{AppRoot, generation, rebuild, set_app_owner, spawn_app, with_app_owner};
pub use system::follow_system;
pub use theme::{
    ThemeConfig, ThemeMode, brightness, init_theme, on_palette_changed, palette,
    set_system_brightness, set_theme_mode, shape, spacing, theme_mode, typography, use_palette,
};
pub use tokens::{Shape, Spacing, elevation};
pub use typography::{
    TextKind, TextStyle, Typography, bold, font_family, set_font_family_resolver, text, text_with,
};

/// Selects and saves a theme mode, rebuilding the UI when the palette changes.
pub fn change_theme_mode(mode: ThemeMode) {
    if set_theme_mode(mode) {
        rebuild();
    }
}
