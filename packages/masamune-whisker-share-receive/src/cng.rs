//! CNG plugin that lets other apps share text, URLs and images with this app.
//!
//! ```ignore
//! use masamune_whisker_share_receive::cng::MasamuneShareReceive;
//!
//! app.project_plugin::<MasamuneShareReceive>(|c| {
//!     c.app_group("group.net.mathru.samurai")
//!         .extension_suffix("ShareExtension")
//!         .display_name("SamuraiAI");
//! });
//! ```
//!
//! The plugin also runs with the defaults when the crate is a dependency and whisker.rs does not
//! mention it.
//!
//! - Android: adds `ACTION_SEND` (`text/plain`, `image/*`) and `ACTION_SEND_MULTIPLE`
//!   (`image/*`) intent filters to `MainActivity` and sets `android:launchMode="singleTask"`
//!   (kept when it is already `singleTask`, `singleInstance` or `singleInstancePerTask`), so a
//!   share reaches the running app through `onNewIntent`.
//! - iOS: adds a Share Extension target (`<app bundle id>.<suffix>`, embedded in `PlugIns`)
//!   whose `ShareViewController` saves the shared content to the App Group and closes. The App
//!   Group entitlement is added to both the app and the extension, and the group id is written to
//!   both `Info.plist`s under [`APP_GROUP_INFO_KEY`]. The extension inherits the project's
//!   deployment target, copies the app's version, device family and signing settings, and accepts
//!   text, one web URL and up to [`DEFAULT_MAX_IMAGES`] images. Enable the App Group for both App
//!   IDs (automatic signing does this).

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use whisker_plugin::project::{
    AndroidManifestAttributeEdit, AndroidManifestEdit, AndroidManifestSelector, AndroidModuleKind,
    AppleBuildPath, AppleBuildSetting, AppleEmbed, AppleEmbedSource, AppleSource, AppleTarget,
    IosProjectIr, ProjectContext, ProjectFile, ProjectIr, ProjectPath, ProjectPlugin,
    ProjectUpdate, PropertyListValue, XmlElement, XmlNode,
};
use whisker_plugin::{FileEntry, PluginConfig};

/// `Info.plist` key (app and extension) holding the App Group id.
pub const APP_GROUP_INFO_KEY: &str = "MasamuneShareReceiveAppGroup";
/// The App Group entitlement.
pub const APP_GROUPS_ENTITLEMENT: &str = "com.apple.security.application-groups";
/// Default bundle id suffix of the Share Extension.
pub const DEFAULT_EXTENSION_SUFFIX: &str = "ShareExtension";
/// Default number of images the Share Extension accepts.
pub const DEFAULT_MAX_IMAGES: u32 = 10;
/// Target id of the Share Extension in the iOS project.
pub const EXTENSION_TARGET: &str = "masamune_share_receive";

const EXTENSION_PRODUCT: &str = "ShareExtension";
const EXTENSION_SOURCE: &str = "MasamuneShareReceive/ShareViewController.swift";
const SHARE_VIEW_CONTROLLER: &str = include_str!("../ios/ShareExtension/ShareViewController.swift");
const MAIN_ACTIVITY: &str = ".MainActivity";
const ANDROID_NS: &str = "http://schemas.android.com/apk/res/android";
/// Launch modes that deliver a share to the running activity through `onNewIntent`.
const REUSING_LAUNCH_MODES: [&str; 3] = ["singleTask", "singleInstance", "singleInstancePerTask"];
/// App settings the extension copies so it is signed and shown like the app.
const COPIED_SETTINGS: [&str; 5] = [
    "CODE_SIGN_IDENTITY",
    "CODE_SIGN_STYLE",
    "DEVELOPMENT_TEAM",
    "TARGETED_DEVICE_FAMILY",
    "IPHONEOS_DEPLOYMENT_TARGET",
];

