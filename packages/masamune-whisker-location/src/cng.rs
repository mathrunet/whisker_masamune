//! CNG plugin that writes the location usage descriptions and background settings.
//!
//! ```ignore
//! use masamune_whisker_location::cng::MasamuneLocation;
//!
//! app.project_plugin::<MasamuneLocation>(|c| {
//!     c.when_in_use_usage("Used to match the music to where you are.")
//!         .always_usage("Used to keep the soundtrack in step while the screen is off.")
//!         .background(true);
//! });
//! ```
//!
//! - iOS: `NSLocationWhenInUseUsageDescription` and `NSLocationAlwaysAndWhenInUseUsageDescription`
//!   when set (iOS terminates the app when it asks without them), and `location` in
//!   `UIBackgroundModes` with [`MasamuneLocationConfig::background`].
//! - Android: `ACCESS_BACKGROUND_LOCATION` with [`MasamuneLocationConfig::background_permission`].
//!   The crate's own manifest already declares `ACCESS_FINE_LOCATION`, `ACCESS_COARSE_LOCATION`,
//!   `FOREGROUND_SERVICE`, `FOREGROUND_SERVICE_LOCATION` and the `location` foreground service.
//!
//! With the defaults the plugin changes nothing, so it is safe when the crate is only a
//! dependency.

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use whisker_plugin::PluginConfig;
use whisker_plugin::project::{
    AndroidManifestEdit, AndroidModuleKind, ProjectContext, ProjectIr, ProjectPlugin,
    ProjectUpdate, PropertyListValue, XmlElement, XmlNode,
};

/// The Android background location permission.
pub const BACKGROUND_LOCATION_PERMISSION: &str = "android.permission.ACCESS_BACKGROUND_LOCATION";

/// Settings of [`MasamuneLocation`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MasamuneLocationConfig {
    /// `NSLocationWhenInUseUsageDescription`.
    pub when_in_use_usage: Option<String>,
    /// `NSLocationAlwaysAndWhenInUseUsageDescription`.
    pub always_usage: Option<String>,
    /// Add `location` to the iOS `UIBackgroundModes`.
    pub background: bool,
    /// Declare `ACCESS_BACKGROUND_LOCATION` on Android.
    pub background_permission: bool,
}

impl MasamuneLocationConfig {
    /// Why the app reads the location while in use (required before asking on iOS).
    pub fn when_in_use_usage(&mut self, description: impl Into<String>) -> &mut Self {
        self.when_in_use_usage = Some(description.into());
        self
    }

    /// Why the app reads the location at any time (required before asking for "Always").
    pub fn always_usage(&mut self, description: impl Into<String>) -> &mut Self {
        self.always_usage = Some(description.into());
        self
    }

    /// Receive positions in the background on iOS (`UIBackgroundModes` `location`). Needed for
    /// `LocationSettings::background`.
    pub fn background(&mut self, enabled: bool) -> &mut Self {
        self.background = enabled;
        self
    }

    /// Declare `ACCESS_BACKGROUND_LOCATION` on Android. Only for apps that read the location
    /// without a foreground service; Google Play reviews this permission.
    pub fn background_permission(&mut self, enabled: bool) -> &mut Self {
        self.background_permission = enabled;
        self
    }
}

impl PluginConfig for MasamuneLocationConfig {
    const NAME: &'static str = "masamune-whisker-location";
}

/// The plugin. Register it with `app.project_plugin::<MasamuneLocation>(|c| …)`.
pub struct MasamuneLocation;

impl ProjectPlugin for MasamuneLocation {
    type Config = MasamuneLocationConfig;

    fn validate(&self, config: &Self::Config) -> Result<()> {
        for (key, value) in [
            ("when_in_use_usage", &config.when_in_use_usage),
            ("always_usage", &config.always_usage),
        ] {
            ensure!(
                value.as_deref().is_none_or(|text| !text.trim().is_empty()),
                "masamune-whisker-location: {key} must not be empty"
            );
        }
        Ok(())
    }

