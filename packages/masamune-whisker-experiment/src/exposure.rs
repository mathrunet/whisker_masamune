use masamune_whisker_logger::{LogEvent, Loggable};

/// Settings for recording which branch of [`crate::Experiment::run`] the user was exposed to.
///
/// When passed to [`crate::Experiment::run`], an [`ExperimentLoggerEvent::EXPOSURE`] event is
/// emitted with `<experiment_id>:<label>` for the selected [`crate::ExperimentCase`].
/// `masamune-whisker-logger-firebase` stores it as the Firebase Analytics user property
/// `user_property`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ExperimentExposure {
    /// Name of the user property that stores the exposed branch (1–24 characters).
    pub user_property: String,
    /// ID of the experiment, used as the prefix of `<experiment_id>:<label>`.
    pub experiment_id: String,
}

impl ExperimentExposure {
    /// Maximum length of `user_property` (Firebase Analytics limit).
    pub const MAX_USER_PROPERTY_LENGTH: usize = 24;
    /// Maximum length of `<experiment_id>:<label>` (Firebase Analytics limit).
    pub const MAX_VALUE_LENGTH: usize = 36;

    pub fn new(user_property: impl Into<String>, experiment_id: impl Into<String>) -> Self {
        let user_property = user_property.into();
        let experiment_id = experiment_id.into();
        debug_assert!(
            !user_property.is_empty() && user_property.len() <= Self::MAX_USER_PROPERTY_LENGTH,
            "user_property must be 1-{} characters: {user_property}",
            Self::MAX_USER_PROPERTY_LENGTH
        );
        debug_assert!(!experiment_id.is_empty(), "experiment_id must not be empty");
        Self {
            user_property,
            experiment_id,
        }
    }

    /// Returns the user property value for `label`: `<experiment_id>:<label>`.
    pub fn value_of(&self, label: &str) -> String {
        format!("{}:{label}", self.experiment_id)
    }

    /// Returns `true` if `user_property` satisfies Firebase Analytics naming rules.
    pub fn is_valid_user_property(&self) -> bool {
        let prop = self.user_property.as_str();
        if prop.is_empty() || prop.len() > Self::MAX_USER_PROPERTY_LENGTH {
            return false;
        }
        let mut chars = prop.chars();
        let Some(first) = chars.next() else {
            return false;
        };
        if !first.is_ascii_alphabetic() {
            return false;
        }
        if !chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_') {
            return false;
        }
        !prop.starts_with("firebase_") && !prop.starts_with("google_") && !prop.starts_with("ga_")
    }
}

/// Event name and parameter keys emitted for experiment exposures.
///
/// Matches Flutter's `ExperimentLoggerEvent` so `masamune-whisker-logger-firebase` and log
/// processors handle exposures identically across Flutter and Whisker.
pub struct ExperimentLoggerEvent;

impl ExperimentLoggerEvent {
    /// Event name emitted when an experiment branch is exposed.
    pub const EXPOSURE: &'static str = "ExperimentLoggerEvent.exposure";
    /// Key for the user property name.
    pub const USER_PROPERTY_KEY: &'static str = "user_property";
    /// Key for the user property value (`<experiment_id>:<label>`).
    pub const VALUE_KEY: &'static str = "value";
    /// Key for the experiment ID.
    pub const EXPERIMENT_ID_KEY: &'static str = "experiment_id";
    /// Key for the selected branch label.
    pub const LABEL_KEY: &'static str = "label";

    /// Builds the [`LogEvent`] for an exposure.
    pub fn exposure_event(exposure: &ExperimentExposure, label: &str) -> LogEvent {
        LogEvent::new(Self::EXPOSURE)
            .param(Self::USER_PROPERTY_KEY, exposure.user_property.as_str())
            .param(Self::VALUE_KEY, exposure.value_of(label))
            .param(Self::EXPERIMENT_ID_KEY, exposure.experiment_id.as_str())
            .param(Self::LABEL_KEY, label)
    }
}

/// Typed loggable exposure event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExposureLog {
    pub(crate) exposure: ExperimentExposure,
    pub(crate) label: String,
}

impl Loggable for ExposureLog {
    fn to_event(&self) -> LogEvent {
        ExperimentLoggerEvent::exposure_event(&self.exposure, &self.label)
    }
}
