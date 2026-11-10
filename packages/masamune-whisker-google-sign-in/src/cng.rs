//! CNG plugin that registers the Google Sign-In redirect URL scheme on iOS (a no-op on Android).
//!
//! ```ignore
//! use masamune_whisker_google_sign_in::cng::MasamuneGoogleSignIn;
//!
//! app.project_plugin::<MasamuneGoogleSignIn>(|c| {
//!     c.ios_config("firebase/GoogleService-Info.plist");
//! });
//! ```
//!
//! The scheme is the `REVERSED_CLIENT_ID` of the app's `GoogleService-Info.plist` (an XML plist
//! as downloaded from the Firebase console), added to `CFBundleURLTypes`. The plist path is
//! relative to the app crate; when it is not set, the plugin uses the file the `whisker-firebase`
//! plugin copies into the project (if that plugin ran first) or `firebase/GoogleService-Info.plist`.
//! [`MasamuneGoogleSignInConfig::reversed_client_id`] skips reading the plist.

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use whisker_plugin::PluginConfig;
use whisker_plugin::project::{
    ProjectContext, ProjectFile, ProjectIr, ProjectPath, ProjectPlugin, ProjectUpdate,
    PropertyListValue,
};

/// Default app-relative location of the iOS Firebase configuration.
pub const DEFAULT_IOS_CONFIG: &str = "firebase/GoogleService-Info.plist";
/// Where `whisker-firebase` stages the plist inside the generated project.
const FIREBASE_PROJECT_FILE: &str = "firebase/GoogleService-Info.plist";

/// Settings of [`MasamuneGoogleSignIn`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MasamuneGoogleSignInConfig {
    /// App-relative path of `GoogleService-Info.plist`.
    pub ios_config: Option<String>,
    /// The URL scheme to register (for example `com.googleusercontent.apps.1234-abcd`).
    pub reversed_client_id: Option<String>,
}

impl MasamuneGoogleSignInConfig {
    pub fn ios_config(&mut self, path: impl Into<String>) -> &mut Self {
        self.ios_config = Some(path.into());
        self
    }

    pub fn reversed_client_id(&mut self, scheme: impl Into<String>) -> &mut Self {
        self.reversed_client_id = Some(scheme.into());
        self
    }
}

impl PluginConfig for MasamuneGoogleSignInConfig {
    const NAME: &'static str = "masamune-whisker-google-sign-in";
}

/// The plugin. Register it with `app.project_plugin::<MasamuneGoogleSignIn>(|c| …)`.
pub struct MasamuneGoogleSignIn;

impl ProjectPlugin for MasamuneGoogleSignIn {
    type Config = MasamuneGoogleSignInConfig;

    fn contribute(&self, context: &ProjectContext, config: &Self::Config) -> Result<ProjectUpdate> {
        let ProjectIr::Ios(ios) = &context.project else {
            return Ok(ProjectUpdate::Keep);
        };
        let scheme = match &config.reversed_client_id {
            Some(scheme) => scheme.trim().to_string(),
            None => {
                let path = match &config.ios_config {
                    Some(path) => path.clone(),
                    None => firebase_config_source(&ios.apple.files)
                        .unwrap_or_else(|| DEFAULT_IOS_CONFIG.into()),
                };
                let root = context
                    .app_crate_dir
                    .as_deref()
                    .context("masamune-whisker-google-sign-in needs the app crate directory")?;
                let file = root.join(ProjectPath::new(&path)?.as_str());
                let bytes = std::fs::read(&file)
                    .with_context(|| format!("read Firebase configuration {}", file.display()))?;
                reversed_client_id(&bytes).with_context(|| {
                    format!("{} has no REVERSED_CLIENT_ID; enable Google sign-in in Firebase and download it again", file.display())
                })?
            }
        };
        ensure!(
            !scheme.is_empty(),
            "masamune-whisker-google-sign-in: the reversed client id is empty"
        );
        let mut ios = ios.clone();
        let target = ios
            .apple
            .targets
            .get_mut(&ios.apple.application)
            .context("masamune-whisker-google-sign-in: main iOS application is missing")?;
        add_url_scheme(&mut target.info_plist, &scheme)?;
        Ok(ProjectUpdate::Replace {
            project: Box::new(ProjectIr::Ios(ios)),
            reason: "Register the Google Sign-In redirect URL scheme".into(),
        })
    }
}

