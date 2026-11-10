//! CNG plugin that declares the URL schemes [`crate::open_url`] may query on iOS.
//!
//! iOS answers `canOpenURL` only for schemes listed in `LSApplicationQueriesSchemes`, so list
//! every custom scheme passed to [`crate::open_url`] (web links need no entry):
//!
//! ```ignore
//! use masamune_whisker_app_control::cng::MasamuneAppControl;
//!
//! app.plugin::<MasamuneAppControl>(|c| {
//!     c.query_scheme("line").query_scheme("twitter");
//! });
//! ```
//!
//! The schemes are merged into any `LSApplicationQueriesSchemes` other plugins declare. Android
//! needs no setup, so the plugin does nothing there.

use anyhow::bail;
use serde::{Deserialize, Serialize};
use whisker_plugin::{GenerateContext, Operation, PlistValue, Plugin, PluginConfig, Target};

/// `Info.plist` key holding the queryable URL schemes.
const QUERIES_SCHEMES: &str = "LSApplicationQueriesSchemes";

/// Settings of [`MasamuneAppControl`].
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct MasamuneAppControlConfig {
    /// URL schemes (without `://`) added to `LSApplicationQueriesSchemes`.
    #[serde(default)]
    pub query_schemes: Vec<String>,
}

impl MasamuneAppControlConfig {
    /// Lets [`crate::open_url`] detect apps handling `scheme` (for example `"line"`) on iOS.
    pub fn query_scheme(&mut self, scheme: impl Into<String>) -> &mut Self {
        let scheme = scheme.into();
        let scheme = scheme.trim_end_matches("://").to_string();
        if !scheme.is_empty() && !self.query_schemes.contains(&scheme) {
            self.query_schemes.push(scheme);
        }
        self
    }
}

impl PluginConfig for MasamuneAppControlConfig {
    const NAME: &'static str = "masamune-whisker-app-control";
}

/// The plugin. Register it with `app.plugin::<MasamuneAppControl>(|c| …)`.
pub struct MasamuneAppControl;

impl Plugin for MasamuneAppControl {
    type Config = MasamuneAppControlConfig;

    fn apply(
        &self,
        ctx: &mut GenerateContext,
        cfg: &MasamuneAppControlConfig,
    ) -> anyhow::Result<()> {
        let Some(ios) = ctx.ios.as_mut() else {
            return Ok(());
        };
        if cfg.query_schemes.is_empty() {
            return Ok(());
        }
        let entry = ios
            .info_plist
            .entry(QUERIES_SCHEMES.to_string())
            .or_insert_with(|| PlistValue::Array(Vec::new()));
        let PlistValue::Array(schemes) = entry else {
            bail!("{QUERIES_SCHEMES} must be an array");
        };
        let mut added = 0;
        for scheme in &cfg.query_schemes {
            let value = PlistValue::String(scheme.clone());
            if !schemes.contains(&value) {
                schemes.push(value);
                added += 1;
            }
        }
        if added > 0 {
            ctx.journal.record(
                MasamuneAppControlConfig::NAME,
                Target::Ios,
                &format!("info_plist.{QUERIES_SCHEMES}"),
                Operation::ArrayPush { count: added },
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use whisker_plugin::IosProjectIr;

    fn ios_context() -> GenerateContext {
        GenerateContext {
            ios: Some(IosProjectIr::default()),
            ..Default::default()
        }
    }

    #[test]
    fn merges_schemes_into_existing_list() {
        let mut ctx = ios_context();
        ctx.ios.as_mut().unwrap().info_plist.insert(
            QUERIES_SCHEMES.into(),
            PlistValue::Array(vec![PlistValue::String("line".into())]),
        );
        let mut cfg = MasamuneAppControlConfig::default();
        cfg.query_scheme("line://").query_scheme("twitter");
        MasamuneAppControl.apply(&mut ctx, &cfg).unwrap();
        assert_eq!(
            ctx.ios.unwrap().info_plist[QUERIES_SCHEMES],
            PlistValue::Array(vec![
                PlistValue::String("line".into()),
                PlistValue::String("twitter".into()),
            ])
        );
    }

    #[test]
    fn empty_config_and_android_are_no_ops() {
        let mut ctx = ios_context();
        MasamuneAppControl
            .apply(&mut ctx, &MasamuneAppControlConfig::default())
            .unwrap();
        assert!(ctx.ios.unwrap().info_plist.is_empty());

        let mut ctx = GenerateContext::default();
        let mut cfg = MasamuneAppControlConfig::default();
        cfg.query_scheme("line");
        MasamuneAppControl.apply(&mut ctx, &cfg).unwrap();
        assert!(ctx.ios.is_none());
    }
}
