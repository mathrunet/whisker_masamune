//! CNG plugin that enables Sign in with Apple on iOS (a no-op on Android).
//!
//! ```ignore
//! use masamune_whisker_apple_sign_in::cng::MasamuneAppleSignIn;
//!
//! app.project_plugin::<MasamuneAppleSignIn>(|_| {});
//! ```
//!
//! It adds the `com.apple.developer.applesignin = ["Default"]` entitlement to the main iOS app.
//! The capability also has to be enabled for the App ID (automatic signing does this).

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use whisker_plugin::PluginConfig;
use whisker_plugin::project::{
    ProjectContext, ProjectIr, ProjectPlugin, ProjectUpdate, PropertyListValue,
};

/// The Sign in with Apple entitlement.
pub const APPLE_SIGN_IN_ENTITLEMENT: &str = "com.apple.developer.applesignin";

/// Settings of [`MasamuneAppleSignIn`] (none today).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MasamuneAppleSignInConfig {}

impl PluginConfig for MasamuneAppleSignInConfig {
    const NAME: &'static str = "masamune-whisker-apple-sign-in";
}

/// The plugin. Register it with `app.project_plugin::<MasamuneAppleSignIn>(|_| {})`.
pub struct MasamuneAppleSignIn;

impl ProjectPlugin for MasamuneAppleSignIn {
    type Config = MasamuneAppleSignInConfig;

    fn contribute(
        &self,
        context: &ProjectContext,
        _config: &Self::Config,
    ) -> Result<ProjectUpdate> {
        let ProjectIr::Ios(ios) = &context.project else {
            return Ok(ProjectUpdate::Keep);
        };
        let mut ios = ios.clone();
        let target = ios
            .apple
            .targets
            .get_mut(&ios.apple.application)
            .context("masamune-whisker-apple-sign-in: main iOS application is missing")?;
        target.entitlements.insert(
            APPLE_SIGN_IN_ENTITLEMENT.into(),
            PropertyListValue::Array(vec![PropertyListValue::String("Default".into())]),
        );
        Ok(ProjectUpdate::Replace {
            project: Box::new(ProjectIr::Ios(ios)),
            reason: "Enable Sign in with Apple".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use whisker_plugin::project::{AppleTarget, IosProjectIr};

    #[test]
    fn adds_the_entitlement_on_ios_only() {
        let mut ios = IosProjectIr::default();
        ios.apple.application = "App".into();
        ios.apple
            .targets
            .insert("App".into(), AppleTarget::default());
        let context = ProjectContext {
            project: ProjectIr::Ios(ios),
            app_crate_dir: None,
        };
        let ProjectUpdate::Replace { project, .. } = MasamuneAppleSignIn
            .contribute(&context, &Default::default())
            .unwrap()
        else {
            panic!("expected a replacement")
        };
        let ProjectIr::Ios(ios) = *project else {
            panic!("expected iOS")
        };
        assert_eq!(
            ios.apple.targets["App"].entitlements[APPLE_SIGN_IN_ENTITLEMENT],
            PropertyListValue::Array(vec![PropertyListValue::String("Default".into())])
        );

        let android = ProjectContext {
            project: ProjectIr::Android(Box::default()),
            app_crate_dir: None,
        };
        assert!(matches!(
            MasamuneAppleSignIn
                .contribute(&android, &Default::default())
                .unwrap(),
            ProjectUpdate::Keep
        ));
    }
}
