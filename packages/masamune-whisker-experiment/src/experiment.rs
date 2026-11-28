use std::cell::RefCell;
use std::collections::{BTreeMap, HashSet};
use std::rc::Rc;
use std::sync::{LazyLock, Mutex};

use masamune_whisker_adapter::MasamuneAdapter;
use tokio::sync::Notify;
use whisker::prelude::*;

use crate::exposure::ExposureLog;
use crate::{
    ExperimentCase, ExperimentConfig, ExperimentError, ExperimentExposure, ExperimentLoggerEvent,
    ExperimentMasamuneAdapter, ExperimentValue,
};

/// Options passed to [`Experiment::run`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RunOptions {
    /// Forces the branch with this label to run regardless of config values, suppressing exposure.
    pub fixed: Option<String>,
    /// Records the selected branch as `<experiment_id>:<label>` via `ExperimentLoggerEvent.exposure`.
    pub exposure: Option<ExperimentExposure>,
}

impl RunOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn fixed(mut self, label: impl Into<String>) -> Self {
        self.fixed = Some(label.into());
        self
    }

    pub fn exposure(mut self, exposure: ExperimentExposure) -> Self {
        self.exposure = Some(exposure);
        self
    }
}

struct ExperimentState {
    loaded: bool,
    loading: Option<Rc<Notify>>,
    values: BTreeMap<String, ExperimentValue>,
    selected_labels: Vec<String>,
}

struct ExperimentInner {
    adapter: Rc<ExperimentMasamuneAdapter>,
    state: RefCell<ExperimentState>,
}

static SENT_EXPOSURES: LazyLock<Mutex<HashSet<(String, String)>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

/// Reactive controller for A/B tests and feature switches (Flutter's `Experiment`).
///
/// `Experiment` is `Clone + Copy` (backed by Whisker signals), so it can be stored directly in
/// `AppState` and read inside `computed` / `effect` closures. Calling [`Self::load`],
/// [`Self::reload`], or [`Self::apply_values`] notifies reactive subscribers automatically.
#[derive(Clone, Copy)]
pub struct Experiment {
    inner: RwSignal<Rc<ExperimentInner>>,
    version: RwSignal<u64>,
}

impl Experiment {
    /// Maximum number of labels kept in [`Self::selected_labels`].
    pub const MAX_SELECTED_LABELS: usize = 100;

    /// Creates a controller backed by the [`ExperimentMasamuneAdapter`] installed in
    /// `MasamuneAdapters`, falling back to an empty in-memory test adapter if none is installed.
    pub fn primary() -> Self {
        let adapter = ExperimentMasamuneAdapter::primary()
            .unwrap_or_else(|| Rc::new(ExperimentMasamuneAdapter::test()));
        Self::from_rc(adapter)
    }

    /// Creates a controller backed by an explicit [`ExperimentMasamuneAdapter`].
    pub fn new(adapter: ExperimentMasamuneAdapter) -> Self {
        Self::from_rc(Rc::new(adapter))
    }

    fn from_rc(adapter: Rc<ExperimentMasamuneAdapter>) -> Self {
        let inner = Rc::new(ExperimentInner {
            adapter,
            state: RefCell::new(ExperimentState {
                loaded: false,
                loading: None,
                values: BTreeMap::new(),
                selected_labels: Vec::new(),
            }),
        });
        Self {
            inner: signal(inner),
            version: signal(0),
        }
    }

    /// Returns `true` if values have been loaded (or applied via [`Self::apply_values`]).
    pub fn is_loaded(&self) -> bool {
        let _ = self.version.get();
        self.inner.get_untracked().state.borrow().loaded
    }

    /// Current merged values (`defaults` → loaded values → `overrides`).
    ///
    /// Subscribes the current reactive scope so `computed` / `effect` re-runs after load/reload.
    pub fn config(&self) -> ExperimentConfig {
        let _ = self.version.get();
        ExperimentConfig::new(self.merged_values())
    }

    /// Labels selected by [`Self::run`], in selection order (up to [`Self::MAX_SELECTED_LABELS`]).
    pub fn selected_labels(&self) -> Vec<String> {
        self.inner
            .get_untracked()
            .state
            .borrow()
            .selected_labels
            .clone()
    }

    /// Clears [`Self::selected_labels`].
    pub fn clear_selected_labels(&self) {
        self.inner
            .get_untracked()
            .state
            .borrow_mut()
            .selected_labels
            .clear();
    }

    /// Clears the process-wide deduplication set of already-sent exposures. Use only in tests.
    pub fn clear_sent_exposures() {
        SENT_EXPOSURES
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
    }

