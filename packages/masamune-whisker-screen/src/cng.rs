//! CNG plugin that declares the main activity's `configChanges` (and orientation).
//!
//! ```ignore
//! use masamune_whisker_screen::ConfigChanges;
//! use masamune_whisker_screen::cng::MasamuneScreen;
//!
//! app.project_plugin::<MasamuneScreen>(|c| {
//!     c.config_changes(ConfigChanges::RESIZE | ConfigChanges::KEYBOARD | ConfigChanges::UI_MODE | ConfigChanges::LOCALE);
//! });
//! ```
//!
//! The plugin also runs with the defaults when the crate is a dependency and whisker.rs does not
//! mention it. On Android it sets `android:configChanges` (and `android:screenOrientation`) on
//! `MainActivity` in the generated `AndroidManifest.xml`. `configChanges` exist only on Android:
//! iOS never recreates the app for these changes.
//!
//! On iOS the supported orientations come from `Info.plist`. Leave them unset to allow every
//! orientation, or restrict them:
//!
//! ```ignore
//! app.project_plugin::<MasamuneScreen>(|c| {
//!     c.ios_orientations(&[IosOrientation::Portrait, IosOrientation::LandscapeLeft, IosOrientation::LandscapeRight]);
//! });
//! ```

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use whisker_plugin::PluginConfig;
use whisker_plugin::project::{
    AndroidManifestAttributeEdit, AndroidManifestEdit, AndroidManifestSelector, AndroidModuleKind,
    ProjectContext, ProjectIr, ProjectPlugin, ProjectUpdate, PropertyListValue, XmlElement,
    XmlNode,
};

use crate::ConfigChanges;

const MAIN_ACTIVITY: &str = ".MainActivity";

/// Settings of [`MasamuneScreen`].
#[derive(Default, Serialize, Deserialize)]
pub struct MasamuneScreenConfig {
    /// Bits of [`ConfigChanges`]. Defaults to `RESIZE | KEYBOARD`.
    #[serde(default)]
    pub config_changes: Option<u32>,
    /// `android:screenOrientation` (for example `"portrait"`).
    #[serde(default)]
    pub screen_orientation: Option<String>,
    /// iPhone orientations (`UISupportedInterfaceOrientations`). `None` keeps the default (all).
    #[serde(default)]
    pub ios_orientations: Option<Vec<IosOrientation>>,
    /// iPad orientations (`UISupportedInterfaceOrientations~ipad`). `None` keeps the default
    /// (all, which iPad multitasking requires).
    #[serde(default)]
    pub ipad_orientations: Option<Vec<IosOrientation>>,
}

/// An iOS interface orientation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IosOrientation {
    Portrait,
    PortraitUpsideDown,
    LandscapeLeft,
    LandscapeRight,
}

impl IosOrientation {
    /// The `Info.plist` value.
    pub fn plist_value(self) -> &'static str {
        match self {
            Self::Portrait => "UIInterfaceOrientationPortrait",
            Self::PortraitUpsideDown => "UIInterfaceOrientationPortraitUpsideDown",
            Self::LandscapeLeft => "UIInterfaceOrientationLandscapeLeft",
            Self::LandscapeRight => "UIInterfaceOrientationLandscapeRight",
        }
    }
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

    /// Orientations an iPhone may use.
    pub fn ios_orientations(&mut self, orientations: &[IosOrientation]) -> &mut Self {
        self.ios_orientations = Some(orientations.to_vec());
        self
    }

    /// Orientations an iPad may use. Restricting them opts the app out of iPad multitasking.
    pub fn ipad_orientations(&mut self, orientations: &[IosOrientation]) -> &mut Self {
        self.ipad_orientations = Some(orientations.to_vec());
        self
    }
}

impl PluginConfig for MasamuneScreenConfig {
    const NAME: &'static str = "masamune-whisker-screen";
}

/// The plugin. Register it with `app.project_plugin::<MasamuneScreen>(|c| …)`.
pub struct MasamuneScreen;

impl ProjectPlugin for MasamuneScreen {
    type Config = MasamuneScreenConfig;

    fn validate(&self, config: &Self::Config) -> Result<()> {
        if let Some(orientation) = &config.screen_orientation {
            ensure!(
                !orientation.is_empty()
                    && orientation
                        .chars()
                        .all(|c| c.is_ascii_alphabetic() || c == '_'),
                "masamune-whisker-screen: invalid screen orientation {orientation:?}"
            );
        }
        Ok(())
    }