/// Settings of [`MasamuneShareReceive`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MasamuneShareReceiveConfig {
    /// Bundle id suffix of the Share Extension ([`DEFAULT_EXTENSION_SUFFIX`] when unset).
    pub extension_suffix: Option<String>,
    /// App Group shared by the app and the extension (`group.<app bundle id>` when unset).
    pub app_group: Option<String>,
    /// Name shown in the share sheet (the app's `CFBundleDisplayName` when unset).
    pub display_name: Option<String>,
    /// Most images accepted in one share on iOS ([`DEFAULT_MAX_IMAGES`] when unset).
    pub max_images: Option<u32>,
}

impl MasamuneShareReceiveConfig {
    /// Bundle id suffix of the Share Extension: `<app bundle id>.<suffix>`.
    pub fn extension_suffix(&mut self, suffix: impl Into<String>) -> &mut Self {
        self.extension_suffix = Some(suffix.into());
        self
    }

    /// App Group shared by the app and the extension, for example `group.net.mathru.samurai`.
    pub fn app_group(&mut self, group: impl Into<String>) -> &mut Self {
        self.app_group = Some(group.into());
        self
    }

    /// Name of the app in the iOS share sheet.
    pub fn display_name(&mut self, name: impl Into<String>) -> &mut Self {
        self.display_name = Some(name.into());
        self
    }

    /// Most images accepted in one share on iOS.
    pub fn max_images(&mut self, count: u32) -> &mut Self {
        self.max_images = Some(count);
        self
    }

    fn suffix(&self) -> &str {
        self.extension_suffix
            .as_deref()
            .unwrap_or(DEFAULT_EXTENSION_SUFFIX)
    }
}

impl PluginConfig for MasamuneShareReceiveConfig {
    const NAME: &'static str = "masamune-whisker-share-receive";
}

/// The plugin. Register it with `app.project_plugin::<MasamuneShareReceive>(|c| …)`.
pub struct MasamuneShareReceive;

impl ProjectPlugin for MasamuneShareReceive {
    type Config = MasamuneShareReceiveConfig;

    fn validate(&self, config: &Self::Config) -> Result<()> {
        let suffix = config.suffix();
        ensure!(
            !suffix.is_empty()
                && !suffix.starts_with('.')
                && !suffix.ends_with('.')
                && suffix
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.'),
            "masamune-whisker-share-receive: invalid extension suffix {suffix:?}"
        );
        if let Some(group) = &config.app_group {
            ensure!(
                group.starts_with("group.") && group.len() > "group.".len(),
                "masamune-whisker-share-receive: the App Group must start with `group.` ({group:?})"
            );
        }
        ensure!(
            config.max_images != Some(0),
            "masamune-whisker-share-receive: max_images must be at least 1"
        );
        Ok(())
    }

    fn contribute(&self, context: &ProjectContext, config: &Self::Config) -> Result<ProjectUpdate> {
        match &context.project {
            ProjectIr::Ios(ios) => Ok(ProjectUpdate::Replace {
                project: Box::new(ProjectIr::Ios(ios_project(ios, config)?)),
                reason: "Add the Share Extension and its App Group".into(),
            }),
            ProjectIr::Android(android) => {
                let mut android = android.clone();
                let id = android.application.clone();
                let module = android.modules.get_mut(&id).context(
                    "masamune-whisker-share-receive: Android application module is missing",
                )?;
                let AndroidModuleKind::Application(app) = &mut module.kind else {
                    bail!("masamune-whisker-share-receive: {id} is not an application module");
                };
                let main =
                    app.android.source_sets.get_mut("main").context(
                        "masamune-whisker-share-receive: the main source set is missing",
                    )?;
                let manifest = main
                    .manifest
                    .as_ref()
                    .context("masamune-whisker-share-receive: AndroidManifest.xml is missing")?;
                let edits = android_edits(manifest)?;
                main.edit_manifest(&edits)?;
                Ok(ProjectUpdate::Replace {
                    project: Box::new(ProjectIr::Android(android)),
                    reason: "Receive shares in MainActivity".into(),
                })
            }
            _ => Ok(ProjectUpdate::Keep),
        }
    }
}