    fn contribute(&self, context: &ProjectContext, config: &Self::Config) -> Result<ProjectUpdate> {
        self.validate(config)?;
        match &context.project {
            ProjectIr::Ios(ios) => {
                if config.when_in_use_usage.is_none()
                    && config.always_usage.is_none()
                    && !config.background
                {
                    return Ok(ProjectUpdate::Keep);
                }
                let mut ios = ios.clone();
                let target = ios
                    .apple
                    .targets
                    .get_mut(&ios.apple.application)
                    .context("masamune-whisker-location: main iOS application is missing")?;
                let plist = &mut target.info_plist;
                for (key, value) in [
                    (
                        "NSLocationWhenInUseUsageDescription",
                        &config.when_in_use_usage,
                    ),
                    (
                        "NSLocationAlwaysAndWhenInUseUsageDescription",
                        &config.always_usage,
                    ),
                ] {
                    if let Some(value) = value {
                        plist.insert(key.into(), PropertyListValue::String(value.clone()));
                    }
                }
                if config.background {
                    add_array_item(plist, "UIBackgroundModes", "location")?;
                }
                Ok(ProjectUpdate::Replace {
                    project: Box::new(ProjectIr::Ios(ios)),
                    reason: "Describe the location use".into(),
                })
            }
            ProjectIr::Android(android) if config.background_permission => {
                let mut android = android.clone();
                let id = android.application.clone();
                let module = android
                    .modules
                    .get_mut(&id)
                    .context("masamune-whisker-location: Android application module is missing")?;
                let AndroidModuleKind::Application(app) = &mut module.kind else {
                    bail!("masamune-whisker-location: {id} is not an application module");
                };
                let main = app
                    .android
                    .source_sets
                    .get_mut("main")
                    .context("masamune-whisker-location: the main source set is missing")?;
                main.edit_manifest(&[AndroidManifestEdit::Upsert {
                    path: Vec::new(),
                    attributes: BTreeMap::new(),
                    append: vec![uses_permission(BACKGROUND_LOCATION_PERMISSION)],
                }])?;
                Ok(ProjectUpdate::Replace {
                    project: Box::new(ProjectIr::Android(android)),
                    reason: "Declare the background location permission".into(),
                })
            }
            _ => Ok(ProjectUpdate::Keep),
        }
    }
}

fn uses_permission(name: &str) -> XmlNode {
    XmlNode::Element(XmlElement {
        name: "uses-permission".into(),
        attributes: [("android:name".to_string(), name.to_string())].into(),
        children: Vec::new(),
    })
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
    use whisker_plugin::project::{
        AndroidApplication, AndroidBuild, AndroidModule, AndroidProjectIr, AndroidSourceSet,
        AppleTarget, IosProjectIr, ProjectPath,
    };

    fn ios() -> ProjectContext {
        let mut ios = IosProjectIr::default();
        ios.apple.application = "App".into();
        let mut app = AppleTarget::default();
        app.info_plist.insert(
            "UIBackgroundModes".into(),
            PropertyListValue::Array(vec![PropertyListValue::String("audio".into())]),
        );
        ios.apple.targets.insert("App".into(), app);
        ProjectContext {
            project: ProjectIr::Ios(ios),
            app_crate_dir: None,
        }
    }

    fn replaced(update: ProjectUpdate) -> ProjectIr {
        match update {
            ProjectUpdate::Replace { project, .. } => *project,
            _ => panic!("expected a replacement"),
        }
    }

    #[test]
    fn writes_ios_descriptions_and_background_mode() {
        let mut config = MasamuneLocationConfig::default();
        config
            .when_in_use_usage("in use")
            .always_usage("always")
            .background(true);
        let ProjectIr::Ios(ios) = replaced(MasamuneLocation.contribute(&ios(), &config).unwrap())
        else {
            panic!("expected iOS")
        };
        let plist = &ios.apple.targets["App"].info_plist;
        assert_eq!(
            plist["NSLocationWhenInUseUsageDescription"],
            PropertyListValue::String("in use".into())
        );
        assert_eq!(
            plist["NSLocationAlwaysAndWhenInUseUsageDescription"],
            PropertyListValue::String("always".into())
        );
        assert_eq!(
            plist["UIBackgroundModes"],
            PropertyListValue::Array(vec![
                PropertyListValue::String("audio".into()),
                PropertyListValue::String("location".into()),
            ])
        );
    }

    #[test]
    fn defaults_change_nothing_and_empty_text_is_rejected() {
        assert!(matches!(
            MasamuneLocation
                .contribute(&ios(), &Default::default())
                .unwrap(),
            ProjectUpdate::Keep
        ));
        let mut config = MasamuneLocationConfig::default();
        config.when_in_use_usage(" ");
        assert!(MasamuneLocation.validate(&config).is_err());
    }

    #[test]
    fn adds_the_background_permission_on_android() {
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
        let android = AndroidProjectIr {
            application: ":app".into(),
            modules: [(":app".to_string(), module)].into(),
            ..Default::default()
        };
        let context = ProjectContext {
            project: ProjectIr::Android(Box::new(android)),
            app_crate_dir: None,
        };
        assert!(matches!(
            MasamuneLocation
                .contribute(&context, &Default::default())
                .unwrap(),
            ProjectUpdate::Keep
        ));
        let mut config = MasamuneLocationConfig::default();
        config.background_permission(true);
        let ProjectIr::Android(android) =
            replaced(MasamuneLocation.contribute(&context, &config).unwrap())
        else {
            panic!("expected Android")
        };
        let AndroidModuleKind::Application(app) = &android.modules[":app"].kind else {
            panic!("expected the application")
        };
        let manifest = app.android.source_sets["main"].manifest.as_ref().unwrap();
        assert!(manifest.children.iter().any(|node| matches!(
            node,
            XmlNode::Element(element)
                if element.name == "uses-permission"
                    && element.attributes["android:name"] == BACKGROUND_LOCATION_PERMISSION
        )));
    }
}
