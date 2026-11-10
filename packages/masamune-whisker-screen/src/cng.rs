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
//! merges into the generated `AndroidManifest.xml`.

use serde::{Deserialize, Serialize};
use whisker_plugin::{FileEntry, GenerateContext, Operation, Plugin, PluginConfig, Target};

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
}

impl PluginConfig for MasamuneScreenConfig {
    const NAME: &'static str = "masamune-whisker-screen";
}

/// The plugin. Register it with `app.plugin::<MasamuneScreen>(|c| …)`.
pub struct MasamuneScreen;

impl Plugin for MasamuneScreen {
    type Config = MasamuneScreenConfig;

    fn apply(&self, ctx: &mut GenerateContext, cfg: &MasamuneScreenConfig) -> anyhow::Result<()> {
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
