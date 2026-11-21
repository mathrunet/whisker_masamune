//! CNG plugin that hands the Google Maps API keys to the native SDKs.
//!
//! ```ignore
//! use masamune_whisker_google_maps::cng::MasamuneGoogleMaps;
//!
//! app.project_plugin::<MasamuneGoogleMaps>(|c| {
//!     c.ios_api_key(std::env::var("GOOGLE_MAPS_IOS_API_KEY").unwrap_or_default())
//!         .android_api_key(std::env::var("GOOGLE_MAPS_ANDROID_API_KEY").unwrap_or_default());
//! });
//! ```
//!
//! - iOS: writes the key to `Info.plist` under [`IOS_API_KEY_INFO_KEY`]. The module calls
//!   `GMSServices.provideAPIKey` with it when it is registered at launch, before any map exists.
//! - Android: adds `<meta-data android:name="com.google.android.geo.API_KEY">` to the
//!   application, which the Maps SDK reads.
//!
//! An empty key is skipped, so the plugin changes nothing with the defaults. The keys end up in the
//! app bundle like any client key: restrict them to the app (bundle id / package name and signing
//! certificate) and to the Maps SDK APIs in the Google Cloud console.

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use whisker_plugin::PluginConfig;
use whisker_plugin::project::{
    AndroidManifestEdit, AndroidManifestSelector, AndroidModuleKind, ProjectContext, ProjectIr,
    ProjectPlugin, ProjectUpdate, PropertyListValue, XmlElement, XmlNode,
};

/// `Info.plist` key holding the iOS API key.
pub const IOS_API_KEY_INFO_KEY: &str = "MasamuneGoogleMapsApiKey";
/// Manifest meta-data name the Android Maps SDK reads the key from.
pub const ANDROID_API_KEY_META_DATA: &str = "com.google.android.geo.API_KEY";

/// Settings of [`MasamuneGoogleMaps`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MasamuneGoogleMapsConfig {
    pub ios_api_key: String,
    pub android_api_key: String,
}

impl MasamuneGoogleMapsConfig {
    /// API key of the Maps SDK for iOS.
    pub fn ios_api_key(&mut self, key: impl Into<String>) -> &mut Self {
        self.ios_api_key = key.into().trim().to_string();
        self
    }

    /// API key of the Maps SDK for Android.
    pub fn android_api_key(&mut self, key: impl Into<String>) -> &mut Self {
        self.android_api_key = key.into().trim().to_string();
        self
    }
}

impl PluginConfig for MasamuneGoogleMapsConfig {
    const NAME: &'static str = "masamune-whisker-google-maps";
}

/// The plugin. Register it with `app.project_plugin::<MasamuneGoogleMaps>(|c| …)`.
pub struct MasamuneGoogleMaps;

