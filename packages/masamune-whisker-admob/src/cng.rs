//! CNG plugin that writes the iOS `Info.plist` keys the Google Mobile Ads SDK needs.
//!
//! ```ignore
//! use masamune_whisker_admob::cng::MasamuneAdmob;
//!
//! app.plugin::<MasamuneAdmob>(|c| {
//!     c.ios_app_id("ca-app-pub-0000000000000000~0000000000")
//!         .tracking_usage_description("Allow tracking to show ads tailored to you.");
//! });
//! ```
//!
//! - `GADApplicationIdentifier` — the AdMob App ID for the iOS app (required; the SDK crashes at start
//!   without it).
//! - `SKAdNetworkItems` — Google's recommended SKAdNetwork identifiers ([`GOOGLE_SK_AD_NETWORK_IDS`]) unless
//!   replaced with [`MasamuneAdmobConfig::sk_ad_network_ids`] or extended with
//!   [`MasamuneAdmobConfig::add_sk_ad_network_id`].
//! - `NSUserTrackingUsageDescription` — only when set; required before calling
//!   [`request_tracking_authorization`](crate::request_tracking_authorization).
//!
//! The plugin does nothing for Android: keep declaring `com.google.android.gms.ads.APPLICATION_ID` with
//! `AndroidMetaData`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use whisker_plugin::{GenerateContext, Operation, PlistValue, Plugin, PluginConfig, Target};

/// Google's recommended SKAdNetwork identifiers for AdMob
/// (<https://developers.google.com/admob/ios/3p-skadnetworks>, last updated April 2, 2025).
pub const GOOGLE_SK_AD_NETWORK_IDS: &[&str] = &[
    "cstr6suwn9.skadnetwork",
    "4fzdc2evr5.skadnetwork",
    "2fnua5tdw4.skadnetwork",
    "ydx93a7ass.skadnetwork",
    "p78axxw29g.skadnetwork",
    "v72qych5uu.skadnetwork",
    "ludvb6z3bs.skadnetwork",
    "cp8zw746q7.skadnetwork",
    "3sh42y64q3.skadnetwork",
    "c6k4g5qg8m.skadnetwork",
    "s39g8k73mm.skadnetwork",
    "wg4vff78zm.skadnetwork",
    "3qy4746246.skadnetwork",
    "f38h382jlk.skadnetwork",
    "hs6bdukanm.skadnetwork",
    "mlmmfzh3r3.skadnetwork",
    "v4nxqhlyqp.skadnetwork",
    "wzmmz9fp6w.skadnetwork",
    "su67r6k2v3.skadnetwork",
    "yclnxrl5pm.skadnetwork",
    "t38b2kh725.skadnetwork",
    "7ug5zh24hu.skadnetwork",
    "gta9lk7p23.skadnetwork",
    "vutu7akeur.skadnetwork",
    "y5ghdn5j9k.skadnetwork",
    "v9wttpbfk9.skadnetwork",
    "n38lu8286q.skadnetwork",
    "47vhws6wlr.skadnetwork",
    "kbd757ywx3.skadnetwork",
    "9t245vhmpl.skadnetwork",
    "a2p9lx4jpn.skadnetwork",
    "22mmun2rn5.skadnetwork",
    "44jx6755aq.skadnetwork",
    "k674qkevps.skadnetwork",
    "4468km3ulz.skadnetwork",
    "2u9pt9hc89.skadnetwork",
    "8s468mfl3y.skadnetwork",
    "klf5c3l5u5.skadnetwork",
    "ppxm28t8ap.skadnetwork",
    "kbmxgpxpgc.skadnetwork",
    "uw77j35x4d.skadnetwork",
    "578prtvx9j.skadnetwork",
    "4dzt52r2t5.skadnetwork",
    "tl55sbb4fm.skadnetwork",
    "c3frkrj4fj.skadnetwork",
    "e5fvkxwrpn.skadnetwork",
    "8c4e2ghe7u.skadnetwork",
    "3rd42ekr43.skadnetwork",
    "97r2b46745.skadnetwork",
    "3qcr597p9d.skadnetwork",
];

/// Settings of [`MasamuneAdmob`].
#[derive(Default, Serialize, Deserialize)]
pub struct MasamuneAdmobConfig {
    /// AdMob App ID of the iOS app (`GADApplicationIdentifier`).
    #[serde(default)]
    pub ios_app_id: Option<String>,
    /// SKAdNetwork identifiers. `None` uses [`GOOGLE_SK_AD_NETWORK_IDS`].
    #[serde(default)]
    pub sk_ad_network_ids: Option<Vec<String>>,
    /// Identifiers appended to the list above.
    #[serde(default)]
    pub extra_sk_ad_network_ids: Vec<String>,
    /// App Tracking Transparency prompt text (`NSUserTrackingUsageDescription`).
    #[serde(default)]
    pub tracking_usage_description: Option<String>,
}

impl MasamuneAdmobConfig {
    /// Sets the AdMob App ID of the iOS app (`ca-app-pub-…~…`).
    pub fn ios_app_id(&mut self, app_id: impl Into<String>) -> &mut Self {
        self.ios_app_id = Some(app_id.into());
        self
    }

    /// Replaces Google's recommended SKAdNetwork identifiers.
    pub fn sk_ad_network_ids<I, S>(&mut self, ids: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.sk_ad_network_ids = Some(ids.into_iter().map(Into::into).collect());
        self
    }

    /// Adds one SKAdNetwork identifier (for example a mediation partner's) to the list.
    pub fn add_sk_ad_network_id(&mut self, id: impl Into<String>) -> &mut Self {
        self.extra_sk_ad_network_ids.push(id.into());
        self
    }