    /// Loads values from the adapter's [`crate::ExperimentSource`] once.
    ///
    /// Subsequent calls return immediately once loaded; concurrent calls share the in-flight load.
    pub async fn load(&self) -> Result<(), ExperimentError> {
        let inner = self.inner.get_untracked();
        let wait_notify = {
            let state = inner.state.borrow();
            if state.loaded {
                return Ok(());
            }
            state.loading.clone()
        };
        if let Some(notify) = wait_notify {
            notify.notified().await;
            return Ok(());
        }
        self.load_inner(inner).await
    }

    /// Reloads values from the adapter's [`crate::ExperimentSource`].
    pub async fn reload(&self) -> Result<(), ExperimentError> {
        let inner = self.inner.get_untracked();
        let wait_notify = inner.state.borrow().loading.clone();
        if let Some(notify) = wait_notify {
            notify.notified().await;
        }
        self.load_inner(inner).await
    }

    async fn load_inner(&self, inner: Rc<ExperimentInner>) -> Result<(), ExperimentError> {
        let notify = Rc::new(Notify::new());
        inner.state.borrow_mut().loading = Some(notify.clone());
        let result = inner.adapter.source.load().await;
        {
            let mut state = inner.state.borrow_mut();
            state.loading = None;
            if let Ok(values) = &result {
                state.values = values.clone();
                state.loaded = true;
            }
        }
        notify.notify_waiters();
        if result.is_ok() {
            self.bump_version();
        }
        result.map(|_| ())
    }

    /// Applies an externally fetched map of values (for example from a shared startup Remote
    /// Config fetch), marks the controller as loaded, and notifies reactive subscribers.
    pub fn apply_values<K, V>(&self, values: impl IntoIterator<Item = (K, V)>)
    where
        K: Into<String>,
        V: Into<ExperimentValue>,
    {
        let mapped: BTreeMap<String, ExperimentValue> = values
            .into_iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect();
        let inner = self.inner.get_untracked();
        {
            let mut state = inner.state.borrow_mut();
            state.values = mapped;
            state.loaded = true;
        }
        self.bump_version();
    }

    /// Evaluates `value` against the current config, runs the first matching [`ExperimentCase`],
    /// and returns its result (or `None` if no case matches).
    ///
    /// Subscribes the current reactive scope so `computed` / `effect` re-runs after load/reload.
    pub fn run<V, R>(
        &self,
        value: impl FnOnce(&ExperimentConfig) -> V,
        cases: impl IntoIterator<Item = ExperimentCase<V, R>>,
        options: RunOptions,
    ) -> Option<R> {
        let _ = self.version.get();
        let cases: Vec<ExperimentCase<V, R>> = cases.into_iter().collect();
        debug_assert!(
            {
                let mut seen = HashSet::new();
                cases.iter().all(|c| seen.insert(c.label()))
            },
            "The labels of ExperimentCase must be unique"
        );
        debug_assert!(
            options
                .exposure
                .as_ref()
                .is_none_or(ExperimentExposure::is_valid_user_property),
            "Invalid ExperimentExposure user_property: {:?}",
            options.exposure
        );

        if let Some(fixed) = options.fixed.as_deref() {
            for case in cases {
                if case.label() == fixed {
                    self.record_selected(case.label());
                    return Some(case.execute());
                }
            }
            debug_assert!(false, "The fixed label [{fixed}] is not found in cases");
            return None;
        }

        let merged = self.merged_values();
        let config = if options.exposure.is_some() {
            ExperimentConfig::tracking(merged)
        } else {
            ExperimentConfig::new(merged)
        };
        let current = value(&config);
        let accessed_keys = config.into_accessed_keys();

        for case in cases {
            if case.matches(&current) {
                let label = case.label().to_owned();
                self.record_selected(&label);
                if let Some(exposure) = options.exposure.as_ref() {
                    self.expose(exposure, &label, &accessed_keys);
                }
                return Some(case.execute());
            }
        }
        None
    }

    fn merged_values(&self) -> BTreeMap<String, ExperimentValue> {
        let inner = self.inner.get_untracked();
        let state = inner.state.borrow();
        let mut merged = inner.adapter.defaults.clone();
        for (k, v) in &state.values {
            merged.insert(k.clone(), v.clone());
        }
        for (k, v) in &inner.adapter.overrides {
            merged.insert(k.clone(), v.clone());
        }
        merged
    }

    fn record_selected(&self, label: &str) {
        let inner = self.inner.get_untracked();
        let mut state = inner.state.borrow_mut();
        state.selected_labels.push(label.to_owned());
        if state.selected_labels.len() > Self::MAX_SELECTED_LABELS {
            state.selected_labels.remove(0);
        }
    }

