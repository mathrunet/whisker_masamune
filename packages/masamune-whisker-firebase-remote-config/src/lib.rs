//! Firebase Remote Config. All values are returned as strings (callers convert numbers and booleans).
//!
//! On iOS the default Firebase app is configured from the bundled `GoogleService-Info.plist` on
//! first use (Android initializes it at startup); the values are the union of the active and the
//! default keys, like Android's `getAll()`.

use std::collections::BTreeMap;
use std::time::Duration;

mod adapter;

pub use adapter::{FirebaseRemoteConfigMasamuneAdapter, fetch};
use masamune_whisker_bridge::NativeModule;
pub use masamune_whisker_bridge::{NativeError, NativeResult};
use serde_json::json;

fn module() -> NativeModule {
    NativeModule::new(env!("CARGO_PKG_NAME"), "FirebaseRemoteConfig")
}

/// Fetches and activates, then returns the values of all keys. If the fetch fails, returns the cached or default values.
pub async fn fetch_and_activate(
    fetch_timeout: Duration,
    minimum_fetch_interval: Duration,
) -> NativeResult<BTreeMap<String, String>> {
    module()
        .call_async(
            "fetchAndActivate",
            &json!({
                "fetchTimeoutSeconds": fetch_timeout.as_secs(),
                "minimumFetchIntervalSeconds": minimum_fetch_interval.as_secs(),
            }),
        )
        .await
}

/// The currently active values (does not fetch).
pub fn values() -> NativeResult<BTreeMap<String, String>> {
    module().call("values", &json!({}))
}