    fn contribute(&self, context: &ProjectContext, config: &Self::Config) -> Result<ProjectUpdate> {
        match &context.project {
            ProjectIr::Ios(ios) => {
                if config.ios_orientations.is_none() && config.ipad_orientations.is_none() {
                    return Ok(ProjectUpdate::Keep);
                }
                let mut ios = ios.clone();
                let target = ios
                    .apple
                    .targets
                    .get_mut(&ios.apple.application)
                    .context("masamune-whisker-screen: main iOS application is missing")?;
                for (key, orientations) in [
                    ("UISupportedInterfaceOrientations", &config.ios_orientations),
                    (
                        "UISupportedInterfaceOrientations~ipad",
                        &config.ipad_orientations,
                    ),
                ] {
                    if let Some(orientations) = orientations {
                        target.info_plist.insert(
                            key.into(),
                            PropertyListValue::Array(
                                orientations
                                    .iter()
                                    .map(|o| PropertyListValue::String(o.plist_value().into()))
                                    .collect(),
                            ),
                        );
                    }
                }
                Ok(ProjectUpdate::Replace {
                    project: Box::new(ProjectIr::Ios(ios)),
                    reason: "Restrict the supported interface orientations".into(),
                })
            }
            ProjectIr::Android(android) => {
                let mut android = android.clone();
                let id = android.application.clone();
                let module = android
                    .modules
                    .get_mut(&id)
                    .context("masamune-whisker-screen: Android application module is missing")?;
                let AndroidModuleKind::Application(app) = &mut module.kind else {
                    bail!("masamune-whisker-screen: {id} is not an application module");
                };
                let main = app
                    .android
                    .source_sets
                    .get_mut("main")
                    .context("masamune-whisker-screen: the main source set is missing")?;
                let manifest = main
                    .manifest
                    .as_ref()
                    .context("masamune-whisker-screen: AndroidManifest.xml is missing")?;
                ensure!(
                    main_activity(manifest).is_some(),
                    "masamune-whisker-screen: MainActivity is missing from AndroidManifest.xml"
                );
                main.edit_manifest(&[android_edit(config)])?;
                Ok(ProjectUpdate::Replace {
                    project: Box::new(ProjectIr::Android(android)),
                    reason: "Declare the configChanges MainActivity handles itself".into(),
                })
            }
            _ => Ok(ProjectUpdate::Keep),
        }
    }
}

fn child<'a>(
    element: &'a XmlElement,
    name: &str,
    android_name: Option<&str>,
) -> Option<&'a XmlElement> {
    element.children.iter().find_map(|node| match node {
        XmlNode::Element(child)
            if child.name == name
                && android_name.is_none_or(|wanted| {
                    child.attributes.get("android:name").map(String::as_str) == Some(wanted)
                }) =>
        {
            Some(child)
        }
        _ => None,
    })
}

fn main_activity(manifest: &XmlElement) -> Option<&XmlElement> {
    child(manifest, "application", None)
        .and_then(|application| child(application, "activity", Some(MAIN_ACTIVITY)))
}

