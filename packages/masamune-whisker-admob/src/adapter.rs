use masamune_whisker_adapter::{AdapterError, MasamuneAdapter, StartFuture};

use crate::{NativeError, NativeResult, request_tracking_authorization, show_interstitial};

/// AdMob settings for [`masamune_whisker_adapter::MasamuneAdapters`] (Masamune's
/// `GoogleAdsMasamuneAdapter`): the app's default ad units.
///
/// The App ID is a build-time setting written by `AndroidMetaData` and [`crate::cng::MasamuneAdmob`].
#[derive(Debug, Clone, Default)]
pub struct AdmobMasamuneAdapter {
    /// Banner unit returned by [`default_banner_ad_unit_id`].
    pub banner_ad_unit_id: Option<String>,
    /// Interstitial unit shown by [`show_default_interstitial`].
    pub interstitial_ad_unit_id: Option<String>,
    /// Ask for App Tracking Transparency after startup instead of before the first ad.
    pub request_tracking_on_start: bool,
}

impl MasamuneAdapter for AdmobMasamuneAdapter {
    fn start(&self) -> Option<StartFuture> {
        if !self.request_tracking_on_start {
            return None;
        }
        Some(Box::pin(async {
            request_tracking_authorization().await?;
            Ok::<(), AdapterError>(())
        }))
    }
}

/// Banner unit of the installed [`AdmobMasamuneAdapter`].
pub fn default_banner_ad_unit_id() -> Option<String> {
    AdmobMasamuneAdapter::primary()?.banner_ad_unit_id.clone()
}

/// [`show_interstitial`] with the interstitial unit of the installed [`AdmobMasamuneAdapter`].
pub async fn show_default_interstitial() -> NativeResult<bool> {
    let ad_unit_id = AdmobMasamuneAdapter::primary()
        .and_then(|adapter| adapter.interstitial_ad_unit_id.clone())
        .ok_or_else(|| NativeError("no interstitial ad unit in AdmobMasamuneAdapter".into()))?;
    show_interstitial(&ad_unit_id).await
}
