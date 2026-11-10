//! CNG plugin that registers the Auth0 callback URL scheme.
//!
//! ```ignore
//! use masamune_whisker_auth0::cng::MasamuneAuth0;
//!
//! app.project_plugin::<MasamuneAuth0>(|c| {
//!     c.scheme("com.example.app").domain("example.jp.auth0.com");
//! });
//! ```
//!
//! - iOS: adds `scheme` to `CFBundleURLTypes`.
//! - Android: sets the `auth0Scheme` and `auth0Domain` manifest placeholders in the application
//!   module's `defaultConfig`. Auth0.Android's own manifest declares `RedirectActivity` with an
//!   intent filter for `${auth0Scheme}://${auth0Domain}/android/${applicationId}/callback`, so
//!   no manifest edit is needed.

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use whisker_plugin::PluginConfig;
use whisker_plugin::project::{
    AndroidModuleKind, ProjectContext, ProjectIr, ProjectPlugin, ProjectUpdate, PropertyListValue,
};

/// Manifest placeholder of the callback scheme read by Auth0.Android.
pub const SCHEME_PLACEHOLDER: &str = "auth0Scheme";
/// Manifest placeholder of the tenant domain read by Auth0.Android.
pub const DOMAIN_PLACEHOLDER: &str = "auth0Domain";

/// Settings of [`MasamuneAuth0`]. Use the same values as [`crate::Auth0Config`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MasamuneAuth0Config {
    /// Callback URL scheme (lowercase), for example the bundle / application id.
    pub scheme: String,
    /// Tenant domain, for example `example.jp.auth0.com`.
    pub domain: String,
}

impl MasamuneAuth0Config {
    pub fn scheme(&mut self, scheme: impl Into<String>) -> &mut Self {
        self.scheme = scheme.into();
        self
    }

    pub fn domain(&mut self, domain: impl Into<String>) -> &mut Self {
        self.domain = domain.into();
        self
    }
}

impl PluginConfig for MasamuneAuth0Config {
    const NAME: &'static str = "masamune-whisker-auth0";
}

/// The plugin. Register it with `app.project_plugin::<MasamuneAuth0>(|c| …)`.
pub struct MasamuneAuth0;

impl ProjectPlugin for MasamuneAuth0 {
    type Config = MasamuneAuth0Config;

    fn validate(&self, config: &Self::Config) -> Result<()> {
        let scheme = config.scheme.trim();
        ensure!(
            !scheme.is_empty(),
            "masamune-whisker-auth0: scheme is required"
        );
        ensure!(
            scheme
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "+-.".contains(c))
                && scheme.starts_with(|c: char| c.is_ascii_lowercase()),
            "masamune-whisker-auth0: scheme must be a lowercase URL scheme (Auth0.Android requires lowercase)"
        );
        let domain = config.domain.trim();
        ensure!(
            !domain.is_empty(),
            "masamune-whisker-auth0: domain is required"
        );
        ensure!(
            !domain.contains("://") && !domain.contains('/') && !domain.contains('"'),
            "masamune-whisker-auth0: domain must be a host name such as example.jp.auth0.com"
        );
        Ok(())
    }

    fn contribute(&self, context: &ProjectContext, config: &Self::Config) -> Result<ProjectUpdate> {
        self.validate(config)?;
        let scheme = config.scheme.trim();
        let domain = config.domain.trim();
        match &context.project {
            ProjectIr::Ios(ios) => {
                let mut ios = ios.clone();
                let target = ios
                    .apple
                    .targets
                    .get_mut(&ios.apple.application)
                    .context("masamune-whisker-auth0: main iOS application is missing")?;
                add_url_scheme(&mut target.info_plist, scheme)?;
                Ok(ProjectUpdate::Replace {
                    project: Box::new(ProjectIr::Ios(ios)),
                    reason: "Register the Auth0 callback URL scheme".into(),
                })
            }
            ProjectIr::Android(android) => {
                let mut android = android.clone();
                let id = android.application.clone();
                let module = android
                    .modules
                    .get_mut(&id)
                    .context("masamune-whisker-auth0: Android application module is missing")?;
                let AndroidModuleKind::Application(app) = &mut module.kind else {
                    bail!("masamune-whisker-auth0: {id} is not an application module");
                };
                let placeholders = &mut app.android.default_config.manifest_placeholders;
                // Values are Kotlin expressions.
                placeholders.insert(SCHEME_PLACEHOLDER.into(), kotlin_string(scheme));
                placeholders.insert(DOMAIN_PLACEHOLDER.into(), kotlin_string(domain));
                Ok(ProjectUpdate::Replace {
                    project: Box::new(ProjectIr::Android(android)),
                    reason: "Set the Auth0 redirect manifest placeholders".into(),
                })
            }
            _ => Ok(ProjectUpdate::Keep),
        }
    }
}

fn kotlin_string(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('$', "\\$")
    )
}