// MARK: - Android

fn element(name: &str, attributes: &[(&str, &str)], children: Vec<XmlNode>) -> XmlNode {
    XmlNode::Element(XmlElement {
        name: name.into(),
        attributes: attributes
            .iter()
            .map(|(key, value)| ((*key).into(), (*value).into()))
            .collect(),
        children,
    })
}

fn share_filter(action: &str, mime_type: &str) -> XmlNode {
    element(
        "intent-filter",
        &[],
        vec![
            element("action", &[("android:name", action)], vec![]),
            element(
                "category",
                &[("android:name", "android.intent.category.DEFAULT")],
                vec![],
            ),
            element("data", &[("android:mimeType", mime_type)], vec![]),
        ],
    )
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

/// The edits for `manifest`: intent filters, and `singleTask` unless the launch mode already
/// reuses the activity.
fn android_edits(manifest: &XmlElement) -> Result<Vec<AndroidManifestEdit>> {
    ensure!(
        manifest.attributes.get("xmlns:android").map(String::as_str) == Some(ANDROID_NS),
        "masamune-whisker-share-receive: AndroidManifest.xml does not declare xmlns:android"
    );
    let activity = child(manifest, "application", None)
        .and_then(|application| child(application, "activity", Some(MAIN_ACTIVITY)))
        .context(
            "masamune-whisker-share-receive: MainActivity is missing from AndroidManifest.xml",
        )?;
    let mut attributes = BTreeMap::new();
    let launch_mode = activity
        .attributes
        .get("android:launchMode")
        .map(String::as_str);
    if !launch_mode.is_some_and(|mode| REUSING_LAUNCH_MODES.contains(&mode)) {
        attributes.insert(
            "android:launchMode".to_string(),
            AndroidManifestAttributeEdit::Override("singleTask".into()),
        );
    }
    Ok(vec![AndroidManifestEdit::Upsert {
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
        append: vec![
            share_filter("android.intent.action.SEND", "text/plain"),
            share_filter("android.intent.action.SEND", "image/*"),
            share_filter("android.intent.action.SEND_MULTIPLE", "image/*"),
        ],
    }])
}

// MARK: - iOS

fn string(value: impl Into<String>) -> PropertyListValue {
    PropertyListValue::String(value.into())
}

fn setting_string(setting: Option<&AppleBuildSetting>) -> Option<&str> {
    match setting {
        Some(AppleBuildSetting::String(value)) => Some(value),
        _ => None,
    }
}

/// Adds `group` to the App Group entitlement.
fn add_app_group(
    entitlements: &mut BTreeMap<String, PropertyListValue>,
    group: &str,
) -> Result<()> {
    let PropertyListValue::Array(groups) = entitlements
        .entry(APP_GROUPS_ENTITLEMENT.into())
        .or_insert_with(|| PropertyListValue::Array(Vec::new()))
    else {
        bail!("{APP_GROUPS_ENTITLEMENT} must be an array");
    };
    let group = string(group);
    if !groups.contains(&group) {
        groups.push(group);
    }
    Ok(())
}

fn ios_project(ios: &IosProjectIr, config: &MasamuneShareReceiveConfig) -> Result<IosProjectIr> {
    let mut ios = ios.clone();
    let app_id = ios.apple.application.clone();
    let app = ios
        .apple
        .targets
        .get(&app_id)
        .context("masamune-whisker-share-receive: main iOS application is missing")?;
    let bundle_id = setting_string(app.build_settings.get("PRODUCT_BUNDLE_IDENTIFIER"))
        .filter(|id| !id.is_empty() && !id.contains('$'))
        .context(
            "masamune-whisker-share-receive: the app needs a literal PRODUCT_BUNDLE_IDENTIFIER",
        )?
        .to_string();
    let suffix = config.suffix();
    let group = config
        .app_group
        .clone()
        .unwrap_or_else(|| format!("group.{bundle_id}"));

    // Build settings: identity, extension-safe APIs and the app's signing/device/deployment values.
    let mut build_settings: BTreeMap<String, AppleBuildSetting> = [
        ("PRODUCT_BUNDLE_IDENTIFIER", format!("{bundle_id}.{suffix}")),
        ("PRODUCT_NAME", "$(TARGET_NAME)".into()),
        ("SDKROOT", "iphoneos".into()),
        ("APPLICATION_EXTENSION_API_ONLY", "YES".into()),
        ("SKIP_INSTALL", "YES".into()),
        (
            "LD_RUNPATH_SEARCH_PATHS",
            "$(inherited) @executable_path/Frameworks @executable_path/../../Frameworks".into(),
        ),
        ("TARGETED_DEVICE_FAMILY", "1,2".into()),
    ]
    .into_iter()
    .map(|(key, value)| (key.to_string(), AppleBuildSetting::String(value)))
    .collect();
    for key in COPIED_SETTINGS {
        let value = app
            .build_settings
            .get(key)
            .or_else(|| ios.apple.build_settings.get(key));
        if let Some(value) = value {
            build_settings.insert(key.into(), value.clone());
        }
    }
    // Per-configuration overrides of the app (flavored bundle ids, signing teams).
    let mut configurations = BTreeMap::new();
    for (name, layer) in &app.configurations {
        let mut extension_layer = whisker_plugin::project::AppleBuildConfiguration::default();
        for (key, value) in &layer.settings {
            if key == "PRODUCT_BUNDLE_IDENTIFIER" {
                if let AppleBuildSetting::String(id) = value {
                    extension_layer.settings.insert(
                        key.clone(),
                        AppleBuildSetting::String(format!("{id}.{suffix}")),
                    );
                }
            } else if COPIED_SETTINGS.contains(&key.as_str()) {
                extension_layer.settings.insert(key.clone(), value.clone());
            }
        }
        if !extension_layer.settings.is_empty() {
            configurations.insert(name.clone(), extension_layer);
        }
    }

    let display_name = config
        .display_name
        .clone()
        .or_else(|| match app.info_plist.get("CFBundleDisplayName") {
            Some(PropertyListValue::String(name)) => Some(name.clone()),
            _ => None,
        })
        .unwrap_or_else(|| "$(PRODUCT_NAME)".into());
    let mut info_plist: BTreeMap<String, PropertyListValue> = [
        ("CFBundleDevelopmentRegion", "$(DEVELOPMENT_LANGUAGE)"),
        ("CFBundleDisplayName", display_name.as_str()),
        ("CFBundleExecutable", "$(EXECUTABLE_NAME)"),
        ("CFBundleIdentifier", "$(PRODUCT_BUNDLE_IDENTIFIER)"),
        ("CFBundleInfoDictionaryVersion", "6.0"),
        ("CFBundleName", "$(PRODUCT_NAME)"),
        ("CFBundlePackageType", "XPC!"),
        ("CFBundleShortVersionString", "1.0"),
        ("CFBundleVersion", "1"),
        (APP_GROUP_INFO_KEY, group.as_str()),
    ]
    .into_iter()
    .map(|(key, value)| (key.to_string(), string(value)))
    .collect();
    // App Store Connect requires the extension's version to match the app's.
    for key in ["CFBundleShortVersionString", "CFBundleVersion"] {
        if let Some(value) = app.info_plist.get(key) {
            info_plist.insert(key.into(), value.clone());
        }
    }
    let max_images = i64::from(config.max_images.unwrap_or(DEFAULT_MAX_IMAGES));
    info_plist.insert(
        "NSExtension".into(),
        PropertyListValue::Dict(
            [
                (
                    "NSExtensionPointIdentifier".to_string(),
                    string("com.apple.share-services"),
                ),
                (
                    "NSExtensionPrincipalClass".to_string(),
                    string("$(PRODUCT_MODULE_NAME).ShareViewController"),
                ),
                (
                    "NSExtensionAttributes".to_string(),
                    PropertyListValue::Dict(
                        [(
                            "NSExtensionActivationRule".to_string(),
                            PropertyListValue::Dict(
                                [
                                    (
                                        "NSExtensionActivationSupportsText".to_string(),
                                        PropertyListValue::Boolean(true),
                                    ),
                                    (
                                        "NSExtensionActivationSupportsWebURLWithMaxCount"
                                            .to_string(),
                                        PropertyListValue::Integer(1),
                                    ),
                                    (
                                        "NSExtensionActivationSupportsImageWithMaxCount"
                                            .to_string(),
                                        PropertyListValue::Integer(max_images),
                                    ),
                                ]
                                .into(),
                            ),
                        )]
                        .into(),
                    ),
                ),
            ]
            .into(),
        ),
    );
    let mut entitlements = BTreeMap::new();
    add_app_group(&mut entitlements, &group)?;

    let source = ProjectPath::new(EXTENSION_SOURCE)?;
    let extension = AppleTarget {
        product_name: EXTENSION_PRODUCT.into(),
        product_type: Some("com.apple.product-type.app-extension".into()),
        sources: vec![AppleSource {
            path: AppleBuildPath::Project(source.clone()),
            platform_filters: vec![],
            compiler_flags: vec![],
        }],
        info_plist,
        entitlements,
        build_settings,
        configurations,
        ..Default::default()
    };

    let app = ios
        .apple
        .targets
        .get_mut(&app_id)
        .context("masamune-whisker-share-receive: main iOS application is missing")?;
    add_app_group(&mut app.entitlements, &group)?;
    app.info_plist
        .insert(APP_GROUP_INFO_KEY.into(), string(group.clone()));
    let embedded = app.embeds.iter().any(|embed| {
        matches!(&embed.source, AppleEmbedSource::Target { target } if target == EXTENSION_TARGET)
    });
    if !embedded {
        app.embeds.push(AppleEmbed {
            source: AppleEmbedSource::Target {
                target: EXTENSION_TARGET.into(),
            },
            platform_filters: vec![],
            destination: ProjectPath::new("PlugIns")?,
            code_sign_on_copy: false,
            remove_headers_on_copy: true,
        });
    }
    ios.apple.targets.insert(EXTENSION_TARGET.into(), extension);
    ios.apple.files.insert(
        source,
        ProjectFile::Generated {
            entry: FileEntry::text(SHARE_VIEW_CONTROLLER),
        },
    );
    Ok(ios)
}

#[cfg(test)]
mod tests {
    use super::*;
    use whisker_plugin::project::{
        AndroidApplication, AndroidBuild, AndroidModule, AndroidProjectIr, AndroidSourceSet,
        AppleBuildConfiguration,
    };

    fn setting(value: &str) -> AppleBuildSetting {
        AppleBuildSetting::String(value.into())
    }

    fn ios() -> IosProjectIr {
        let mut ios = IosProjectIr::default();
        ios.apple.application = "app".into();
        ios.apple
            .build_settings
            .insert("IPHONEOS_DEPLOYMENT_TARGET".into(), setting("15.0"));
        let mut app = AppleTarget {
            product_name: "Samurai".into(),
            product_type: Some("com.apple.product-type.application".into()),
            ..Default::default()
        };
        app.build_settings.insert(
            "PRODUCT_BUNDLE_IDENTIFIER".into(),
            setting("net.mathru.samurai"),
        );
        app.build_settings
            .insert("DEVELOPMENT_TEAM".into(), setting("TEAM123"));
        app.info_plist
            .insert("CFBundleDisplayName".into(), string("SamuraiAI"));
        app.info_plist
            .insert("CFBundleShortVersionString".into(), string("2.1.0"));
        app.info_plist
            .insert("CFBundleVersion".into(), string("42"));
        app.configurations.insert(
            "Release".into(),
            AppleBuildConfiguration {
                settings: [(
                    "PRODUCT_BUNDLE_IDENTIFIER".to_string(),
                    setting("net.mathru.samurai.prod"),
                )]
                .into(),
                ..Default::default()
            },
        );
        ios.apple.targets.insert("app".into(), app);
        ios
    }

    fn contribute(project: ProjectIr, config: &MasamuneShareReceiveConfig) -> ProjectIr {
        MasamuneShareReceive.validate(config).unwrap();
        let context = ProjectContext {
            project,
            app_crate_dir: None,
        };
        match MasamuneShareReceive.contribute(&context, config).unwrap() {
            ProjectUpdate::Replace { project, .. } => *project,
            other => panic!("expected a replacement, got {other:?}"),
        }
    }

    fn contribute_ios(config: &MasamuneShareReceiveConfig) -> IosProjectIr {
        let ProjectIr::Ios(ios) = contribute(ProjectIr::Ios(ios()), config) else {
            panic!("expected iOS")
        };
        ios
    }

    #[test]
    fn adds_the_share_extension_with_defaults() {
        let ios = contribute_ios(&Default::default());
        let extension = &ios.apple.targets[EXTENSION_TARGET];
        assert_eq!(
            extension.product_type.as_deref(),
            Some("com.apple.product-type.app-extension")
        );
        assert_eq!(
            extension.build_settings["PRODUCT_BUNDLE_IDENTIFIER"],
            setting("net.mathru.samurai.ShareExtension")
        );
        assert_eq!(
            extension.configurations["Release"].settings["PRODUCT_BUNDLE_IDENTIFIER"],
            setting("net.mathru.samurai.prod.ShareExtension")
        );
        assert_eq!(
            extension.build_settings["IPHONEOS_DEPLOYMENT_TARGET"],
            setting("15.0")
        );
        assert_eq!(
            extension.build_settings["DEVELOPMENT_TEAM"],
            setting("TEAM123")
        );
        assert_eq!(extension.info_plist["CFBundleVersion"], string("42"));
        assert_eq!(
            extension.info_plist["CFBundleDisplayName"],
            string("SamuraiAI")
        );
        let PropertyListValue::Dict(ns) = &extension.info_plist["NSExtension"] else {
            panic!("NSExtension must be a dictionary")
        };
        assert_eq!(
            ns["NSExtensionPointIdentifier"],
            string("com.apple.share-services")
        );
        let group = PropertyListValue::Array(vec![string("group.net.mathru.samurai")]);
        assert_eq!(extension.entitlements[APP_GROUPS_ENTITLEMENT], group);

        let app = &ios.apple.targets["app"];
        assert_eq!(app.entitlements[APP_GROUPS_ENTITLEMENT], group);
        assert_eq!(
            app.info_plist[APP_GROUP_INFO_KEY],
            string("group.net.mathru.samurai")
        );
        assert_eq!(app.embeds.len(), 1);
        assert_eq!(app.embeds[0].destination.as_str(), "PlugIns");
        assert!(matches!(
            &ios.apple.files[&ProjectPath::new(EXTENSION_SOURCE).unwrap()],
            ProjectFile::Generated { .. }
        ));
    }

    #[test]
    fn uses_the_configured_group_suffix_and_limits() {
        let mut config = MasamuneShareReceiveConfig::default();
        config
            .app_group("group.net.mathru.shared")
            .extension_suffix("Share")
            .display_name("Samurai")
            .max_images(3);
        let mut ios = ios();
        ios.apple
            .targets
            .get_mut("app")
            .unwrap()
            .entitlements
            .insert(
                APP_GROUPS_ENTITLEMENT.into(),
                PropertyListValue::Array(vec![string("group.other")]),
            );
        let ProjectIr::Ios(ios) = contribute(ProjectIr::Ios(ios), &config) else {
            panic!("expected iOS")
        };
        let extension = &ios.apple.targets[EXTENSION_TARGET];
        assert_eq!(
            extension.build_settings["PRODUCT_BUNDLE_IDENTIFIER"],
            setting("net.mathru.samurai.Share")
        );
        assert_eq!(
            extension.info_plist["CFBundleDisplayName"],
            string("Samurai")
        );
        assert_eq!(
            ios.apple.targets["app"].entitlements[APP_GROUPS_ENTITLEMENT],
            PropertyListValue::Array(vec![
                string("group.other"),
                string("group.net.mathru.shared")
            ])
        );
        let PropertyListValue::Dict(ns) = &extension.info_plist["NSExtension"] else {
            panic!()
        };
        let PropertyListValue::Dict(attributes) = &ns["NSExtensionAttributes"] else {
            panic!()
        };
        let PropertyListValue::Dict(rule) = &attributes["NSExtensionActivationRule"] else {
            panic!()
        };
        assert_eq!(
            rule["NSExtensionActivationSupportsImageWithMaxCount"],
            PropertyListValue::Integer(3)
        );
    }

    #[test]
    fn rejects_invalid_settings() {
        let mut config = MasamuneShareReceiveConfig::default();
        config.app_group("net.mathru");
        assert!(MasamuneShareReceive.validate(&config).is_err());
        let mut config = MasamuneShareReceiveConfig::default();
        config.extension_suffix("Share Extension");
        assert!(MasamuneShareReceive.validate(&config).is_err());
        let mut config = MasamuneShareReceiveConfig::default();
        config.max_images(0);
        assert!(MasamuneShareReceive.validate(&config).is_err());
    }

    fn android(launch_mode: Option<&str>) -> ProjectIr {
        let mut activity = XmlElement::new("activity");
        activity
            .attributes
            .insert("android:name".into(), MAIN_ACTIVITY.into());
        if let Some(mode) = launch_mode {
            activity
                .attributes
                .insert("android:launchMode".into(), mode.into());
        }
        let mut application = XmlElement::new("application");
        application.children.push(XmlNode::Element(activity));
        let mut manifest = XmlElement::new("manifest");
        manifest
            .attributes
            .insert("xmlns:android".into(), ANDROID_NS.into());
        manifest.children.push(XmlNode::Element(application));
        let build = AndroidBuild {
            namespace: "net.mathru.samurai".into(),
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
                application_id: Some("net.mathru.samurai".into()),
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
            panic!()
        };
        let manifest = app.android.source_sets["main"].manifest.as_ref().unwrap();
        child(
            child(manifest, "application", None).unwrap(),
            "activity",
            Some(MAIN_ACTIVITY),
        )
        .unwrap()
        .clone()
    }

    #[test]
    fn adds_intent_filters_and_single_task_on_android() {
        let config = MasamuneShareReceiveConfig::default();
        let project = contribute(android(None), &config);
        let activity = activity(&project);
        assert_eq!(activity.attributes["android:launchMode"], "singleTask");
        let filters = activity
            .children
            .iter()
            .filter(|node| matches!(node, XmlNode::Element(e) if e.name == "intent-filter"))
            .count();
        assert_eq!(filters, 3);

        // Running again on the result changes nothing.
        let again = contribute(project.clone(), &config);
        assert_eq!(again, project);

        let standard = activity_of(contribute(android(Some("standard")), &config));
        assert_eq!(standard.attributes["android:launchMode"], "singleTask");
        let single_instance = activity_of(contribute(android(Some("singleInstance")), &config));
        assert_eq!(
            single_instance.attributes["android:launchMode"],
            "singleInstance"
        );
    }

    fn activity_of(project: ProjectIr) -> XmlElement {
        activity(&project)
    }

    #[test]
    fn other_platforms_are_kept() {
        let context = ProjectContext {
            project: ProjectIr::Macos(Default::default()),
            app_crate_dir: None,
        };
        assert!(matches!(
            MasamuneShareReceive
                .contribute(&context, &Default::default())
                .unwrap(),
            ProjectUpdate::Keep
        ));
    }
}
