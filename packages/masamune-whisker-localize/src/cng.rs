//! CNG plugin that declares the app's languages on iOS (a no-op on Android).
//!
//! ```ignore
//! use masamune_whisker_localize::cng::MasamuneLocalize;
//!
//! app.project_plugin::<MasamuneLocalize>(|c| {
//!     c.strings("en", "CFBundleDisplayName", "NANSURU")
//!         .strings("ja", "CFBundleDisplayName", "ナンスル")
//!         .strings("ja", "NSCameraUsageDescription", "プロフィール画像の撮影に使います。");
//! });
//! ```
//!
//! For every language it bundles `<language>.lproj/InfoPlist.strings` and lists the language in
//! `CFBundleLocalizations`. iOS only uses a language for the app's system UI (date pickers, share
//! sheets, permission prompts) and its home screen name when the bundle declares it; without this
//! the system UI stays in the development language whatever the device language is.

use std::collections::BTreeMap;

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use whisker_plugin::project::{
    AppleResource, IosProjectIr, ProjectContext, ProjectFile, ProjectIr, ProjectPath,
    ProjectPlugin, ProjectUpdate, PropertyListValue, Resource, ResourceKind,
};
use whisker_plugin::{FileEntry, PluginConfig};

/// Settings of [`MasamuneLocalize`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MasamuneLocalizeConfig {
    /// `InfoPlist.strings` entries per `.lproj` language (for example `en`, `ja`, `zh-Hans`).
    pub info_plist_strings: BTreeMap<String, BTreeMap<String, String>>,
}

impl MasamuneLocalizeConfig {
    /// Declares a language without localized `Info.plist` values.
    pub fn language(&mut self, language: impl Into<String>) -> &mut Self {
        self.info_plist_strings.entry(language.into()).or_default();
        self
    }

    /// Localizes one `Info.plist` key (such as `CFBundleDisplayName` or a usage description)
    /// for `language`, declaring the language as well.
    pub fn strings(
        &mut self,
        language: impl Into<String>,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> &mut Self {
        self.info_plist_strings
            .entry(language.into())
            .or_default()
            .insert(key.into(), value.into());
        self
    }
}

impl PluginConfig for MasamuneLocalizeConfig {
    const NAME: &'static str = "masamune-whisker-localize";
}

/// The plugin. Register it with `app.project_plugin::<MasamuneLocalize>(|c| …)`.
pub struct MasamuneLocalize;

impl ProjectPlugin for MasamuneLocalize {
    type Config = MasamuneLocalizeConfig;

    fn contribute(&self, context: &ProjectContext, config: &Self::Config) -> Result<ProjectUpdate> {
        let ProjectIr::Ios(ios) = &context.project else {
            return Ok(ProjectUpdate::Keep);
        };
        if config.info_plist_strings.is_empty() {
            return Ok(ProjectUpdate::Keep);
        }
        let mut ios = ios.clone();
        let mut target = ios
            .apple
            .targets
            .get(&ios.apple.application)
            .context("masamune-whisker-localize: main iOS application is missing")?
            .clone();
        let mut addition = IosProjectIr::default();
        for (language, entries) in &config.info_plist_strings {
            ensure!(
                !language.is_empty()
                    && language
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
                "masamune-whisker-localize: invalid language `{language}`"
            );
            let directory = ProjectPath::new(format!("Localizations/{language}.lproj"))?;
            addition.apple.files.insert(
                ProjectPath::new(format!("Localizations/{language}.lproj/InfoPlist.strings"))?,
                ProjectFile::Generated {
                    entry: FileEntry::text(render_strings(entries)),
                },
            );
            target.resources.push(AppleResource::Copy {
                resource: Resource {
                    source: directory,
                    destination: ProjectPath::new(format!("{language}.lproj"))?,
                    kind: ResourceKind::Directory,
                },
            });
        }
        target.info_plist.insert(
            "CFBundleLocalizations".into(),
            PropertyListValue::Array(
                config
                    .info_plist_strings
                    .keys()
                    .map(|language| PropertyListValue::String(language.clone()))
                    .collect(),
            ),
        );
        addition
            .apple
            .targets
            .insert(ios.apple.application.clone(), target);
        ios.merge_from(&addition)?;
        Ok(ProjectUpdate::Replace {
            project: Box::new(ProjectIr::Ios(ios)),
            reason: "Declare the app's languages".into(),
        })
    }
}

/// Renders a `.strings` file (UTF-8 text, which iOS reads as is).
fn render_strings(entries: &BTreeMap<String, String>) -> String {
    let escape = |text: &str| {
        text.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
    };
    entries
        .iter()
        .map(|(key, value)| format!("\"{}\" = \"{}\";\n", escape(key), escape(value)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use whisker_plugin::project::AppleTarget;

    #[test]
    fn escapes_strings() {
        let mut entries = BTreeMap::new();
        entries.insert("Key".to_string(), "Say \"hi\"\\\nnow".to_string());
        assert_eq!(
            render_strings(&entries),
            "\"Key\" = \"Say \\\"hi\\\"\\\\\\nnow\";\n"
        );
    }

    #[test]
    fn declares_languages_on_ios_only() {
        let mut ios = IosProjectIr::default();
        ios.apple.application = "App".into();
        ios.apple
            .targets
            .insert("App".into(), AppleTarget::default());
        let context = ProjectContext {
            project: ProjectIr::Ios(ios),
            app_crate_dir: None,
        };
        let mut config = MasamuneLocalizeConfig::default();
        config
            .strings("ja", "CFBundleDisplayName", "ナンスル")
            .language("en");
        let ProjectUpdate::Replace { project, .. } =
            MasamuneLocalize.contribute(&context, &config).unwrap()
        else {
            panic!("expected a replacement")
        };
        let ProjectIr::Ios(ios) = *project else {
            panic!("expected iOS")
        };
        let target = &ios.apple.targets["App"];
        assert_eq!(
            target.info_plist["CFBundleLocalizations"],
            PropertyListValue::Array(vec![
                PropertyListValue::String("en".into()),
                PropertyListValue::String("ja".into()),
            ])
        );
        assert_eq!(target.resources.len(), 2);
        assert!(
            ios.apple.files.contains_key(
                &ProjectPath::new("Localizations/ja.lproj/InfoPlist.strings").unwrap()
            )
        );

        let android = ProjectContext {
            project: ProjectIr::Android(Box::default()),
            app_crate_dir: None,
        };
        assert!(matches!(
            MasamuneLocalize.contribute(&android, &config).unwrap(),
            ProjectUpdate::Keep
        ));
    }
}