/// Adds `scheme` to `CFBundleURLTypes` unless an entry already declares it.
fn add_url_scheme(
    info_plist: &mut std::collections::BTreeMap<String, PropertyListValue>,
    scheme: &str,
) -> Result<()> {
    let wanted = PropertyListValue::String(scheme.into());
    let PropertyListValue::Array(types) = info_plist
        .entry("CFBundleURLTypes".into())
        .or_insert_with(|| PropertyListValue::Array(Vec::new()))
    else {
        bail!("CFBundleURLTypes must be an array");
    };
    let declared = types.iter().any(|entry| match entry {
        PropertyListValue::Dict(entry) => matches!(
            entry.get("CFBundleURLSchemes"),
            Some(PropertyListValue::Array(schemes)) if schemes.contains(&wanted)
        ),
        _ => false,
    });
    if !declared {
        types.push(PropertyListValue::Dict(
            [
                (
                    "CFBundleTypeRole".to_string(),
                    PropertyListValue::String("None".into()),
                ),
                (
                    "CFBundleURLName".to_string(),
                    PropertyListValue::String("auth0".into()),
                ),
                (
                    "CFBundleURLSchemes".to_string(),
                    PropertyListValue::Array(vec![wanted]),
                ),
            ]
            .into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use whisker_plugin::project::{
        AndroidApplication, AndroidBuild, AndroidModule, AndroidProjectIr, AppleTarget,
        IosProjectIr, ProjectPath,
    };

    fn config() -> MasamuneAuth0Config {
        let mut config = MasamuneAuth0Config::default();
        config
            .scheme("net.mathru.example")
            .domain("example.jp.auth0.com");
        config
    }

    fn ios() -> ProjectIr {
        let mut ios = IosProjectIr::default();
        ios.apple.application = "App".into();
        ios.apple
            .targets
            .insert("App".into(), AppleTarget::default());
        ProjectIr::Ios(ios)
    }

    fn android() -> ProjectIr {
        let module = AndroidModule {
            directory: ProjectPath::new("app").unwrap(),
            kind: AndroidModuleKind::Application(AndroidApplication {
                android: AndroidBuild {
                    namespace: "net.mathru.example".into(),
                    sdk: Default::default(),
                    default_config: Default::default(),
                    default_config_statements: vec![],
                    variants: Default::default(),
                    source_sets: Default::default(),
                    statements: vec![],
                },
                application_id: Some("net.mathru.example".into()),
                dynamic_features: vec![],
                asset_packs: vec![],
            }),
            build: Default::default(),
            dependencies: vec![],
        };
        ProjectIr::Android(Box::new(AndroidProjectIr {
            application: ":app".into(),
            modules: [(":app".to_string(), module)].into(),
            ..Default::default()
        }))
    }

    fn contribute(project: ProjectIr, config: &MasamuneAuth0Config) -> ProjectIr {
        let context = ProjectContext {
            project,
            app_crate_dir: None,
        };
        let ProjectUpdate::Replace { project, .. } =
            MasamuneAuth0.contribute(&context, config).unwrap()
        else {
            panic!("expected a replacement")
        };
        *project
    }

    #[test]
    fn registers_the_scheme_once_on_ios() {
        let once = contribute(ios(), &config());
        let ProjectIr::Ios(ios) = contribute(once, &config()) else {
            panic!("expected iOS")
        };
        let PropertyListValue::Array(types) =
            &ios.apple.targets["App"].info_plist["CFBundleURLTypes"]
        else {
            panic!("expected an array")
        };
        assert_eq!(types.len(), 1);
        let PropertyListValue::Dict(entry) = &types[0] else {
            panic!("expected a dictionary")
        };
        assert_eq!(
            entry["CFBundleURLSchemes"],
            PropertyListValue::Array(vec![PropertyListValue::String("net.mathru.example".into())])
        );
    }

    #[test]
    fn sets_manifest_placeholders_on_android() {
        let ProjectIr::Android(android) = contribute(android(), &config()) else {
            panic!("expected Android")
        };
        let AndroidModuleKind::Application(app) = &android.modules[":app"].kind else {
            panic!("expected an application")
        };
        let placeholders = &app.android.default_config.manifest_placeholders;
        assert_eq!(placeholders["auth0Scheme"], "\"net.mathru.example\"");
        assert_eq!(placeholders["auth0Domain"], "\"example.jp.auth0.com\"");
    }

    #[test]
    fn rejects_invalid_settings() {
        assert!(MasamuneAuth0.validate(&Default::default()).is_err());
        let mut upper = config();
        upper.scheme("Net.Example");
        assert!(MasamuneAuth0.validate(&upper).is_err());
        let mut url = config();
        url.domain("https://example.jp.auth0.com");
        assert!(MasamuneAuth0.validate(&url).is_err());
        assert!(MasamuneAuth0.validate(&config()).is_ok());
    }
}
