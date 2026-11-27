use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;

use masamune_whisker_adapter::MasamuneAdapter;
use masamune_whisker_logger::LoggerAdapter;

#[cfg(feature = "firebase-remote-config")]
use crate::FirebaseRemoteConfigSource;
use crate::{ExperimentSource, ExperimentValue, RuntimeExperimentSource};

/// [`MasamuneAdapter`] for running A/B tests and feature switches with [`crate::Experiment`].
///
/// Mirrors Flutter's `ExperimentMasamuneAdapter`:
/// - `defaults` are used before values are loaded.
/// - `overrides` always take precedence over loaded values (for debugging/testing; do not pass
///   overrides in production flavors).
/// - `logger_adapters` receive `ExperimentLoggerEvent.exposure` in addition to the global
///   `masamune_whisker_logger` adapters.
pub struct ExperimentMasamuneAdapter {
    pub(crate) source: Rc<dyn ExperimentSource>,
    pub(crate) defaults: BTreeMap<String, ExperimentValue>,
    pub(crate) overrides: BTreeMap<String, ExperimentValue>,
    pub(crate) logger_adapters: Vec<Arc<dyn LoggerAdapter>>,
}

impl ExperimentMasamuneAdapter {
    /// Creates an adapter backed by a custom [`ExperimentSource`].
    pub fn with_source(source: impl ExperimentSource) -> Self {
        Self {
            source: Rc::new(source),
            defaults: BTreeMap::new(),
            overrides: BTreeMap::new(),
            logger_adapters: Vec::new(),
        }
    }

    /// Creates an adapter backed by Firebase Remote Config with default fetch settings.
    #[cfg(feature = "firebase-remote-config")]
    pub fn firebase() -> Self {
        Self::with_source(FirebaseRemoteConfigSource::default())
    }

    /// Creates an in-memory adapter for unit tests and test flavors (never initializes Firebase).
    ///
    /// Values added via [`Self::value`] / [`Self::values`] act as both defaults and loaded values.
    pub fn test() -> Self {
        Self::with_source(RuntimeExperimentSource::default())
    }

    /// Creates an in-memory adapter pre-populated with `values` (used as both defaults and loaded
    /// values) and `overrides`.
    pub fn test_with(
        values: BTreeMap<String, ExperimentValue>,
        overrides: BTreeMap<String, ExperimentValue>,
    ) -> Self {
        Self {
            source: Rc::new(RuntimeExperimentSource::new(values.clone())),
            defaults: values,
            overrides,
            logger_adapters: Vec::new(),
        }
    }

    /// Adds a single default value used before loading.
    pub fn default_value(
        mut self,
        key: impl Into<String>,
        value: impl Into<ExperimentValue>,
    ) -> Self {
        self.defaults.insert(key.into(), value.into());
        self
    }

    /// Extends default values used before loading.
    pub fn defaults<K, V>(mut self, entries: impl IntoIterator<Item = (K, V)>) -> Self
    where
        K: Into<String>,
        V: Into<ExperimentValue>,
    {
        for (key, value) in entries {
            self.defaults.insert(key.into(), value.into());
        }
        self
    }

    /// Adds an override value that always takes precedence over loaded values.
    ///
    /// Reading an overridden key suppresses exposure logging. Do not pass overrides in prod.
    pub fn override_value(
        mut self,
        key: impl Into<String>,
        value: impl Into<ExperimentValue>,
    ) -> Self {
        self.overrides.insert(key.into(), value.into());
        self
    }

    /// Extends override values.
    pub fn overrides<K, V>(mut self, entries: impl IntoIterator<Item = (K, V)>) -> Self
    where
        K: Into<String>,
        V: Into<ExperimentValue>,
    {
        for (key, value) in entries {
            self.overrides.insert(key.into(), value.into());
        }
        self
    }

    /// Adds a [`LoggerAdapter`] that receives exposure events in addition to the global logger.
    pub fn logger_adapter(mut self, adapter: impl LoggerAdapter) -> Self {
        self.logger_adapters.push(Arc::new(adapter));
        self
    }

    pub fn defaults_map(&self) -> &BTreeMap<String, ExperimentValue> {
        &self.defaults
    }

    pub fn overrides_map(&self) -> &BTreeMap<String, ExperimentValue> {
        &self.overrides
    }
}

impl MasamuneAdapter for ExperimentMasamuneAdapter {}
