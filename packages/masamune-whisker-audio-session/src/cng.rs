//! CNG plugin that keeps audio playing in the background on iOS.
//!
//! ```ignore
//! use masamune_whisker_audio_session::cng::MasamuneAudioSession;
//!
//! app.project_plugin::<MasamuneAudioSession>(|c| {
//!     c.background_audio(true);
//! });
//! ```
//!
//! - iOS: with `background_audio(true)` adds `audio` to `UIBackgroundModes` (merged with the
//!   modes other plugins set, such as `whisker_audio::WhiskerAudio`). Without it the plugin
//!   changes nothing, so it is safe when the crate is only a dependency.
//! - Android: nothing to generate. The crate's own manifest declares the media playback
//!   foreground service and the `FOREGROUND_SERVICE`, `FOREGROUND_SERVICE_MEDIA_PLAYBACK`,
//!   `WAKE_LOCK` and `POST_NOTIFICATIONS` permissions, which the Android build merges.

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use whisker_plugin::PluginConfig;
use whisker_plugin::project::{
    ProjectContext, ProjectIr, ProjectPlugin, ProjectUpdate, PropertyListValue,
};

/// Settings of [`MasamuneAudioSession`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MasamuneAudioSessionConfig {
    /// Add `audio` to the iOS `UIBackgroundModes`.
    pub background_audio: bool,
}

impl MasamuneAudioSessionConfig {
    /// Keep playing while the app is in the background or the screen is locked (iOS).
    pub fn background_audio(&mut self, enabled: bool) -> &mut Self {
        self.background_audio = enabled;
        self
    }
}

impl PluginConfig for MasamuneAudioSessionConfig {
    const NAME: &'static str = "masamune-whisker-audio-session";
}

/// The plugin. Register it with `app.project_plugin::<MasamuneAudioSession>(|c| …)`.
pub struct MasamuneAudioSession;

impl ProjectPlugin for MasamuneAudioSession {
    type Config = MasamuneAudioSessionConfig;

    fn contribute(&self, context: &ProjectContext, config: &Self::Config) -> Result<ProjectUpdate> {
        let ProjectIr::Ios(ios) = &context.project else {
            return Ok(ProjectUpdate::Keep);
        };
        if !config.background_audio {
            return Ok(ProjectUpdate::Keep);
        }
        let mut ios = ios.clone();
        let target = ios
            .apple
            .targets
            .get_mut(&ios.apple.application)
            .context("masamune-whisker-audio-session: main iOS application is missing")?;
        add_array_item(&mut target.info_plist, "UIBackgroundModes", "audio")?;
        Ok(ProjectUpdate::Replace {
            project: Box::new(ProjectIr::Ios(ios)),
            reason: "Play audio in the background".into(),
        })
    }
}

/// Appends `value` to the string array `key` unless it is already there.
fn add_array_item(
    plist: &mut BTreeMap<String, PropertyListValue>,
    key: &str,
    value: &str,
) -> Result<()> {
    let PropertyListValue::Array(items) = plist
        .entry(key.into())
        .or_insert_with(|| PropertyListValue::Array(Vec::new()))
    else {
        bail!("{key} must be an array");
    };
    let value = PropertyListValue::String(value.into());
    if !items.contains(&value) {
        items.push(value);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use whisker_plugin::project::{AppleTarget, IosProjectIr};

    fn ios(modes: &[&str]) -> ProjectContext {
        let mut ios = IosProjectIr::default();
        ios.apple.application = "App".into();
        let mut app = AppleTarget::default();
        if !modes.is_empty() {
            app.info_plist.insert(
                "UIBackgroundModes".into(),
                PropertyListValue::Array(
                    modes
                        .iter()
                        .map(|mode| PropertyListValue::String((*mode).into()))
                        .collect(),
                ),
            );
        }
        ios.apple.targets.insert("App".into(), app);
        ProjectContext {
            project: ProjectIr::Ios(ios),
            app_crate_dir: None,
        }
    }

    fn modes(update: ProjectUpdate) -> PropertyListValue {
        let ProjectUpdate::Replace { project, .. } = update else {
            panic!("expected a replacement")
        };
        let ProjectIr::Ios(ios) = *project else {
            panic!("expected iOS")
        };
        ios.apple.targets["App"].info_plist["UIBackgroundModes"].clone()
    }

    #[test]
    fn merges_the_audio_background_mode() {
        let mut config = MasamuneAudioSessionConfig::default();
        config.background_audio(true);
        let merged = modes(
            MasamuneAudioSession
                .contribute(&ios(&["audio", "location"]), &config)
                .unwrap(),
        );
        assert_eq!(
            merged,
            PropertyListValue::Array(vec![
                PropertyListValue::String("audio".into()),
                PropertyListValue::String("location".into()),
            ])
        );
        let added = modes(MasamuneAudioSession.contribute(&ios(&[]), &config).unwrap());
        assert_eq!(
            added,
            PropertyListValue::Array(vec![PropertyListValue::String("audio".into())])
        );
    }

    #[test]
    fn defaults_and_android_change_nothing() {
        assert!(matches!(
            MasamuneAudioSession
                .contribute(&ios(&[]), &Default::default())
                .unwrap(),
            ProjectUpdate::Keep
        ));
        let android = ProjectContext {
            project: ProjectIr::Android(Box::default()),
            app_crate_dir: None,
        };
        let mut config = MasamuneAudioSessionConfig::default();
        config.background_audio(true);
        assert!(matches!(
            MasamuneAudioSession.contribute(&android, &config).unwrap(),
            ProjectUpdate::Keep
        ));
    }
}
