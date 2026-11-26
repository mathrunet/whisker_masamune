//! CNG plugin that enables Push Notifications (`aps-environment`) on iOS (a no-op on Android).
//!
//! ```ignore
//! use masamune_whisker_messaging_firebase::cng::MasamuneFirebaseMessaging;
//!
//! app.project_plugin::<MasamuneFirebaseMessaging>(|c| {
//!     c.aps_environment("production");
//! });
//! ```

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use whisker_plugin::PluginConfig;
use whisker_plugin::project::{
    ProjectContext, ProjectIr, ProjectPlugin, ProjectUpdate, PropertyListValue,
};

/// iOS Push Notifications entitlement key.
pub const APS_ENVIRONMENT_ENTITLEMENT: &str = "aps-environment";

/// Settings of [`MasamuneFirebaseMessaging`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MasamuneFirebaseMessagingConfig {
    /// `development` or `production` (default).
    pub aps_environment: String,
    /// Adds `remote-notification` to `UIBackgroundModes` on iOS (default `true`).
    pub remote_notification_background_mode: bool,
}

impl Default for MasamuneFirebaseMessagingConfig {
    fn default() -> Self {
        Self {
            aps_environment: "production".into(),
            remote_notification_background_mode: true,
        }
    }
}

impl MasamuneFirebaseMessagingConfig {
    pub fn aps_environment(&mut self, environment: impl Into<String>) -> &mut Self {
        self.aps_environment = environment.into();
        self
    }

    pub fn remote_notification_background_mode(&mut self, enabled: bool) -> &mut Self {
        self.remote_notification_background_mode = enabled;
        self
    }
}

impl PluginConfig for MasamuneFirebaseMessagingConfig {
    const NAME: &'static str = "masamune-whisker-messaging-firebase";
}

/// The plugin. Register it with `app.project_plugin::<MasamuneFirebaseMessaging>(|c| …)`.
pub struct MasamuneFirebaseMessaging;

impl ProjectPlugin for MasamuneFirebaseMessaging {
    type Config = MasamuneFirebaseMessagingConfig;

    fn validate(&self, config: &Self::Config) -> Result<()> {
        ensure!(
            matches!(
                config.aps_environment.as_str(),
                "development" | "production"
            ),
            "masamune-whisker-messaging-firebase: aps_environment must be `development` or `production`"
        );
        Ok(())
    }

    fn contribute(&self, context: &ProjectContext, config: &Self::Config) -> Result<ProjectUpdate> {
        self.validate(config)?;
        let ProjectIr::Ios(ios) = &context.project else {
            return Ok(ProjectUpdate::Keep);
        };
        let mut ios = ios.clone();
        let target = ios
            .apple
            .targets
            .get_mut(&ios.apple.application)
            .context("masamune-whisker-messaging-firebase: main iOS application is missing")?;
        target.entitlements.insert(
            APS_ENVIRONMENT_ENTITLEMENT.into(),
            PropertyListValue::String(config.aps_environment.clone()),
        );
        if config.remote_notification_background_mode {
            let PropertyListValue::Array(modes) = target
                .info_plist
                .entry("UIBackgroundModes".into())
                .or_insert_with(|| PropertyListValue::Array(Vec::new()))
            else {
                bail!("UIBackgroundModes must be an array");
            };
            let remote = PropertyListValue::String("remote-notification".into());
            if !modes.contains(&remote) {
                modes.push(remote);
            }
        }
        Ok(ProjectUpdate::Replace {
            project: Box::new(ProjectIr::Ios(ios)),
            reason: "Enable Push Notifications (FCM) on iOS".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use whisker_plugin::project::{AppleTarget, IosProjectIr};

    #[test]
    fn adds_aps_entitlement_and_background_mode_on_ios() {
        let mut ios = IosProjectIr::default();
        ios.apple.application = "App".into();
        ios.apple
            .targets
            .insert("App".into(), AppleTarget::default());
        let context = ProjectContext {
            project: ProjectIr::Ios(ios),
            app_crate_dir: None,
        };
        let mut config = MasamuneFirebaseMessagingConfig::default();
        config.aps_environment("development");
        let ProjectUpdate::Replace { project, .. } = MasamuneFirebaseMessaging
            .contribute(&context, &config)
            .unwrap()
        else {
            panic!("expected a replacement")
        };
        let ProjectIr::Ios(ios) = *project else {
            panic!("expected iOS")
        };
        let target = &ios.apple.targets["App"];
        assert_eq!(
            target.entitlements[APS_ENVIRONMENT_ENTITLEMENT],
            PropertyListValue::String("development".into())
        );
        assert_eq!(
            target.info_plist["UIBackgroundModes"],
            PropertyListValue::Array(vec![PropertyListValue::String(
                "remote-notification".into()
            )])
        );

        let mut bad = MasamuneFirebaseMessagingConfig::default();
        bad.aps_environment("staging");
        assert!(MasamuneFirebaseMessaging.validate(&bad).is_err());

        let android = ProjectContext {
            project: ProjectIr::Android(Box::default()),
            app_crate_dir: None,
        };
        assert!(matches!(
            MasamuneFirebaseMessaging
                .contribute(&android, &MasamuneFirebaseMessagingConfig::default())
                .unwrap(),
            ProjectUpdate::Keep
        ));
    }
}
