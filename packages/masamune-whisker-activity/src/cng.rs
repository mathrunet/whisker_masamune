//! CNG plugin that writes the iOS motion usage description.
//!
//! ```ignore
//! use masamune_whisker_activity::cng::MasamuneActivity;
//!
//! app.project_plugin::<MasamuneActivity>(|c| {
//!     c.motion_usage("Used to switch the music when you start or stop moving.");
//! });
//! ```
//!
//! - iOS: `NSMotionUsageDescription` (iOS terminates an app that reads motion activity without
//!   it). Without a description the plugin changes nothing.
//! - Android: nothing to generate. The crate's manifest declares `ACTIVITY_RECOGNITION` (Android
//!   10 and later), `com.google.android.gms.permission.ACTIVITY_RECOGNITION` (Android 9 and
//!   earlier) and the receiver of the transition updates.

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use whisker_plugin::PluginConfig;
use whisker_plugin::project::{
    ProjectContext, ProjectIr, ProjectPlugin, ProjectUpdate, PropertyListValue,
};

/// Settings of [`MasamuneActivity`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MasamuneActivityConfig {
    /// `NSMotionUsageDescription`.
    pub motion_usage: Option<String>,
}

impl MasamuneActivityConfig {
    /// Why the app reads motion activity (required on iOS).
    pub fn motion_usage(&mut self, description: impl Into<String>) -> &mut Self {
        self.motion_usage = Some(description.into());
        self
    }
}

impl PluginConfig for MasamuneActivityConfig {
    const NAME: &'static str = "masamune-whisker-activity";
}

/// The plugin. Register it with `app.project_plugin::<MasamuneActivity>(|c| …)`.
pub struct MasamuneActivity;

impl ProjectPlugin for MasamuneActivity {
    type Config = MasamuneActivityConfig;

    fn validate(&self, config: &Self::Config) -> Result<()> {
        ensure!(
            config
                .motion_usage
                .as_deref()
                .is_none_or(|text| !text.trim().is_empty()),
            "masamune-whisker-activity: motion_usage must not be empty"
        );
        Ok(())
    }

    fn contribute(&self, context: &ProjectContext, config: &Self::Config) -> Result<ProjectUpdate> {
        self.validate(config)?;
        let (ProjectIr::Ios(ios), Some(usage)) = (&context.project, &config.motion_usage) else {
            return Ok(ProjectUpdate::Keep);
        };
        let mut ios = ios.clone();
        let target = ios
            .apple
            .targets
            .get_mut(&ios.apple.application)
            .context("masamune-whisker-activity: main iOS application is missing")?;
        target.info_plist.insert(
            "NSMotionUsageDescription".into(),
            PropertyListValue::String(usage.clone()),
        );
        Ok(ProjectUpdate::Replace {
            project: Box::new(ProjectIr::Ios(ios)),
            reason: "Describe the motion activity use".into(),
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
    fn writes_the_motion_description() {
        let mut config = MasamuneActivityConfig::default();
        config.motion_usage("motion");
        let ProjectUpdate::Replace { project, .. } =
            MasamuneActivity.contribute(&ios(), &config).unwrap()
        else {
            panic!("expected a replacement")
        };
        let ProjectIr::Ios(ios) = *project else {
            panic!("expected iOS")
        };
        assert_eq!(
            ios.apple.targets["App"].info_plist["NSMotionUsageDescription"],
            PropertyListValue::String("motion".into())
        );
    }

    #[test]
    fn defaults_change_nothing() {
        assert!(matches!(
            MasamuneActivity
                .contribute(&ios(), &Default::default())
                .unwrap(),
            ProjectUpdate::Keep
        ));
        let mut config = MasamuneActivityConfig::default();
        config.motion_usage("");
        assert!(MasamuneActivity.validate(&config).is_err());
    }
}
