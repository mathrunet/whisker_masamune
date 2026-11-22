//! CNG plugin that registers the app's URL schemes, Universal Links and App Links.
//!
//! ```ignore
//! use masamune_whisker_deeplink::cng::MasamuneDeeplink;
//!
//! app.project_plugin::<MasamuneDeeplink>(|c| {
//!     c.scheme("segue")
//!         .app_link("segue.day", Some("/app/"));
//! });
//! ```
//!
//! Restrict a scheme to some hosts with [`MasamuneDeeplinkConfig::scheme_hosts`] (Android only) and
//! limit an app link to one platform with [`MasamuneDeeplinkConfig::android_app_link`] /
//! [`MasamuneDeeplinkConfig::ios_app_link`].
//!
//! - iOS: each scheme is added to `CFBundleURLTypes`, each app link host to the
//!   `com.apple.developer.associated-domains` entitlement as `applinks:<host>`. The domain has to
//!   serve `/.well-known/apple-app-site-association` and the Associated Domains capability has to be
//!   enabled for the App ID (automatic signing does this).
//! - Android: `MainActivity` gets a `VIEW` / `BROWSABLE` intent filter per scheme (with one
//!   `<data android:scheme android:host>` per host when the scheme is restricted to hosts) and an
//!   `android:autoVerify="true"` filter per app link (`https`, host, optional path prefix), and
//!   `android:launchMode="singleTask"` (kept when it already reuses the activity) so links reach the
//!   running app through `onNewIntent`. The domain has to serve `/.well-known/assetlinks.json`.
//!
//! With the defaults (no scheme, no app link) the plugin changes nothing.

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use whisker_plugin::PluginConfig;
use whisker_plugin::project::{
    AndroidManifestAttributeEdit, AndroidManifestEdit, AndroidManifestSelector, AndroidModuleKind,
    ProjectContext, ProjectIr, ProjectPlugin, ProjectUpdate, PropertyListValue, XmlElement,
    XmlNode,
};

/// The Associated Domains entitlement.
pub const ASSOCIATED_DOMAINS_ENTITLEMENT: &str = "com.apple.developer.associated-domains";

const MAIN_ACTIVITY: &str = ".MainActivity";
const ANDROID_NS: &str = "http://schemas.android.com/apk/res/android";
const REUSING_LAUNCH_MODES: [&str; 3] = ["singleTask", "singleInstance", "singleInstancePerTask"];

/// A Universal Link / App Link domain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AppLink {
    /// Host name such as `segue.day`.
    pub host: String,
    /// Path prefix Android opens in the app (for example `/app/`). iOS reads the paths from the
    /// domain's `apple-app-site-association` file.
    pub path_prefix: Option<String>,
    /// Add `applinks:<host>` to the iOS associated domains (default `true`).
    pub ios: bool,
    /// Add the `android:autoVerify` intent filter on Android (default `true`).
    pub android: bool,
}

impl Default for AppLink {
    fn default() -> Self {
        Self {
            host: String::new(),
            path_prefix: None,
            ios: true,
            android: true,
        }
    }
}

/// Settings of [`MasamuneDeeplink`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MasamuneDeeplinkConfig {
    /// Custom URL schemes (lowercase), such as `segue`.
    pub schemes: Vec<String>,
    /// Hosts each scheme is restricted to on Android (`segue` → `["track", "playlist"]` opens only
    /// `segue://track…` and `segue://playlist…`). Schemes without an entry accept any host. iOS
    /// cannot restrict a URL scheme to hosts, so this has no effect there.
    pub scheme_hosts: BTreeMap<String, Vec<String>>,
    /// Universal Link / App Link domains.
    pub app_links: Vec<AppLink>,
}

impl MasamuneDeeplinkConfig {
    /// Opens `<scheme>://…` links in the app.
    pub fn scheme(&mut self, scheme: impl Into<String>) -> &mut Self {
        self.schemes.push(scheme.into());
        self
    }

    /// Opens `<scheme>://<host>…` links in the app only for the given hosts on Android (one
    /// `<data android:scheme android:host>` per host). On iOS the scheme opens any host. Adds the
    /// scheme when it is not registered yet.
    pub fn scheme_hosts<I, S>(&mut self, scheme: impl Into<String>, hosts: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let scheme = scheme.into();
        if !self.schemes.contains(&scheme) {
            self.schemes.push(scheme.clone());
        }
        self.scheme_hosts
            .entry(scheme)
            .or_default()
            .extend(hosts.into_iter().map(Into::into));
        self
    }

