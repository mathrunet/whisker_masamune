use std::cell::RefCell;
use std::collections::{BTreeMap, HashSet};

use crate::ExperimentValue;

const TRUE_VALUES: [&str; 6] = ["1", "true", "t", "yes", "y", "on"];

/// Values passed to the first callback of [`crate::Experiment::run`].
///
/// Matches Firebase Remote Config's getters and conversion rules. Undefined keys return
/// `false`, `0`, `0.0`, and `""`.
#[derive(Clone, Debug)]
pub struct ExperimentConfig {
    values: BTreeMap<String, ExperimentValue>,
    accessed_keys: Option<RefCell<HashSet<String>>>,
}

impl ExperimentConfig {
    /// Creates a config view over `values`.
    pub fn new(values: BTreeMap<String, ExperimentValue>) -> Self {
        Self {
            values,
            accessed_keys: None,
        }
    }

    /// Creates a config view that tracks which keys were read.
    pub(crate) fn tracking(values: BTreeMap<String, ExperimentValue>) -> Self {
        Self {
            values,
            accessed_keys: Some(RefCell::new(HashSet::new())),
        }
    }

    pub(crate) fn into_accessed_keys(self) -> HashSet<String> {
        self.accessed_keys
            .map(|keys| keys.into_inner())
            .unwrap_or_default()
    }

    fn record_key(&self, key: &str) {
        if let Some(accessed) = &self.accessed_keys {
            accessed.borrow_mut().insert(key.to_owned());
        }
    }

    /// Returns `true` if `key` exists.
    pub fn contains_key(&self, key: &str) -> bool {
        self.record_key(key);
        self.values.contains_key(key)
    }

    /// Returns all values.
    pub fn get_all(&self) -> BTreeMap<String, ExperimentValue> {
        if let Some(accessed) = &self.accessed_keys {
            let mut set = accessed.borrow_mut();
            for key in self.values.keys() {
                set.insert(key.clone());
            }
        }
        self.values.clone()
    }

    /// Gets the value of `key` as a `String`, or `""` if missing.
    pub fn get_string(&self, key: &str) -> String {
        self.record_key(key);
        self.values
            .get(key)
            .map(ExperimentValue::as_str_value)
            .unwrap_or_default()
    }

    /// Gets the value of `key` as a `bool`.
    ///
    /// Returns `true` only if the value is `true` or one of `1`, `true`, `t`, `yes`, `y`, `on`
    /// (case-insensitive).
    pub fn get_bool(&self, key: &str) -> bool {
        self.record_key(key);
        match self.values.get(key) {
            Some(ExperimentValue::Bool(value)) => *value,
            Some(other) => {
                let lowered = other.as_str_value().to_ascii_lowercase();
                TRUE_VALUES.contains(&lowered.as_str())
            }
            None => false,
        }
    }

    /// Gets the value of `key` as an `i64`, or `0` if missing or not parseable.
    pub fn get_int(&self, key: &str) -> i64 {
        self.record_key(key);
        match self.values.get(key) {
            Some(ExperimentValue::Int(value)) => *value,
            Some(other) => other.as_str_value().parse::<i64>().unwrap_or(0),
            None => 0,
        }
    }

    /// Gets the value of `key` as an `f64`, or `0.0` if missing or not parseable.
    pub fn get_float(&self, key: &str) -> f64 {
        self.record_key(key);
        match self.values.get(key) {
            Some(ExperimentValue::Float(value)) => *value,
            Some(ExperimentValue::Int(value)) => *value as f64,
            Some(other) => other.as_str_value().parse::<f64>().unwrap_or(0.0),
            None => 0.0,
        }
    }
}
