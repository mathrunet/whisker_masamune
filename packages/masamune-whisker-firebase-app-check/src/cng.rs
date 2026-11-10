//! CNG plugin that prepares the iOS app for App Check (a no-op on Android).
//!
//! ```ignore
//! use masamune_whisker_firebase_app_check::cng::MasamuneFirebaseAppCheck;
//!
//! app.project_plugin::<MasamuneFirebaseAppCheck>(|c| {
//!     c.app_attest_environment("production");
//! });
//! ```
//!
//! It adds the `com.apple.developer.devicecheck.appattest-environment` entitlement (App Attest
//! also has to be enabled for the App ID) and, when [`MasamuneFirebaseAppCheckConfig::provider`]
//! is not `auto`, the `MasamuneFirebaseAppCheckProvider` Info.plist key that forces the provider.

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use whisker_plugin::PluginConfig;
use whisker_plugin::project::{
    ProjectContext, ProjectIr, ProjectPlugin, ProjectUpdate, PropertyListValue,
};

/// Info.plist key read by the iOS module (`debug` or `appAttest`).
pub const PROVIDER_INFO_KEY: &str = "MasamuneFirebaseAppCheckProvider";
/// The App Attest environment entitlement.
pub const APP_ATTEST_ENTITLEMENT: &str = "com.apple.developer.devicecheck.appattest-environment";

/// Settings of [`MasamuneFirebaseAppCheck`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MasamuneFirebaseAppCheckConfig {
    /// `development` or `production` (default). App Store, TestFlight and Enterprise builds
    /// always use production regardless of this value.
    pub app_attest_environment: String,
    /// `auto` (default: the debug provider in Debug builds, App Attest in Release builds),
    /// `debug` or `app_attest`.
    pub provider: String,
}

impl Default for MasamuneFirebaseAppCheckConfig {
    fn default() -> Self {
        Self {
            app_attest_environment: "production".into(),
            provider: "auto".into(),
        }
    }
}

impl MasamuneFirebaseAppCheckConfig {
    pub fn app_attest_environment(&mut self, environment: impl Into<String>) -> &mut Self {
        self.app_attest_environment = environment.into();
        self
    }

    pub fn provider(&mut self, provider: impl Into<String>) -> &mut Self {
        self.provider = provider.into();
        self
    }
}

impl PluginConfig for MasamuneFirebaseAppCheckConfig {
    const NAME: &'static str = "masamune-whisker-firebase-app-check";
}

/// The plugin. Register it with `app.project_plugin::<MasamuneFirebaseAppCheck>(|c| …)`.
pub struct MasamuneFirebaseAppCheck;

impl ProjectPlugin for MasamuneFirebaseAppCheck {
    type Config = MasamuneFirebaseAppCheckConfig;

    fn validate(&self, config: &Self::Config) -> Result<()> {
        ensure!(
            matches!(
                config.app_attest_environment.as_str(),
                "development" | "production"
            ),
            "masamune-whisker-firebase-app-check: app_attest_environment must be `development` or `production`"
        );
        ensure!(
            matches!(config.provider.as_str(), "auto" | "debug" | "app_attest"),
            "masamune-whisker-firebase-app-check: provider must be `auto`, `debug` or `app_attest`"
        );
        Ok(())
    }

    fn contribute(&self, context: &ProjectContext, config: &Self::Config) -> Result<ProjectUpdate> {
        let ProjectIr::Ios(ios) = &context.project else {
            return Ok(ProjectUpdate::Keep);
        };
        let mut ios = ios.clone();
        let target = ios
            .apple
            .targets
            .get_mut(&ios.apple.application)
            .context("masamune-whisker-firebase-app-check: main iOS application is missing")?;
        target.entitlements.insert(
            APP_ATTEST_ENTITLEMENT.into(),
            PropertyListValue::String(config.app_attest_environment.clone()),
        );
        match config.provider.as_str() {
            "debug" => {
                target.info_plist.insert(
                    PROVIDER_INFO_KEY.into(),
                    PropertyListValue::String("debug".into()),
                );
            }
            "app_attest" => {
                target.info_plist.insert(
                    PROVIDER_INFO_KEY.into(),
                    PropertyListValue::String("appAttest".into()),
                );
            }
            _ => {
                target.info_plist.remove(PROVIDER_INFO_KEY);
            }
        }
        Ok(ProjectUpdate::Replace {
            project: Box::new(ProjectIr::Ios(ios)),
            reason: "Enable App Attest for Firebase App Check".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use whisker_plugin::project::{AppleTarget, IosProjectIr};

    fn context() -> ProjectContext {
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

    fn target(update: ProjectUpdate) -> AppleTarget {
        let ProjectUpdate::Replace { project, .. } = update else {
            panic!("expected a replacement")
        };
        let ProjectIr::Ios(mut ios) = *project else {
            panic!("expected iOS")
        };
        ios.apple.targets.remove("App").unwrap()
    }

    #[test]
    fn adds_the_app_attest_entitlement() {
        let config = MasamuneFirebaseAppCheckConfig::default();
        MasamuneFirebaseAppCheck.validate(&config).unwrap();
        let target = target(
            MasamuneFirebaseAppCheck
                .contribute(&context(), &config)
                .unwrap(),
        );
        assert_eq!(
            target.entitlements[APP_ATTEST_ENTITLEMENT],
            PropertyListValue::String("production".into())
        );
        assert!(!target.info_plist.contains_key(PROVIDER_INFO_KEY));
    }

    #[test]
    fn forces_the_provider_and_rejects_typos() {
        let mut config = MasamuneFirebaseAppCheckConfig::default();
        config
            .app_attest_environment("development")
            .provider("debug");
        let target = target(
            MasamuneFirebaseAppCheck
                .contribute(&context(), &config)
                .unwrap(),
        );
        assert_eq!(
            target.entitlements[APP_ATTEST_ENTITLEMENT],
            PropertyListValue::String("development".into())
        );
        assert_eq!(
            target.info_plist[PROVIDER_INFO_KEY],
            PropertyListValue::String("debug".into())
        );
        config.provider("play_integrity");
        assert!(MasamuneFirebaseAppCheck.validate(&config).is_err());
    }

    #[test]
    fn keeps_android_untouched() {
        let context = ProjectContext {
            project: ProjectIr::Android(Box::default()),
            app_crate_dir: None,
        };
        let update = MasamuneFirebaseAppCheck
            .contribute(&context, &MasamuneFirebaseAppCheckConfig::default())
            .unwrap();
        assert!(matches!(update, ProjectUpdate::Keep));
    }
}