    /// Sets the App Tracking Transparency prompt text (`NSUserTrackingUsageDescription`).
    pub fn tracking_usage_description(&mut self, text: impl Into<String>) -> &mut Self {
        self.tracking_usage_description = Some(text.into());
        self
    }

    /// SKAdNetwork identifiers to write, without duplicates and in declaration order.
    pub fn resolved_sk_ad_network_ids(&self) -> Vec<String> {
        let base = match &self.sk_ad_network_ids {
            Some(ids) => ids.clone(),
            None => GOOGLE_SK_AD_NETWORK_IDS
                .iter()
                .map(|id| id.to_string())
                .collect(),
        };
        let mut ids: Vec<String> = Vec::new();
        for id in base
            .into_iter()
            .chain(self.extra_sk_ad_network_ids.iter().cloned())
        {
            let id = id.trim().to_lowercase();
            if !id.is_empty() && !ids.contains(&id) {
                ids.push(id);
            }
        }
        ids
    }
}

impl PluginConfig for MasamuneAdmobConfig {
    const NAME: &'static str = "masamune-whisker-admob";
}

/// The plugin. Register it with `app.plugin::<MasamuneAdmob>(|c| …)`.
pub struct MasamuneAdmob;

impl Plugin for MasamuneAdmob {
    type Config = MasamuneAdmobConfig;

    fn apply(&self, ctx: &mut GenerateContext, cfg: &MasamuneAdmobConfig) -> anyhow::Result<()> {
        let Some(ios) = ctx.ios.as_mut() else {
            return Ok(());
        };
        let mut set = |key: &str, value: PlistValue| {
            ios.info_plist.insert(key.to_string(), value);
            ctx.journal.record(
                MasamuneAdmobConfig::NAME,
                Target::Ios,
                &format!("info_plist.{key}"),
                Operation::Set,
            );
        };
        if let Some(app_id) = cfg
            .ios_app_id
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty())
        {
            set(
                "GADApplicationIdentifier",
                PlistValue::String(app_id.to_string()),
            );
        }
        let items = cfg
            .resolved_sk_ad_network_ids()
            .into_iter()
            .map(|id| {
                PlistValue::Dict(BTreeMap::from([(
                    "SKAdNetworkIdentifier".to_string(),
                    PlistValue::String(id),
                )]))
            })
            .collect::<Vec<_>>();
        if !items.is_empty() {
            set("SKAdNetworkItems", PlistValue::Array(items));
        }
        if let Some(text) = cfg
            .tracking_usage_description
            .as_deref()
            .filter(|t| !t.trim().is_empty())
        {
            set(
                "NSUserTrackingUsageDescription",
                PlistValue::String(text.to_string()),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use whisker_plugin::{AndroidProjectIr, IosProjectIr};

    fn ios_ctx() -> GenerateContext {
        GenerateContext {
            ios: Some(IosProjectIr::default()),
            ..Default::default()
        }
    }

    #[test]
    fn writes_ios_keys() {
        let mut cfg = MasamuneAdmobConfig::default();
        cfg.ios_app_id("ca-app-pub-1~2")
            .tracking_usage_description("Ads")
            .add_sk_ad_network_id("CSTR6SUWN9.skadnetwork")
            .add_sk_ad_network_id("extra.skadnetwork");
        let mut ctx = ios_ctx();
        MasamuneAdmob.apply(&mut ctx, &cfg).unwrap();
        let plist = &ctx.ios.as_ref().unwrap().info_plist;
        assert_eq!(
            plist.get("GADApplicationIdentifier"),
            Some(&PlistValue::String("ca-app-pub-1~2".into()))
        );
        assert_eq!(
            plist.get("NSUserTrackingUsageDescription"),
            Some(&PlistValue::String("Ads".into()))
        );
        let Some(PlistValue::Array(items)) = plist.get("SKAdNetworkItems") else {
            panic!("SKAdNetworkItems missing");
        };
        // Google's list plus one new identifier (the upper-case duplicate is dropped).
        assert_eq!(items.len(), GOOGLE_SK_AD_NETWORK_IDS.len() + 1);
        assert_eq!(
            items[0],
            PlistValue::Dict(BTreeMap::from([(
                "SKAdNetworkIdentifier".to_string(),
                PlistValue::String("cstr6suwn9.skadnetwork".into())
            )]))
        );
    }

    #[test]
    fn replaces_sk_ad_network_ids_and_skips_unset_keys() {
        let mut cfg = MasamuneAdmobConfig::default();
        cfg.sk_ad_network_ids(["a.skadnetwork"]);
        let mut ctx = ios_ctx();
        MasamuneAdmob.apply(&mut ctx, &cfg).unwrap();
        let plist = &ctx.ios.as_ref().unwrap().info_plist;
        assert!(!plist.contains_key("GADApplicationIdentifier"));
        assert!(!plist.contains_key("NSUserTrackingUsageDescription"));
        assert!(
            matches!(plist.get("SKAdNetworkItems"), Some(PlistValue::Array(items)) if items.len() == 1)
        );
    }

    #[test]
    fn android_is_untouched() {
        let mut cfg = MasamuneAdmobConfig::default();
        cfg.ios_app_id("ca-app-pub-1~2");
        let mut ctx = GenerateContext {
            android: Some(AndroidProjectIr::default()),
            ..Default::default()
        };
        MasamuneAdmob.apply(&mut ctx, &cfg).unwrap();
        assert!(ctx.ios.is_none());
        assert!(ctx.journal.records.is_empty());
    }
}
