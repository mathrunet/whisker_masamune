use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
#[cfg(feature = "firebase-remote-config")]
use std::time::Duration;

use crate::ExperimentValue;

/// Failure returned by [`ExperimentSource::load`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExperimentError(pub String);

impl ExperimentError {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl std::fmt::Display for ExperimentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ExperimentError {}

/// Future returned by [`ExperimentSource::load`].
pub type LoadFuture =
    Pin<Box<dyn Future<Output = Result<BTreeMap<String, ExperimentValue>, ExperimentError>>>>;

/// Backend that loads experiment values (for example Firebase Remote Config or an in-memory map).
pub trait ExperimentSource: 'static {
    fn load(&self) -> LoadFuture;
}

/// In-memory [`ExperimentSource`] for unit tests and test flavors. Never calls Firebase.
#[derive(Clone, Debug, Default)]
pub struct RuntimeExperimentSource {
    values: BTreeMap<String, ExperimentValue>,
}

impl RuntimeExperimentSource {
    pub fn new(values: BTreeMap<String, ExperimentValue>) -> Self {
        Self { values }
    }
}

impl ExperimentSource for RuntimeExperimentSource {
    fn load(&self) -> LoadFuture {
        let values = self.values.clone();
        Box::pin(async move { Ok(values) })
    }
}

/// [`ExperimentSource`] backed by `masamune-whisker-firebase-remote-config`.
#[cfg(feature = "firebase-remote-config")]
#[derive(Clone, Copy, Debug)]
pub struct FirebaseRemoteConfigSource {
    pub fetch_timeout: Duration,
    pub minimum_fetch_interval: Duration,
}

#[cfg(feature = "firebase-remote-config")]
impl Default for FirebaseRemoteConfigSource {
    fn default() -> Self {
        Self {
            fetch_timeout: Duration::from_secs(10),
            minimum_fetch_interval: Duration::ZERO,
        }
    }
}

#[cfg(feature = "firebase-remote-config")]
impl ExperimentSource for FirebaseRemoteConfigSource {
    fn load(&self) -> LoadFuture {
        let timeout = self.fetch_timeout;
        let min_interval = self.minimum_fetch_interval;
        Box::pin(async move {
            let raw =
                masamune_whisker_firebase_remote_config::fetch_and_activate(timeout, min_interval)
                    .await
                    .map_err(|err| ExperimentError::new(err.to_string()))?;
            Ok(raw
                .into_iter()
                .map(|(k, v)| (k, ExperimentValue::String(v)))
                .collect())
        })
    }
}
