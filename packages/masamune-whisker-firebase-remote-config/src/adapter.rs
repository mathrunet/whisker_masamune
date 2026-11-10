use std::collections::BTreeMap;
use std::time::Duration;

use masamune_whisker_adapter::{AdapterError, MasamuneAdapter, StartFuture};

use crate::{NativeResult, fetch_and_activate};

/// Remote Config settings for [`masamune_whisker_adapter::MasamuneAdapters`]: how [`fetch`] fetches.
///
/// Values the first screen depends on (such as a forced update) belong in the boot task, which
/// awaits [`fetch`]; set [`Self::fetch_on_start`] only to warm the cache in the background.
#[derive(Debug, Clone)]
pub struct FirebaseRemoteConfigMasamuneAdapter {
    pub fetch_timeout: Duration,
    pub minimum_fetch_interval: Duration,
    /// Fetch after startup without waiting for it.
    pub fetch_on_start: bool,
}

impl Default for FirebaseRemoteConfigMasamuneAdapter {
    fn default() -> Self {
        Self {
            fetch_timeout: Duration::from_secs(60),
            minimum_fetch_interval: Duration::from_secs(12 * 60 * 60),
            fetch_on_start: false,
        }
    }
}

impl MasamuneAdapter for FirebaseRemoteConfigMasamuneAdapter {
    fn start(&self) -> Option<StartFuture> {
        if !self.fetch_on_start {
            return None;
        }
        Some(Box::pin(async {
            fetch().await?;
            Ok::<(), AdapterError>(())
        }))
    }
}

/// [`fetch_and_activate`] with the intervals of the installed
/// [`FirebaseRemoteConfigMasamuneAdapter`] (its defaults when none is installed).
pub async fn fetch() -> NativeResult<BTreeMap<String, String>> {
    let adapter = FirebaseRemoteConfigMasamuneAdapter::primary().unwrap_or_default();
    fetch_and_activate(adapter.fetch_timeout, adapter.minimum_fetch_interval).await
}