fn valid_key(key: &str) -> bool {
    key.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

impl ProjectPlugin for MasamuneGoogleMaps {
    type Config = MasamuneGoogleMapsConfig;

    fn validate(&self, config: &Self::Config) -> Result<()> {
        ensure!(
            valid_key(&config.ios_api_key) && valid_key(&config.android_api_key),
            "masamune-whisker-google-maps: API keys may only contain letters, digits, `-` and `_`"
        );
        Ok(())
    }

    fn contribute(&self, context: &ProjectContext, config: &Self::Config) -> Result<ProjectUpdate> {
        self.validate(config)?;
        match &context.project {
            ProjectIr::Ios(ios) if !config.ios_api_key.is_empty() => {
                let mut ios = ios.clone();
                let target = ios
                    .apple
                    .targets
                    .get_mut(&ios.apple.application)
                    .context("masamune-whisker-google-maps: main iOS application is missing")?;
                target.info_plist.insert(
                    IOS_API_KEY_INFO_KEY.into(),
                    PropertyListValue::String(config.ios_api_key.clone()),
                );
                Ok(ProjectUpdate::Replace {
                    project: Box::new(ProjectIr::Ios(ios)),
                    reason: "Provide the Google Maps API key".into(),
                })
            }
            ProjectIr::Android(android) if !config.android_api_key.is_empty() => {
                let mut android = android.clone();
                let id = android.application.clone();
                let module = android.modules.get_mut(&id).context(
                    "masamune-whisker-google-maps: Android application module is missing",
                )?;
                let AndroidModuleKind::Application(app) = &mut module.kind else {
                    bail!("masamune-whisker-google-maps: {id} is not an application module");
                };
                let main = app
                    .android
                    .source_sets
                    .get_mut("main")
                    .context("masamune-whisker-google-maps: the main source set is missing")?;
                main.edit_manifest(&[AndroidManifestEdit::Upsert {
                    path: vec![AndroidManifestSelector {
                        name: "application".into(),
                        attributes: BTreeMap::new(),
                    }],
                    attributes: BTreeMap::new(),
                    append: vec![XmlNode::Element(XmlElement {
                        name: "meta-data".into(),
                        attributes: [
                            (
                                "android:name".to_string(),
                                ANDROID_API_KEY_META_DATA.to_string(),
                            ),
                            ("android:value".to_string(), config.android_api_key.clone()),
                        ]
                        .into(),
                        children: Vec::new(),
                    })],
                }])?;
                Ok(ProjectUpdate::Replace {
                    project: Box::new(ProjectIr::Android(android)),
                    reason: "Provide the Google Maps API key".into(),
                })
            }
            _ => Ok(ProjectUpdate::Keep),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use whisker_plugin::project::{
        AndroidApplication, AndroidBuild, AndroidModule, AndroidProjectIr, AndroidSourceSet,
        AppleTarget, IosProjectIr, ProjectPath,
    };

    fn config() -> MasamuneGoogleMapsConfig {
        let mut config = MasamuneGoogleMapsConfig::default();
        config
            .ios_api_key(" AIza-ios_1 ")
            .android_api_key("AIza-android");
        config
    }

    fn contribute(project: ProjectIr, config: &MasamuneGoogleMapsConfig) -> ProjectUpdate {
        MasamuneGoogleMaps
            .contribute(
                &ProjectContext {
                    project,
                    app_crate_dir: None,
                },
                config,
            )
            .unwrap()
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
        let mut manifest = XmlElement::new("manifest");
        manifest.attributes.insert(
            "xmlns:android".into(),
            "http://schemas.android.com/apk/res/android".into(),
        );
        manifest
            .children
            .push(XmlNode::Element(XmlElement::new("application")));
        let build = AndroidBuild {
            namespace: "net.mathru.app".into(),
            sdk: Default::default(),
            default_config: Default::default(),
            default_config_statements: vec![],
            variants: Default::default(),
            source_sets: [(
                "main".to_string(),
                AndroidSourceSet {
                    manifest: Some(manifest),
                    ..Default::default()
                },
            )]
            .into(),
            statements: vec![],
        };
        let module = AndroidModule {
            directory: ProjectPath::new("app").unwrap(),
            kind: AndroidModuleKind::Application(AndroidApplication {
                android: build,
                application_id: Some("net.mathru.app".into()),
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

    #[test]
    fn writes_the_ios_key() {
        let ProjectUpdate::Replace { project, .. } = contribute(ios(), &config()) else {
            panic!("expected a replacement")
        };
        let ProjectIr::Ios(ios) = *project else {
            panic!("expected iOS")
        };
        assert_eq!(
            ios.apple.targets["App"].info_plist[IOS_API_KEY_INFO_KEY],
            PropertyListValue::String("AIza-ios_1".into())
        );
    }

    #[test]
    fn adds_the_android_meta_data() {
        let ProjectUpdate::Replace { project, .. } = contribute(android(), &config()) else {
            panic!("expected a replacement")
        };
        let ProjectIr::Android(android) = *project else {
            panic!("expected Android")
        };
        let AndroidModuleKind::Application(app) = &android.modules[":app"].kind else {
            panic!("expected the application")
        };
        let manifest = app.android.source_sets["main"].manifest.as_ref().unwrap();
        let XmlNode::Element(application) = &manifest.children[0] else {
            panic!("expected the application element")
        };
        let XmlNode::Element(meta) = &application.children[0] else {
            panic!("expected meta-data")
        };
        assert_eq!(meta.attributes["android:name"], ANDROID_API_KEY_META_DATA);
        assert_eq!(meta.attributes["android:value"], "AIza-android");
    }

    #[test]
    fn empty_keys_change_nothing_and_bad_keys_fail() {
        let empty = MasamuneGoogleMapsConfig::default();
        assert!(matches!(contribute(ios(), &empty), ProjectUpdate::Keep));
        assert!(matches!(contribute(android(), &empty), ProjectUpdate::Keep));
        let mut bad = MasamuneGoogleMapsConfig::default();
        bad.ios_api_key("key\"><x");
        assert!(MasamuneGoogleMaps.validate(&bad).is_err());
    }
}