/// Sets `configChanges` (default `RESIZE | KEYBOARD`) and the optional orientation on
/// `MainActivity`.
fn android_edit(config: &MasamuneScreenConfig) -> AndroidManifestEdit {
    let changes = config
        .config_changes
        .map(ConfigChanges::from_bits)
        .unwrap_or(ConfigChanges::RESIZE | ConfigChanges::KEYBOARD);
    let mut attributes = BTreeMap::from([(
        "android:configChanges".to_string(),
        AndroidManifestAttributeEdit::Override(changes.android_value()),
    )]);
    if let Some(orientation) = &config.screen_orientation {
        attributes.insert(
            "android:screenOrientation".to_string(),
            AndroidManifestAttributeEdit::Override(orientation.clone()),
        );
    }
    AndroidManifestEdit::Upsert {
        path: vec![
            AndroidManifestSelector {
                name: "application".into(),
                attributes: BTreeMap::new(),
            },
            AndroidManifestSelector {
                name: "activity".into(),
                attributes: [("android:name".to_string(), MAIN_ACTIVITY.to_string())].into(),
            },
        ],
        attributes,
        append: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use whisker_plugin::project::{
        AndroidApplication, AndroidBuild, AndroidModule, AndroidProjectIr, AndroidSourceSet,
        AppleTarget, IosProjectIr, ProjectPath,
    };

    fn contribute(project: ProjectIr, config: &MasamuneScreenConfig) -> ProjectIr {
        let context = ProjectContext {
            project,
            app_crate_dir: None,
        };
        match MasamuneScreen.contribute(&context, config).unwrap() {
            ProjectUpdate::Replace { project, .. } => *project,
            other => panic!("expected Replace, got {other:?}"),
        }
    }

    fn android() -> ProjectIr {
        let mut activity = XmlElement::new("activity");
        activity
            .attributes
            .insert("android:name".into(), MAIN_ACTIVITY.into());
        let mut application = XmlElement::new("application");
        application.children.push(XmlNode::Element(activity));
        let mut manifest = XmlElement::new("manifest");
        manifest.attributes.insert(
            "xmlns:android".into(),
            "http://schemas.android.com/apk/res/android".into(),
        );
        manifest.children.push(XmlNode::Element(application));
        let module = AndroidModule {
            directory: ProjectPath::new("app").unwrap(),
            kind: AndroidModuleKind::Application(AndroidApplication {
                android: AndroidBuild {
                    namespace: "net.mathru.example".into(),
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

    fn activity(project: &ProjectIr) -> XmlElement {
        let ProjectIr::Android(android) = project else {
            panic!("expected Android")
        };
        let AndroidModuleKind::Application(app) = &android.modules[":app"].kind else {
            panic!("expected an application module")
        };
        main_activity(app.android.source_sets["main"].manifest.as_ref().unwrap())
            .unwrap()
            .clone()
    }

    #[test]
    fn declares_default_config_changes_on_the_main_activity() {
        let project = contribute(android(), &MasamuneScreenConfig::default());
        let activity = activity(&project);
        assert_eq!(
            activity.attributes["android:configChanges"],
            (ConfigChanges::RESIZE | ConfigChanges::KEYBOARD).android_value()
        );
        assert!(
            !activity
                .attributes
                .contains_key("android:screenOrientation")
        );
        assert_eq!(
            contribute(project.clone(), &MasamuneScreenConfig::default()),
            project
        );
    }

    #[test]
    fn declares_configured_changes_and_orientation() {
        let mut config = MasamuneScreenConfig::default();
        config
            .config_changes(ConfigChanges::RESIZE | ConfigChanges::LOCALE)
            .screen_orientation("portrait");
        let activity = activity(&contribute(android(), &config));
        assert_eq!(
            activity.attributes["android:configChanges"],
            (ConfigChanges::RESIZE | ConfigChanges::LOCALE).android_value()
        );
        assert_eq!(activity.attributes["android:screenOrientation"], "portrait");
    }

    #[test]
    fn restricts_ios_orientations_only_when_configured() {
        let mut ios = IosProjectIr::default();
        ios.apple.application = "App".into();
        ios.apple
            .targets
            .insert("App".into(), AppleTarget::default());
        let context = ProjectContext {
            project: ProjectIr::Ios(ios.clone()),
            app_crate_dir: None,
        };
        assert!(matches!(
            MasamuneScreen
                .contribute(&context, &MasamuneScreenConfig::default())
                .unwrap(),
            ProjectUpdate::Keep
        ));

        let mut config = MasamuneScreenConfig::default();
        config.ios_orientations(&[IosOrientation::Portrait]);
        let ProjectIr::Ios(ios) = contribute(ProjectIr::Ios(ios), &config) else {
            panic!("expected iOS")
        };
        let plist = &ios.apple.targets["App"].info_plist;
        assert_eq!(
            plist["UISupportedInterfaceOrientations"],
            PropertyListValue::Array(vec![PropertyListValue::String(
                "UIInterfaceOrientationPortrait".into()
            )])
        );
        assert!(!plist.contains_key("UISupportedInterfaceOrientations~ipad"));
    }

    #[test]
    fn rejects_invalid_orientations() {
        let mut config = MasamuneScreenConfig::default();
        config.screen_orientation("portrait\" />");
        assert!(MasamuneScreen.validate(&config).is_err());
    }
}