    /// Opens `https://<host><path_prefix>…` links in the app on iOS and Android.
    pub fn app_link(&mut self, host: impl Into<String>, path_prefix: Option<&str>) -> &mut Self {
        self.app_links.push(AppLink {
            host: host.into(),
            path_prefix: path_prefix.map(String::from),
            ..Default::default()
        });
        self
    }

    /// Opens `https://<host><path_prefix>…` links in the app on Android only (no iOS associated
    /// domain).
    pub fn android_app_link(
        &mut self,
        host: impl Into<String>,
        path_prefix: Option<&str>,
    ) -> &mut Self {
        self.app_links.push(AppLink {
            host: host.into(),
            path_prefix: path_prefix.map(String::from),
            ios: false,
            android: true,
        });
        self
    }

    /// Opens `https://<host>…` Universal Links in the app on iOS only (no Android intent filter).
    pub fn ios_app_link(&mut self, host: impl Into<String>) -> &mut Self {
        self.app_links.push(AppLink {
            host: host.into(),
            path_prefix: None,
            ios: true,
            android: false,
        });
        self
    }

    fn ios_app_links(&self) -> impl Iterator<Item = &AppLink> {
        self.app_links.iter().filter(|link| link.ios)
    }

    fn android_app_links(&self) -> impl Iterator<Item = &AppLink> {
        self.app_links.iter().filter(|link| link.android)
    }
}

impl PluginConfig for MasamuneDeeplinkConfig {
    const NAME: &'static str = "masamune-whisker-deeplink";
}

/// The plugin. Register it with `app.project_plugin::<MasamuneDeeplink>(|c| …)`.
pub struct MasamuneDeeplink;

impl ProjectPlugin for MasamuneDeeplink {
    type Config = MasamuneDeeplinkConfig;

    fn validate(&self, config: &Self::Config) -> Result<()> {
        for scheme in &config.schemes {
            ensure!(
                scheme.starts_with(|c: char| c.is_ascii_lowercase())
                    && scheme
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "+-.".contains(c)),
                "masamune-whisker-deeplink: scheme `{scheme}` must be a lowercase URL scheme"
            );
            ensure!(
                scheme != "http" && scheme != "https",
                "masamune-whisker-deeplink: use app_link for http(s) links"
            );
        }
        for (scheme, hosts) in &config.scheme_hosts {
            ensure!(
                config.schemes.contains(scheme),
                "masamune-whisker-deeplink: scheme_hosts names `{scheme}`, which is not in schemes"
            );
            ensure!(
                !hosts.is_empty(),
                "masamune-whisker-deeplink: scheme `{scheme}` is restricted to no host"
            );
            for host in hosts {
                ensure!(
                    !host.is_empty()
                        && host
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || "-._*".contains(c)),
                    "masamune-whisker-deeplink: host `{host}` of scheme `{scheme}` must be a host name"
                );
            }
        }
        for link in &config.app_links {
            ensure!(
                !link.host.is_empty()
                    && link
                        .host
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || "-.*".contains(c)),
                "masamune-whisker-deeplink: app link host `{}` must be a host name",
                link.host
            );
            ensure!(
                link.path_prefix
                    .as_deref()
                    .is_none_or(|prefix| prefix.starts_with('/') && !prefix.contains('"')),
                "masamune-whisker-deeplink: path prefix must start with `/`"
            );
            ensure!(
                link.ios || link.android,
                "masamune-whisker-deeplink: app link `{}` targets neither iOS nor Android",
                link.host
            );
        }
        Ok(())
    }

    fn contribute(&self, context: &ProjectContext, config: &Self::Config) -> Result<ProjectUpdate> {
        self.validate(config)?;
        if config.schemes.is_empty() && config.app_links.is_empty() {
            return Ok(ProjectUpdate::Keep);
        }
        match &context.project {
            ProjectIr::Ios(_)
                if config.schemes.is_empty() && config.ios_app_links().next().is_none() =>
            {
                Ok(ProjectUpdate::Keep)
            }
            ProjectIr::Android(_)
                if config.schemes.is_empty() && config.android_app_links().next().is_none() =>
            {
                Ok(ProjectUpdate::Keep)
            }
            ProjectIr::Ios(ios) => {
                let mut ios = ios.clone();
                let target = ios
                    .apple
                    .targets
                    .get_mut(&ios.apple.application)
                    .context("masamune-whisker-deeplink: main iOS application is missing")?;
                for scheme in &config.schemes {
                    add_url_scheme(&mut target.info_plist, scheme)?;
                }
                for link in config.ios_app_links() {
                    add_array_item(
                        &mut target.entitlements,
                        ASSOCIATED_DOMAINS_ENTITLEMENT,
                        &format!("applinks:{}", link.host),
                    )?;
                }
                Ok(ProjectUpdate::Replace {
                    project: Box::new(ProjectIr::Ios(ios)),
                    reason: "Register the deep link schemes and associated domains".into(),
                })
            }
            ProjectIr::Android(android) => {
                let mut android = android.clone();
                let id = android.application.clone();
                let module = android
                    .modules
                    .get_mut(&id)
                    .context("masamune-whisker-deeplink: Android application module is missing")?;
                let AndroidModuleKind::Application(app) = &mut module.kind else {
                    bail!("masamune-whisker-deeplink: {id} is not an application module");
                };
                let main = app
                    .android
                    .source_sets
                    .get_mut("main")
                    .context("masamune-whisker-deeplink: the main source set is missing")?;
                let manifest = main
                    .manifest
                    .as_ref()
                    .context("masamune-whisker-deeplink: AndroidManifest.xml is missing")?;
                let edits = android_edits(manifest, config)?;
                main.edit_manifest(&edits)?;
                Ok(ProjectUpdate::Replace {
                    project: Box::new(ProjectIr::Android(android)),
                    reason: "Open deep links in MainActivity".into(),
                })
            }
            _ => Ok(ProjectUpdate::Keep),
        }
    }
}