    fn expose(&self, exposure: &ExperimentExposure, label: &str, accessed_keys: &HashSet<String>) {
        let value = exposure.value_of(label);
        debug_assert!(
            value.len() <= ExperimentExposure::MAX_VALUE_LENGTH,
            "The exposure value [{value}] must be at most {} characters",
            ExperimentExposure::MAX_VALUE_LENGTH
        );
        let inner = self.inner.get_untracked();
        if !inner.state.borrow().loaded {
            return;
        }
        if accessed_keys
            .iter()
            .any(|key| inner.adapter.overrides.contains_key(key))
        {
            return;
        }
        let dedup_key = (exposure.user_property.clone(), value);
        {
            let mut sent = SENT_EXPOSURES.lock().unwrap_or_else(|e| e.into_inner());
            if !sent.insert(dedup_key) {
                return;
            }
        }
        let loggable = ExposureLog {
            exposure: exposure.clone(),
            label: label.to_owned(),
        };
        masamune_whisker_logger::send(&loggable);
        let event = ExperimentLoggerEvent::exposure_event(exposure, label);
        for logger_adapter in &inner.adapter.logger_adapters {
            logger_adapter.send(&event);
        }
    }

    fn bump_version(&self) {
        let next = self.version.get_untracked().wrapping_add(1);
        self.version.set(next);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use masamune_whisker_logger::{LogEntry, LogValue, RuntimeLoggerAdapter};
    use whisker::runtime::{RuntimeContext, RuntimeWakeHandle};

    fn with_runtime<R>(f: impl FnOnce() -> R) -> R {
        RuntimeContext::new(RuntimeWakeHandle::new(|| {})).enter(|| {
            let owner = whisker::Owner::new(None);
            let result = owner.with(f);
            owner.dispose();
            result
        })
    }

    #[test]
    fn config_getters_follow_remote_config_rules() {
        let mut map = BTreeMap::new();
        map.insert("s".into(), ExperimentValue::from("hello"));
        map.insert("b_true".into(), ExperimentValue::from("YES"));
        map.insert("b_false".into(), ExperimentValue::from("no"));
        map.insert("b_native".into(), ExperimentValue::from(true));
        map.insert("i".into(), ExperimentValue::from("42"));
        map.insert("i_bad".into(), ExperimentValue::from("nan"));
        map.insert("f".into(), ExperimentValue::from("3.5"));

        let config = ExperimentConfig::new(map);
        assert!(config.contains_key("s"));
        assert!(!config.contains_key("missing"));
        assert_eq!(config.get_string("s"), "hello");
        assert_eq!(config.get_string("missing"), "");
        assert!(config.get_bool("b_true"));
        assert!(!config.get_bool("b_false"));
        assert!(config.get_bool("b_native"));
        assert!(!config.get_bool("missing"));
        assert_eq!(config.get_int("i"), 42);
        assert_eq!(config.get_int("i_bad"), 0);
        assert_eq!(config.get_int("missing"), 0);
        assert!((config.get_float("f") - 3.5).abs() < 1e-6);
        assert_eq!(config.get_float("missing"), 0.0);
    }

    #[test]
    fn exposure_validation_matches_ga4_rules() {
        assert!(ExperimentExposure::new("shinogi_ab_5", "F-20261127-02").is_valid_user_property());
        assert_eq!(
            ExperimentExposure::new("shinogi_ab_5", "F-20261127-02").value_of("treatment"),
            "F-20261127-02:treatment"
        );
        let reserved = ExperimentExposure {
            user_property: "firebase_ab".into(),
            experiment_id: "F-1".into(),
        };
        assert!(!reserved.is_valid_user_property());
        let bad_start = ExperimentExposure {
            user_property: "1_ab".into(),
            experiment_id: "F-1".into(),
        };
        assert!(!bad_start.is_valid_user_property());
    }

    #[test]
    fn precedence_is_defaults_then_loaded_then_overrides() {
        with_runtime(|| {
            let adapter = ExperimentMasamuneAdapter::test()
                .default_value("flag_a", "control")
                .default_value("flag_b", "control")
                .override_value("flag_b", "forced");
            let exp = Experiment::new(adapter);

            assert_eq!(exp.config().get_string("flag_a"), "control");
            assert_eq!(exp.config().get_string("flag_b"), "forced");

            exp.apply_values([("flag_a", "treatment"), ("flag_b", "treatment")]);
            assert_eq!(exp.config().get_string("flag_a"), "treatment");
            assert_eq!(exp.config().get_string("flag_b"), "forced");
        });
    }

    #[test]
    fn run_selects_branch_supports_fixed_and_caps_selected_labels() {
        with_runtime(|| {
            let exp = Experiment::new(
                ExperimentMasamuneAdapter::test().default_value("flag", "treatment"),
            );
            let out = exp.run(
                |c| c.get_string("flag"),
                [
                    ExperimentCase::equals("treatment", "treatment", || 2),
                    ExperimentCase::otherwise("control", || 1),
                ],
                RunOptions::default(),
            );
            assert_eq!(out, Some(2));
            assert_eq!(exp.selected_labels(), vec!["treatment"]);

            let fixed = exp.run(
                |c| c.get_string("flag"),
                [
                    ExperimentCase::equals("treatment", "treatment", || 2),
                    ExperimentCase::otherwise("control", || 1),
                ],
                RunOptions::new().fixed("control"),
            );
            assert_eq!(fixed, Some(1));
            assert_eq!(exp.selected_labels(), vec!["treatment", "control"]);

            exp.clear_selected_labels();
            for _ in 0..(Experiment::MAX_SELECTED_LABELS + 5) {
                let _ = exp.run(
                    |c| c.get_string("flag"),
                    [ExperimentCase::otherwise("control", || 1)],
                    RunOptions::default(),
                );
            }
            assert_eq!(exp.selected_labels().len(), Experiment::MAX_SELECTED_LABELS);
        });
    }

    #[test]
    fn exposure_suppressed_before_load_on_override_and_on_fixed_and_deduplicated() {
        with_runtime(|| {
            Experiment::clear_sent_exposures();
            let logger = RuntimeLoggerAdapter::new(32);
            let adapter = ExperimentMasamuneAdapter::test()
                .default_value("flag", "control")
                .override_value("overridden", "treatment")
                .logger_adapter(logger.clone());
            let exp = Experiment::new(adapter);
            let exposure = ExperimentExposure::new("shinogi_ab_test", "F-TEST-01");

            // Before load -> suppressed
            let _ = exp.run(
                |c| c.get_string("flag"),
                [ExperimentCase::otherwise("control", || ())],
                RunOptions::new().exposure(exposure.clone()),
            );
            assert!(logger.entries().is_empty());

            // After load -> sent once, then deduplicated
            exp.apply_values([("flag", "treatment")]);
            for _ in 0..3 {
                let _ = exp.run(
                    |c| c.get_string("flag"),
                    [
                        ExperimentCase::equals("treatment", "treatment", || ()),
                        ExperimentCase::otherwise("control", || ()),
                    ],
                    RunOptions::new().exposure(exposure.clone()),
                );
            }
            let events: Vec<_> = logger
                .entries()
                .into_iter()
                .filter_map(|e| match e {
                    LogEntry::Event(ev) => Some(ev),
                    _ => None,
                })
                .collect();
            assert_eq!(events.len(), 1);
            assert_eq!(events[0].name(), ExperimentLoggerEvent::EXPOSURE);
            assert!(events[0].params().contains(&(
                ExperimentLoggerEvent::USER_PROPERTY_KEY.into(),
                LogValue::String("shinogi_ab_test".into()),
            )));
            assert!(events[0].params().contains(&(
                ExperimentLoggerEvent::VALUE_KEY.into(),
                LogValue::String("F-TEST-01:treatment".into()),
            )));

            // Overridden key -> suppressed
            let override_exposure = ExperimentExposure::new("shinogi_ab_ov", "F-TEST-02");
            let _ = exp.run(
                |c| c.get_string("overridden"),
                [ExperimentCase::equals("treatment", "treatment", || ())],
                RunOptions::new().exposure(override_exposure),
            );
            assert_eq!(logger.entries().len(), 1);

            // Fixed -> suppressed
            let fixed_exposure = ExperimentExposure::new("shinogi_ab_fx", "F-TEST-03");
            let _ = exp.run(
                |c| c.get_string("flag"),
                [ExperimentCase::otherwise("control", || ())],
                RunOptions::new().fixed("control").exposure(fixed_exposure),
            );
            assert_eq!(logger.entries().len(), 1);
        });
    }

    #[test]
    fn reactive_computed_updates_on_apply_values() {
        with_runtime(|| {
            let exp =
                Experiment::new(ExperimentMasamuneAdapter::test().default_value("flag", "control"));
            let variant = computed(move || {
                exp.run(
                    |c| c.get_string("flag"),
                    [
                        ExperimentCase::equals("treatment", "treatment", || "T"),
                        ExperimentCase::otherwise("control", || "C"),
                    ],
                    RunOptions::default(),
                )
                .unwrap_or("C")
            });
            assert_eq!(variant.get(), "C");
            exp.apply_values([("flag", "treatment")]);
            whisker::flush();
            assert_eq!(variant.get(), "T");
        });
    }
}
