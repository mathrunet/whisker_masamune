//! CNG plugin that declares the main activity's `configChanges` (and orientation).
//!
//! ```ignore
//! use masamune_whisker_screen::ConfigChanges;
//! use masamune_whisker_screen::cng::MasamuneScreen;
//!
//! app.plugin::<MasamuneScreen>(|c| {
//!     c.config_changes(ConfigChanges::RESIZE | ConfigChanges::KEYBOARD | ConfigChanges::UI_MODE | ConfigChanges::LOCALE);
//! });
//! ```
//!
//! The plugin writes a manifest overlay for the debug and release build types, which Gradle
//! merges into the generated `AndroidManifest.xml`. `configChanges` exist only on Android: iOS
//! never recreates the app for these changes.
//!
//! On iOS the supported orientations come from `Info.plist`. Leave them unset to allow every
//! orientation, or restrict them:
//!
//! ```ignore
//! app.plugin::<MasamuneScreen>(|c| {
//!     c.ios_orientations(&[IosOrientation::Portrait, IosOrientation::LandscapeLeft, IosOrientation::LandscapeRight]);
//! });
//! ```

use serde::{Deserialize, Serialize};
use whisker_plugin::{
    FileEntry, GenerateContext, Operation, PlistValue, Plugin, PluginConfig, Target,
};

use crate::{ConfigChanges, android_main_activity_overlay};

/// Settings of [`MasamuneScreen`].
#[derive(Default, Serialize, Deserialize)]
pub struct MasamuneScreenConfig {
    /// Bits of [`ConfigChanges`]. Defaults to `RESIZE | KEYBOARD`.
    #[serde(default)]
    pub config_changes: Option<u32>,
    /// `android:screenOrientation` (for example `"portrait"`).
    #[serde(default)]
    pub screen_orientation: Option<String>,
    /// iPhone orientations (`UISupportedInterfaceOrientations`). `None` keeps the default (all).
    #[serde(default)]
    pub ios_orientations: Option<Vec<IosOrientation>>,
    /// iPad orientations (`UISupportedInterfaceOrientations~ipad`). `None` keeps the default
    /// (all, which iPad multitasking requires).
    #[serde(default)]
    pub ipad_orientations: Option<Vec<IosOrientation>>,
}

/// An iOS interface orientation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IosOrientation {
    Portrait,
    PortraitUpsideDown,
    LandscapeLeft,
    LandscapeRight,
}

impl IosOrientation {
    /// The `Info.plist` value.
    pub fn plist_value(self) -> &'static str {
        match self {
            Self::Portrait => "UIInterfaceOrientationPortrait",
            Self::PortraitUpsideDown => "UIInterfaceOrientationPortraitUpsideDown",
            Self::LandscapeLeft => "UIInterfaceOrientationLandscapeLeft",
            Self::LandscapeRight => "UIInterfaceOrientationLandscapeRight",
        }
    }
}

impl MasamuneScreenConfig {
    pub fn config_changes(&mut self, changes: ConfigChanges) -> &mut Self {
        self.config_changes = Some(changes.bits());
        self
    }

    pub fn screen_orientation(&mut self, orientation: impl Into<String>) -> &mut Self {
        self.screen_orientation = Some(orientation.into());
        self
    }

    /// Orientations an iPhone may use.
    pub fn ios_orientations(&mut self, orientations: &[IosOrientation]) -> &mut Self {
        self.ios_orientations = Some(orientations.to_vec());
        self
    }

    /// Orientations an iPad may use. Restricting them opts the app out of iPad multitasking.
    pub fn ipad_orientations(&mut self, orientations: &[IosOrientation]) -> &mut Self {
        self.ipad_orientations = Some(orientations.to_vec());
        self
    }
}

impl PluginConfig for MasamuneScreenConfig {
    const NAME: &'static str = "masamune-whisker-screen";
}

/// The plugin. Register it with `app.plugin::<MasamuneScreen>(|c| …)`.
pub struct MasamuneScreen;

impl Plugin for MasamuneScreen {
    type Config = MasamuneScreenConfig;

    fn apply(&self, ctx: &mut GenerateContext, cfg: &MasamuneScreenConfig) -> anyhow::Result<()> {
        if let Some(ios) = ctx.ios.as_mut() {
            for (key, orientations) in [
                ("UISupportedInterfaceOrientations", &cfg.ios_orientations),
                (
                    "UISupportedInterfaceOrientations~ipad",
                    &cfg.ipad_orientations,
                ),
            ] {
                let Some(orientations) = orientations else {
                    continue;
                };
                ios.info_plist.insert(
                    key.into(),
                    PlistValue::Array(
                        orientations
                            .iter()
                            .map(|o| PlistValue::String(o.plist_value().into()))
                            .collect(),
                    ),
                );
                ctx.journal.record(
                    MasamuneScreenConfig::NAME,
                    Target::Ios,
                    &format!("info_plist.{key}"),
                    Operation::Override,
                );
            }
        }
        let Some(android) = ctx.android.as_mut() else {
            return Ok(());
        };
        let changes = cfg
            .config_changes
            .map(ConfigChanges::from_bits)
            .unwrap_or(ConfigChanges::RESIZE | ConfigChanges::KEYBOARD);
        let overlay = android_main_activity_overlay(changes, cfg.screen_orientation.as_deref());
        for path in [
            "app/src/debug/AndroidManifest.xml",
            "app/src/release/AndroidManifest.xml",
        ] {
            android.extra_files.insert(
                path.into(),
                FileEntry {
                    contents: overlay.clone(),
                    contents_base64: None,
                    mode: None,
                },
            );
        }
        ctx.journal.record(
            MasamuneScreenConfig::NAME,
            Target::Android,
            "extra_files",
            Operation::ArrayPush { count: 2 },
        );
        Ok(())
    }
}