// MARK: - iOS

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

/// Adds `scheme` to `CFBundleURLTypes` unless an entry already declares it.
fn add_url_scheme(
    info_plist: &mut BTreeMap<String, PropertyListValue>,
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
                    PropertyListValue::String("Editor".into()),
                ),
                (
                    "CFBundleURLName".to_string(),
                    PropertyListValue::String(scheme.into()),
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

fn view_filter(auto_verify: bool, data: Vec<XmlNode>) -> XmlNode {
    let mut children = vec![
        element(
            "action",
            &[("android:name", "android.intent.action.VIEW")],
            vec![],
        ),
        element(
            "category",
            &[("android:name", "android.intent.category.DEFAULT")],
            vec![],
        ),
        element(
            "category",
            &[("android:name", "android.intent.category.BROWSABLE")],
            vec![],
        ),
    ];
    children.extend(data);
    element(
        "intent-filter",
        if auto_verify {
            &[("android:autoVerify", "true")]
        } else {
            &[]
        },
        children,
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

fn android_edits(
    manifest: &XmlElement,
    config: &MasamuneDeeplinkConfig,
) -> Result<Vec<AndroidManifestEdit>> {
    ensure!(
        manifest.attributes.get("xmlns:android").map(String::as_str) == Some(ANDROID_NS),
        "masamune-whisker-deeplink: AndroidManifest.xml does not declare xmlns:android"
    );
    let activity = child(manifest, "application", None)
        .and_then(|application| child(application, "activity", Some(MAIN_ACTIVITY)))
        .context("masamune-whisker-deeplink: MainActivity is missing from AndroidManifest.xml")?;
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
    let mut append = Vec::new();
    for scheme in &config.schemes {
        let data = match config.scheme_hosts.get(scheme) {
            Some(hosts) if !hosts.is_empty() => hosts
                .iter()
                .map(|host| {
                    element(
                        "data",
                        &[("android:scheme", scheme), ("android:host", host)],
                        vec![],
                    )
                })
                .collect(),
            _ => vec![element("data", &[("android:scheme", scheme)], vec![])],
        };
        append.push(view_filter(false, data));
    }
    for link in config.android_app_links() {
        let mut data = vec![
            ("android:scheme", "https"),
            ("android:host", link.host.as_str()),
        ];
        if let Some(prefix) = &link.path_prefix {
            data.push(("android:pathPrefix", prefix));
        }
        append.push(view_filter(true, vec![element("data", &data, vec![])]));
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
        append,
    }])
}

#[cfg(test)]
mod tests {
    use super::*;
    use whisker_plugin::project::{
        AndroidApplication, AndroidBuild, AndroidModule, AndroidProjectIr, AndroidSourceSet,
        AppleTarget, IosProjectIr, ProjectPath,
    };

    fn config() -> MasamuneDeeplinkConfig {
        let mut config = MasamuneDeeplinkConfig::default();
        config.scheme("segue").app_link("segue.day", Some("/app/"));
        config
    }

    fn replaced(project: ProjectIr, config: &MasamuneDeeplinkConfig) -> ProjectIr {
        let context = ProjectContext {
            project,
            app_crate_dir: None,
        };
        match MasamuneDeeplink.contribute(&context, config).unwrap() {
            ProjectUpdate::Replace { project, .. } => *project,
            _ => panic!("expected a replacement"),
        }
    }

    fn ios() -> ProjectIr {
        let mut ios = IosProjectIr::default();
        ios.apple.application = "App".into();
        ios.apple
            .targets
            .insert("App".into(), AppleTarget::default());
        ProjectIr::Ios(ios)
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
            namespace: "net.mathru.segue".into(),
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
                application_id: Some("net.mathru.segue".into()),
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
            panic!("expected the application")
        };
        let manifest = app.android.source_sets["main"].manifest.as_ref().unwrap();
        child(
            child(manifest, "application", None).unwrap(),
            "activity",
            None,
        )
        .unwrap()
        .clone()
    }

    #[test]
    fn registers_schemes_and_domains_on_ios() {
        let ProjectIr::Ios(ios) = replaced(ios(), &config()) else {
            panic!("expected iOS")
        };
        let target = &ios.apple.targets["App"];
        assert_eq!(
            target.entitlements[ASSOCIATED_DOMAINS_ENTITLEMENT],
            PropertyListValue::Array(vec![PropertyListValue::String("applinks:segue.day".into())])
        );
        let PropertyListValue::Array(types) = &target.info_plist["CFBundleURLTypes"] else {
            panic!("expected an array")
        };
        assert_eq!(types.len(), 1);
        // Idempotent.
        let ProjectIr::Ios(again) = replaced(ProjectIr::Ios(ios.clone()), &config()) else {
            panic!("expected iOS")
        };
        assert_eq!(
            again.apple.targets["App"].info_plist["CFBundleURLTypes"],
            target.info_plist["CFBundleURLTypes"]
        );
    }

    #[test]
    fn adds_intent_filters_on_android() {
        let activity = activity(&replaced(android(None), &config()));
        assert_eq!(activity.attributes["android:launchMode"], "singleTask");
        let filters: Vec<&XmlElement> = activity
            .children
            .iter()
            .filter_map(|node| match node {
                XmlNode::Element(element) if element.name == "intent-filter" => Some(element),
                _ => None,
            })
            .collect();
        assert_eq!(filters.len(), 2);
        assert!(!filters[0].attributes.contains_key("android:autoVerify"));
        assert_eq!(filters[1].attributes["android:autoVerify"], "true");
        let data = child(filters[1], "data", None).unwrap();
        assert_eq!(data.attributes["android:host"], "segue.day");
        assert_eq!(data.attributes["android:pathPrefix"], "/app/");
        let kept = activity_of_mode("singleInstance");
        assert_eq!(kept.attributes["android:launchMode"], "singleInstance");
    }

    fn activity_of_mode(mode: &str) -> XmlElement {
        activity(&replaced(android(Some(mode)), &config()))
    }

    fn intent_filters(activity: &XmlElement) -> Vec<&XmlElement> {
        activity
            .children
            .iter()
            .filter_map(|node| match node {
                XmlNode::Element(element) if element.name == "intent-filter" => Some(element),
                _ => None,
            })
            .collect()
    }

    fn data_elements(filter: &XmlElement) -> Vec<&XmlElement> {
        filter
            .children
            .iter()
            .filter_map(|node| match node {
                XmlNode::Element(element) if element.name == "data" => Some(element),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn restricts_schemes_to_hosts_on_android() {
        let mut config = MasamuneDeeplinkConfig::default();
        config
            .scheme_hosts("segue", ["track", "playlist"])
            .scheme("other");
        assert_eq!(config.schemes, ["segue", "other"]);
        let activity = activity(&replaced(android(None), &config));
        let filters = intent_filters(&activity);
        assert_eq!(filters.len(), 2);
        let restricted: Vec<(&str, &str)> = data_elements(filters[0])
            .iter()
            .map(|data| {
                (
                    data.attributes["android:scheme"].as_str(),
                    data.attributes["android:host"].as_str(),
                )
            })
            .collect();
        assert_eq!(restricted, [("segue", "track"), ("segue", "playlist")]);
        let open = data_elements(filters[1]);
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].attributes["android:scheme"], "other");
        assert!(!open[0].attributes.contains_key("android:host"));
        // iOS registers the scheme itself; hosts cannot be restricted there.
        let ProjectIr::Ios(ios) = replaced(ios(), &config) else {
            panic!("expected iOS")
        };
        let PropertyListValue::Array(types) =
            &ios.apple.targets["App"].info_plist["CFBundleURLTypes"]
        else {
            panic!("expected an array")
        };
        assert_eq!(types.len(), 2);
    }

    #[test]
    fn targets_app_links_per_platform() {
        let mut config = MasamuneDeeplinkConfig::default();
        config
            .android_app_link("android.segue.day", Some("/app/"))
            .ios_app_link("ios.segue.day");
        let ProjectIr::Ios(ios_project) = replaced(ios(), &config) else {
            panic!("expected iOS")
        };
        assert_eq!(
            ios_project.apple.targets["App"].entitlements[ASSOCIATED_DOMAINS_ENTITLEMENT],
            PropertyListValue::Array(vec![PropertyListValue::String(
                "applinks:ios.segue.day".into()
            )])
        );
        let activity = activity(&replaced(android(None), &config));
        let filters = intent_filters(&activity);
        assert_eq!(filters.len(), 1);
        assert_eq!(
            data_elements(filters[0])[0].attributes["android:host"],
            "android.segue.day"
        );

        // Android-only links leave the iOS project untouched, and vice versa.
        let mut android_only = MasamuneDeeplinkConfig::default();
        android_only.android_app_link("segue.day", None);
        let context = ProjectContext {
            project: ios(),
            app_crate_dir: None,
        };
        assert!(matches!(
            MasamuneDeeplink
                .contribute(&context, &android_only)
                .unwrap(),
            ProjectUpdate::Keep
        ));
        let mut ios_only = MasamuneDeeplinkConfig::default();
        ios_only.ios_app_link("segue.day");
        let context = ProjectContext {
            project: android(None),
            app_crate_dir: None,
        };
        assert!(matches!(
            MasamuneDeeplink.contribute(&context, &ios_only).unwrap(),
            ProjectUpdate::Keep
        ));
    }

    #[test]
    fn app_links_default_to_both_platforms() {
        let link: AppLink = serde_json::from_str(r#"{"host":"segue.day"}"#).unwrap();
        assert!(link.ios && link.android);
        let config: MasamuneDeeplinkConfig =
            serde_json::from_str(r#"{"schemes":["segue"],"app_links":[{"host":"segue.day"}]}"#)
                .unwrap();
        assert!(config.scheme_hosts.is_empty());
        assert!(config.app_links[0].ios && config.app_links[0].android);
    }

    #[test]
    fn validates_and_skips_empty_configs() {
        let mut bad = MasamuneDeeplinkConfig::default();
        bad.scheme("Segue");
        assert!(MasamuneDeeplink.validate(&bad).is_err());
        let mut web = MasamuneDeeplinkConfig::default();
        web.scheme("https");
        assert!(MasamuneDeeplink.validate(&web).is_err());
        let mut host = MasamuneDeeplinkConfig::default();
        host.app_link("https://segue.day", None);
        assert!(MasamuneDeeplink.validate(&host).is_err());
        let mut nowhere = MasamuneDeeplinkConfig::default();
        nowhere.app_links.push(AppLink {
            host: "segue.day".into(),
            ios: false,
            android: false,
            ..Default::default()
        });
        assert!(MasamuneDeeplink.validate(&nowhere).is_err());
        let mut unknown = MasamuneDeeplinkConfig::default();
        unknown
            .scheme_hosts
            .insert("segue".into(), vec!["track".into()]);
        assert!(MasamuneDeeplink.validate(&unknown).is_err());
        let mut bad_host = MasamuneDeeplinkConfig::default();
        bad_host.scheme_hosts("segue", ["track/x"]);
        assert!(MasamuneDeeplink.validate(&bad_host).is_err());
        let mut no_hosts = MasamuneDeeplinkConfig::default();
        no_hosts.scheme_hosts("segue", Vec::<String>::new());
        assert!(MasamuneDeeplink.validate(&no_hosts).is_err());
        let context = ProjectContext {
            project: ios(),
            app_crate_dir: None,
        };
        assert!(matches!(
            MasamuneDeeplink
                .contribute(&context, &Default::default())
                .unwrap(),
            ProjectUpdate::Keep
        ));
    }
}
