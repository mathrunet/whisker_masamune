//! Theme selection: light, dark or following the system, persisted across launches.

use std::cell::RefCell;
use std::rc::Rc;

use whisker::prelude::*;
use whisker_local_store::WhiskerLocalStore;

use crate::palette::{Brightness, Palette};
use crate::tokens::{Shape, Spacing};
use crate::typography::Typography;

/// Which palette the app uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum ThemeMode {
    /// Follow the system dark mode setting.
    #[default]
    System,
    Light,
    Dark,
}

impl ThemeMode {
    pub const ALL: [ThemeMode; 3] = [ThemeMode::System, ThemeMode::Light, ThemeMode::Dark];

    pub fn as_str(self) -> &'static str {
        match self {
            ThemeMode::System => "system",
            ThemeMode::Light => "light",
            ThemeMode::Dark => "dark",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "system" => Some(ThemeMode::System),
            "light" => Some(ThemeMode::Light),
            "dark" => Some(ThemeMode::Dark),
            _ => None,
        }
    }
}

/// The app-wide theme (the Masamune `AppThemeData`): palettes, typography, fonts, shapes,
/// spacing and the default mode.
///
/// `ThemeConfig::default()` is the Material 3 baseline. Override only what the app needs:
/// `ThemeConfig { light: LIGHT, dark: DARK, storage_key: "app.theme_mode", ..Default::default() }`.
#[derive(Clone)]
pub struct ThemeConfig {
    pub light: Palette,
    pub dark: Palette,
    /// Text styles shared by both palettes.
    pub typography: Typography,
    /// Corner radii.
    pub shape: Shape,
    /// Gaps and paddings.
    pub spacing: Spacing,
    /// Chooses the default font family (for example per language; an empty name keeps the
    /// platform font). `None` keeps the resolver set by
    /// [`crate::typography::set_font_family_resolver`]. Register the font files with
    /// `masamune-whisker-fonts`.
    pub font_family: Option<fn() -> &'static str>,
    /// Mode used until the user picks one.
    pub default_mode: ThemeMode,
    /// `whisker-local-store` key of the saved mode.
    pub storage_key: &'static str,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            light: Palette::MATERIAL_LIGHT,
            dark: Palette::MATERIAL_DARK,
            typography: Typography::MATERIAL,
            shape: Shape::DEFAULT,
            spacing: Spacing::DEFAULT,
            font_family: None,
            default_mode: ThemeMode::System,
            storage_key: "masamune.appearance.theme_mode",
        }
    }
}

type Listener = Rc<dyn Fn(&Palette)>;

struct State {
    config: ThemeConfig,
    mode: ThemeMode,
    system: Brightness,
    listeners: Vec<Listener>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State {
        config: ThemeConfig::default(),
        mode: ThemeMode::System,
        system: Brightness::Dark,
        listeners: Vec::new(),
    });
}

whisker::runtime_local! {
    static PALETTE: RwSignal<Palette> = signal(palette());
}

fn resolve(state: &State) -> Palette {
    let brightness = match state.mode {
        ThemeMode::System => state.system,
        ThemeMode::Light => Brightness::Light,
        ThemeMode::Dark => Brightness::Dark,
    };
    match brightness {
        Brightness::Light => state.config.light,
        Brightness::Dark => state.config.dark,
    }
}

/// Sets the palettes and restores the saved mode. Call once while preparing the app, on the
/// UI thread (the saved mode comes from `whisker-local-store`).
pub fn init_theme(config: ThemeConfig) {
    let saved = WhiskerLocalStore::load(config.storage_key.to_owned())
        .ok()
        .flatten()
        .and_then(|value| ThemeMode::parse(&value));
    if let Some(resolver) = config.font_family {
        crate::typography::set_font_family_resolver(resolver);
    }
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.mode = saved.unwrap_or(config.default_mode);
        state.config = config;
    });
    publish();
}

/// The selected mode.
pub fn theme_mode() -> ThemeMode {
    STATE.with(|state| state.borrow().mode)
}

/// Selects and saves a mode. Returns whether the palette changed.
pub fn set_theme_mode(mode: ThemeMode) -> bool {
    let key = STATE.with(|state| state.borrow().config.storage_key);
    if let Err(error) = WhiskerLocalStore::save(key.to_owned(), mode.as_str().to_owned()) {
        log::warn!("theme mode save failed: {error}");
    }
    update(|state| state.mode = mode)
}

/// Reports the system dark mode setting (from the device configuration). Returns whether the
/// palette changed.
pub fn set_system_brightness(brightness: Brightness) -> bool {
    update(|state| state.system = brightness)
}

/// Brightness of the current palette.
pub fn brightness() -> Brightness {
    palette().brightness
}

/// The current palette. Use [`use_palette`] in reactive closures that must update in place.
pub fn palette() -> Palette {
    STATE.with(|state| resolve(&state.borrow()))
}

/// The current text styles.
pub fn typography() -> Typography {
    STATE.with(|state| state.borrow().config.typography)
}

/// The current corner radii.
pub fn shape() -> Shape {
    STATE.with(|state| state.borrow().config.shape)
}

/// The current gaps and paddings.
pub fn spacing() -> Spacing {
    STATE.with(|state| state.borrow().config.spacing)
}

/// The current palette as a signal.
pub fn use_palette() -> ReadSignal<Palette> {
    PALETTE.with(|signal| signal.read_only())
}

/// Calls `f` with the new palette whenever it changes (and once right away).
pub fn on_palette_changed(f: impl Fn(&Palette) + 'static) {
    let listener: Listener = Rc::new(f);
    listener(&palette());
    STATE.with(|state| state.borrow_mut().listeners.push(listener));
}

fn update(change: impl FnOnce(&mut State)) -> bool {
    let before = palette();
    STATE.with(|state| change(&mut state.borrow_mut()));
    let changed = palette() != before;
    if changed {
        publish();
    }
    changed
}

fn publish() {
    let current = palette();
    PALETTE.with(|signal| {
        if signal.get_untracked() != current {
            signal.set(current);
        }
    });
    let listeners = STATE.with(|state| state.borrow().listeners.clone());
    for listener in listeners {
        listener(&current);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_material_baseline() {
        let config = ThemeConfig::default();
        assert_eq!(config.light, Palette::MATERIAL_LIGHT);
        assert_eq!(config.dark, Palette::MATERIAL_DARK);
        assert_eq!(config.typography, Typography::MATERIAL);
        assert_eq!(config.shape.extra_large, 28.0);
        assert_eq!(config.spacing.medium, 16.0);
        assert_eq!(config.default_mode, ThemeMode::System);
        assert!(config.font_family.is_none());
        assert_eq!(typography(), Typography::MATERIAL);
        assert_eq!(shape(), Shape::DEFAULT);
    }
}