/// The app-relative plist the `whisker-firebase` plugin staged, if it already ran.
fn firebase_config_source(
    files: &std::collections::BTreeMap<ProjectPath, ProjectFile>,
) -> Option<String> {
    files.iter().find_map(|(destination, file)| match file {
        ProjectFile::AppFile { source } if destination.as_str() == FIREBASE_PROJECT_FILE => {
            Some(source.as_str().to_string())
        }
        _ => None,
    })
}

/// Reads `REVERSED_CLIENT_ID` from an XML property list.
fn reversed_client_id(bytes: &[u8]) -> Result<String> {
    if bytes.starts_with(b"bplist") {
        bail!("binary property lists are not supported; set reversed_client_id instead");
    }
    let xml = std::str::from_utf8(bytes).context("the property list is not UTF-8")?;
    let key = xml
        .find("<key>REVERSED_CLIENT_ID</key>")
        .context("REVERSED_CLIENT_ID not found")?;
    let rest = &xml[key..];
    let start = rest
        .find("<string>")
        .context("REVERSED_CLIENT_ID has no value")?
        + "<string>".len();
    let end = rest[start..]
        .find("</string>")
        .context("REVERSED_CLIENT_ID has no value")?;
    Ok(rest[start..start + end]
        .trim()
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">"))
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
                    PropertyListValue::String("Editor".into()),
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
    use whisker_plugin::project::{AppleTarget, IosProjectIr};

    const PLIST: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CLIENT_ID</key>
	<string>1234-abcd.apps.googleusercontent.com</string>
	<key>REVERSED_CLIENT_ID</key>
	<string>com.googleusercontent.apps.1234-abcd</string>
</dict>
</plist>
"#;

    fn ios() -> IosProjectIr {
        let mut ios = IosProjectIr::default();
        ios.apple.application = "App".into();
        ios.apple
            .targets
            .insert("App".into(), AppleTarget::default());
        ios
    }

    fn schemes(update: ProjectUpdate) -> PropertyListValue {
        let ProjectUpdate::Replace { project, .. } = update else {
            panic!("expected a replacement")
        };
        let ProjectIr::Ios(ios) = *project else {
            panic!("expected iOS")
        };
        ios.apple.targets["App"].info_plist["CFBundleURLTypes"].clone()
    }

    #[test]
    fn reads_the_reversed_client_id_from_the_app_plist() {
        let dir =
            std::env::temp_dir().join(format!("masamune-google-sign-in-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("firebase")).unwrap();
        std::fs::write(dir.join("firebase/GoogleService-Info.plist"), PLIST).unwrap();
        let context = ProjectContext {
            project: ProjectIr::Ios(ios()),
            app_crate_dir: Some(dir.clone()),
        };
        let config = MasamuneGoogleSignInConfig::default();
        let types = schemes(MasamuneGoogleSignIn.contribute(&context, &config).unwrap());
        let _ = std::fs::remove_dir_all(&dir);
        let PropertyListValue::Array(types) = types else {
            panic!("expected an array")
        };
        assert_eq!(types.len(), 1);
        let PropertyListValue::Dict(entry) = &types[0] else {
            panic!("expected a dictionary")
        };
        assert_eq!(
            entry["CFBundleURLSchemes"],
            PropertyListValue::Array(vec![PropertyListValue::String(
                "com.googleusercontent.apps.1234-abcd".into()
            )])
        );
    }

    #[test]
    fn explicit_scheme_is_added_once() {
        let mut ios = ios();
        add_url_scheme(
            &mut ios.apple.targets.get_mut("App").unwrap().info_plist,
            "my.scheme",
        )
        .unwrap();
        let context = ProjectContext {
            project: ProjectIr::Ios(ios),
            app_crate_dir: None,
        };
        let mut config = MasamuneGoogleSignInConfig::default();
        config.reversed_client_id("my.scheme");
        let PropertyListValue::Array(types) =
            schemes(MasamuneGoogleSignIn.contribute(&context, &config).unwrap())
        else {
            panic!("expected an array")
        };
        assert_eq!(types.len(), 1);
    }

    #[test]
    fn keeps_android_untouched() {
        let context = ProjectContext {
            project: ProjectIr::Android(Box::default()),
            app_crate_dir: None,
        };
        assert!(matches!(
            MasamuneGoogleSignIn
                .contribute(&context, &Default::default())
                .unwrap(),
            ProjectUpdate::Keep
        ));
    }

    #[test]
    fn parser_rejects_missing_keys() {
        assert!(reversed_client_id(b"<plist><dict></dict></plist>").is_err());
        assert!(reversed_client_id(b"bplist00").is_err());
    }
}
