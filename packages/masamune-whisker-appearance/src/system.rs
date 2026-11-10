//! Keeps the app in sync with the system: dark mode and the system bars.

use std::cell::Cell;

use masamune_whisker_device_info::{DeviceConfiguration, Subscription};

use crate::palette::{Brightness, Palette};
use crate::root::rebuild;
use crate::theme::{on_palette_changed, set_system_brightness};

thread_local! {
    static FOLLOWING: Cell<bool> = const { Cell::new(false) };
}

whisker::runtime_local! {
    static SUBSCRIPTION: Subscription = masamune_whisker_device_info::on_configuration_changed(apply_configuration);
}

fn apply_configuration(config: DeviceConfiguration) {
    let brightness = if config.night_mode {
        Brightness::Dark
    } else {
        Brightness::Light
    };
    if set_system_brightness(brightness) {
        log::info!("appearance: system theme changed; rebuilding");
        rebuild();
    }
}

/// Converts a palette color to `0xAARRGGBB` for the window background.
fn argb(palette: &Palette) -> u32 {
    let mut css = String::new();
    let _ = whisker::css::ToCss::to_css(&palette.background, &mut css);
    u32::from_str_radix(css.trim_start_matches('#').get(..6).unwrap_or("000000"), 16)
        .map(|rgb| 0xFF00_0000 | rgb)
        .unwrap_or(0xFF00_0000)
}

fn style_system_bars(palette: &Palette) {
    let light = palette.brightness == Brightness::Light;
    if let Err(error) = masamune_whisker_app_control::set_system_bars_style(light, light) {
        log::warn!("system bar style failed: {error}");
    }
    if let Err(error) = masamune_whisker_app_control::set_window_background(argb(palette)) {
        log::warn!("window background failed: {error}");
    }
}

/// Follows the system dark mode (rebuilding the UI when it changes the palette) and styles the
/// system bars for the current palette. Call once on the UI thread after [`crate::init_theme`].
/// System languages are followed by masamune-whisker-localize.
///
/// Add `uiMode` to the activity's `configChanges` (masamune-whisker-screen's
/// `ConfigChanges::UI_MODE`), otherwise Android restarts the app instead.
pub fn follow_system() {
    // Start from the current configuration so the first event does not look like a change.
    if let Ok(info) = masamune_whisker_device_info::info() {
        set_system_brightness(if info.night_mode {
            Brightness::Dark
        } else {
            Brightness::Light
        });
    }
    SUBSCRIPTION.with(|_| {});
    if !FOLLOWING.with(|f| f.replace(true)) {
        on_palette_changed(style_system_bars);
    }
}
