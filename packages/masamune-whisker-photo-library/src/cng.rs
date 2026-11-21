//! CNG plugin that writes the iOS photo library usage description.
//!
//! ```ignore
//! use masamune_whisker_photo_library::cng::MasamunePhotoLibrary;
//!
//! app.project_plugin::<MasamunePhotoLibrary>(|c| {
//!     c.photo_library_usage("Used to show the photos you took along today's trail.")
//!         .prevent_limited_access_alert(true);
//! });
//! ```
//!
//! - iOS: `NSPhotoLibraryUsageDescription` (iOS terminates an app that asks without it) and, with
//!   [`MasamunePhotoLibraryConfig::prevent_limited_access_alert`],
//!   `PHPhotoLibraryPreventAutomaticLimitedAccessAlert` so iOS does not offer to change the
//!   selection on every launch. Without settings the plugin changes nothing. The project plugin
//!   runs after `masamune-whisker-image-picker`'s plugin, so its description wins when both set one.
//! - Android: nothing to generate. The crate's manifest declares `READ_MEDIA_IMAGES`,
//!   `READ_MEDIA_VIDEO`, `READ_MEDIA_VISUAL_USER_SELECTED`, `ACCESS_MEDIA_LOCATION` and
//!   `READ_EXTERNAL_STORAGE` (up to Android 12).

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use whisker_plugin::PluginConfig;
use whisker_plugin::project::{
    ProjectContext, ProjectIr, ProjectPlugin, ProjectUpdate, PropertyListValue,
};

/// Settings of [`MasamunePhotoLibrary`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MasamunePhotoLibraryConfig {
    /// `NSPhotoLibraryUsageDescription`.
    pub photo_library_usage: Option<String>,
    /// `PHPhotoLibraryPreventAutomaticLimitedAccessAlert`.
    pub prevent_limited_access_alert: bool,
}

impl MasamunePhotoLibraryConfig {
    /// Why the app reads the photo library (required on iOS).
    pub fn photo_library_usage(&mut self, description: impl Into<String>) -> &mut Self {
        self.photo_library_usage = Some(description.into());
        self
    }

    /// Do not let iOS offer to change the limited selection automatically; the app calls
    /// `present_limited_picker` instead.
    pub fn prevent_limited_access_alert(&mut self, prevent: bool) -> &mut Self {
        self.prevent_limited_access_alert = prevent;
        self
    }
}

impl PluginConfig for MasamunePhotoLibraryConfig {
    const NAME: &'static str = "masamune-whisker-photo-library";
}

/// The plugin. Register it with `app.project_plugin::<MasamunePhotoLibrary>(|c| …)`.
pub struct MasamunePhotoLibrary;

impl ProjectPlugin for MasamunePhotoLibrary {
    type Config = MasamunePhotoLibraryConfig;

    fn validate(&self, config: &Self::Config) -> Result<()> {
        ensure!(
            config
                .photo_library_usage
                .as_deref()
                .is_none_or(|text| !text.trim().is_empty()),
            "masamune-whisker-photo-library: photo_library_usage must not be empty"
        );
        Ok(())
    }

    fn contribute(&self, context: &ProjectContext, config: &Self::Config) -> Result<ProjectUpdate> {
        self.validate(config)?;
        let ProjectIr::Ios(ios) = &context.project else {
            return Ok(ProjectUpdate::Keep);
        };
        if config.photo_library_usage.is_none() && !config.prevent_limited_access_alert {
            return Ok(ProjectUpdate::Keep);
        }
        let mut ios = ios.clone();
        let target = ios
            .apple
            .targets
            .get_mut(&ios.apple.application)
            .context("masamune-whisker-photo-library: main iOS application is missing")?;
        if let Some(usage) = &config.photo_library_usage {
            target.info_plist.insert(
                "NSPhotoLibraryUsageDescription".into(),
                PropertyListValue::String(usage.clone()),
            );
        }
        if config.prevent_limited_access_alert {
            target.info_plist.insert(
                "PHPhotoLibraryPreventAutomaticLimitedAccessAlert".into(),
                PropertyListValue::Boolean(true),
            );
        }
        Ok(ProjectUpdate::Replace {
            project: Box::new(ProjectIr::Ios(ios)),
            reason: "Describe the photo library use".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use whisker_plugin::project::{AppleTarget, IosProjectIr};

    fn ios() -> ProjectContext {
        let mut ios = IosProjectIr::default();
        ios.apple.application = "App".into();
        ios.apple
            .targets
            .insert("App".into(), AppleTarget::default());
        ProjectContext {
            project: ProjectIr::Ios(ios),
            app_crate_dir: None,
        }
    }

    #[test]
    fn writes_ios_keys() {
        let mut config = MasamunePhotoLibraryConfig::default();
        config
            .photo_library_usage("photos")
            .prevent_limited_access_alert(true);
        let ProjectUpdate::Replace { project, .. } =
            MasamunePhotoLibrary.contribute(&ios(), &config).unwrap()
        else {
            panic!("expected a replacement")
        };
        let ProjectIr::Ios(ios) = *project else {
            panic!("expected iOS")
        };
        let plist = &ios.apple.targets["App"].info_plist;
        assert_eq!(
            plist["NSPhotoLibraryUsageDescription"],
            PropertyListValue::String("photos".into())
        );
        assert_eq!(
            plist["PHPhotoLibraryPreventAutomaticLimitedAccessAlert"],
            PropertyListValue::Boolean(true)
        );
    }

    #[test]
    fn defaults_change_nothing() {
        assert!(matches!(
            MasamunePhotoLibrary
                .contribute(&ios(), &Default::default())
                .unwrap(),
            ProjectUpdate::Keep
        ));
        let mut config = MasamunePhotoLibraryConfig::default();
        config.photo_library_usage(" ");
        assert!(MasamunePhotoLibrary.validate(&config).is_err());
    }
}
