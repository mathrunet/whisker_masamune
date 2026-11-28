#![doc = include_str!("../README.md")]

use masamune_whisker_logger::{LevelFilter, LogEvent, LogValue, LoggerAdapter};
use std::error::Error;
use whisker::RuntimeDispatcher;
use whisker_firebase::analytics::{Analytics, Consent, Param};
use whisker_firebase::crashlytics::{Crashlytics, ErrorReport, StackFrame};

pub use whisker_firebase::analytics::FirebaseError;

/// Sends events to Analytics, and warnings and handled errors to Crashlytics.
///
/// Create it in the app's startup preparation on the UI thread; calls from other
/// threads are posted to that runtime. Firebase failures are ignored because the
/// logging facade cannot return them.
pub struct FirebaseLoggerAdapter {
    analytics: Option<Analytics>,
    crashlytics: Option<Crashlytics>,
    breadcrumb_level: LevelFilter,
    dispatcher: Option<RuntimeDispatcher>,
}

impl FirebaseLoggerAdapter {
    /// Initializes the default Firebase app with both Analytics and Crashlytics.
    pub fn new() -> Result<Self, FirebaseError> {
        Ok(Self {
            analytics: Some(Analytics::instance()?),
            crashlytics: Some(Crashlytics::instance()?),
            breadcrumb_level: LevelFilter::Warn,
            dispatcher: whisker::runtime_dispatcher(),
        })
    }

    pub fn without_analytics(mut self) -> Self {
        self.analytics = None;
        self
    }

    pub fn without_crashlytics(mut self) -> Self {
        self.crashlytics = None;
        self
    }

    /// Records sent as Crashlytics breadcrumbs; still limited by the flavor's level.
    pub fn breadcrumb_level(mut self, level: LevelFilter) -> Self {
        self.breadcrumb_level = level;
        self
    }

    /// Reports Rust panics as Crashlytics crashes with their message and backtrace.
    pub fn record_panics(self) -> Self {
        if let Some(crashlytics) = self.crashlytics {
            crashlytics.record_panics();
        }
        self
    }

    // Native modules are bound only on the runtime's UI thread.
    fn on_ui(&self, work: impl FnOnce() + Send + 'static) {
        if whisker::runtime_dispatcher().is_some() {
            work();
        } else if let Some(dispatcher) = &self.dispatcher {
            dispatcher.post(work);
        }
    }
}

impl LoggerAdapter for FirebaseLoggerAdapter {
    fn level(&self) -> LevelFilter {
        if self.crashlytics.is_some() {
            self.breadcrumb_level
        } else {
            LevelFilter::Off
        }
    }

    fn record(&self, record: &log::Record<'_>) {
        let Some(crashlytics) = self.crashlytics else {
            return;
        };
        let line = format!(
            "[{}] {}: {}",
            record.level(),
            record.target(),
            record.args()
        );
        self.on_ui(move || {
            let _ = crashlytics.log(&line);
        });
    }

    fn send(&self, event: &LogEvent) {
        let Some(analytics) = self.analytics else {
            return;
        };
        let name = event.name().to_owned();
        let params = event_params(event);
        self.on_ui(move || {
            let _ = if name == "screen_view" {
                screen_view(analytics, &params)
            } else {
                analytics.log_event(&name, params)
            };
        });
    }

    fn error(&self, error: &(dyn Error + 'static), context: Option<&str>) {
        let Some(crashlytics) = self.crashlytics else {
            return;
        };
        let safe_error = HandledError::for_reporting(error, context);
        if whisker::runtime_dispatcher().is_some() {
            let _ = crashlytics.record_error(&safe_error);
            return;
        }
        // The UI thread's backtrace would be misleading, so off-thread reports carry none.
        self.on_ui(move || {
            let _ = crashlytics.record_report(&safe_error.report());
        });
    }
}

/// Firebase Analytics for apps that own their consent and event allowlist.
///
/// Call on the active Whisker runtime. Event calls are forwarded immediately
/// so a later consent change cannot leave any work pending on a Rust dispatcher.
pub struct FirebaseAnalyticsAdapter(Analytics);

impl FirebaseAnalyticsAdapter {
    pub fn new() -> Result<Self, FirebaseError> {
        Ok(Self(Analytics::instance()?))
    }

    /// Enables Analytics storage and collection with all advertising consent denied.
    pub fn enable_without_advertising(&self) -> Result<(), FirebaseError> {
        self.0.set_consent(analytics_consent(true))?;
        self.0.set_collection_enabled(true)
    }

    /// Stops collection and deletes the local Analytics data and app instance ID.
    /// Each SDK operation is attempted even when an earlier operation fails.
    pub fn stop_and_reset(&self) -> Result<(), FirebaseError> {
        let stopped = self.0.set_collection_enabled(false);
        let denied = self.0.set_consent(analytics_consent(false));
        let reset = self.0.reset_analytics_data();
        stopped.and(denied).and(reset)
    }

    /// Submits one app-approved event to the SDK on the current runtime.
    pub fn send(&self, event: &LogEvent) -> Result<(), FirebaseError> {
        self.0.log_event(event.name(), event_params(event))
    }
}

/// A vetted code location attached to a [`DiagnosticReport`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticFrame {
    pub symbol: String,
    pub file: Option<String>,
    pub line: Option<u32>,
}

/// An allowlisted handled-error diagnostic payload.
///
/// Only a fixed classification code and vetted code frames are serialized;
/// raw error strings, messages, user IDs, custom keys, and breadcrumbs are omitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticReport {
    classification: String,
    frames: Vec<DiagnosticFrame>,
}

impl DiagnosticReport {
    /// Builds a sanitized diagnostic report from a fixed classification code
    /// (`[a-z][a-z0-9_]{0,63}`; invalid inputs fall back to `"handled_error"`)
    /// and vetted stack frames.
    pub fn new(classification: &str, frames: Vec<DiagnosticFrame>) -> Self {
        let classification = if is_valid_classification(classification) {
            classification.to_owned()
        } else {
            "handled_error".to_owned()
        };
        let frames = frames
            .into_iter()
            .filter_map(sanitize_frame)
            .take(32)
            .collect();
        Self {
            classification,
            frames,
        }
    }

    pub fn classification(&self) -> &str {
        &self.classification
    }

    pub fn frames(&self) -> &[DiagnosticFrame] {
        &self.frames
    }

    fn to_error_report(&self) -> ErrorReport {
        ErrorReport {
            name: "HandledError".into(),
            reason: self.classification.clone(),
            frames: self
                .frames
                .iter()
                .map(|frame| StackFrame {
                    symbol: frame.symbol.clone(),
                    file: frame.file.clone(),
                    line: frame.line,
                })
                .collect(),
        }
    }
}

fn is_valid_classification(value: &str) -> bool {
    let mut bytes = value.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    value.len() <= 64
        && first.is_ascii_lowercase()
        && bytes.all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn is_valid_symbol(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b':'))
}

fn is_valid_relative_file(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && !value.starts_with('/')
        && !value.contains("..")
        && !value.contains("://")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b'/'))
}

fn sanitize_frame(frame: DiagnosticFrame) -> Option<DiagnosticFrame> {
    if !is_valid_symbol(&frame.symbol) {
        return None;
    }
    let file = frame.file.filter(|path| is_valid_relative_file(path));
    let line = frame.line.filter(|line| *line > 0);
    Some(DiagnosticFrame {
        symbol: frame.symbol,
        file,
        line,
    })
}

/// Firebase Crashlytics for apps that own manual unsent-report dispatch and deletion gating.
///
/// Initializes only Crashlytics (not Analytics), keeps automatic collection disabled,
/// and never attaches breadcrumbs, custom user IDs, or custom keys.
pub struct FirebaseCrashlyticsAdapter(Crashlytics);

impl FirebaseCrashlyticsAdapter {
    /// Initializes Crashlytics only and ensures automatic collection is disabled.
    pub fn new() -> Result<Self, FirebaseError> {
        let crashlytics = Crashlytics::instance()?;
        crashlytics.set_collection_enabled(false)?;
        Ok(Self(crashlytics))
    }

    /// Ensures automatic Crashlytics collection remains disabled.
    pub fn disable_automatic_collection(&self) -> Result<(), FirebaseError> {
        self.0.set_collection_enabled(false)
    }

    /// Returns whether Crashlytics automatic collection is currently enabled.
    pub fn is_collection_enabled(&self) -> Result<bool, FirebaseError> {
        self.0.is_collection_enabled()
    }

    /// Records a sanitized handled-error report locally without enabling automatic sending.
    pub fn record(&self, report: &DiagnosticReport) -> Result<(), FirebaseError> {
        self.0.record_report(&report.to_error_report())
    }

    /// Checks whether unsent reports are waiting on the device.
    pub async fn check_for_unsent_reports(&self) -> Result<bool, FirebaseError> {
        self.0.check_for_unsent_reports().await
    }

    /// Sends queued unsent reports once without enabling automatic collection.
    pub fn send_unsent_reports(&self) -> Result<(), FirebaseError> {
        self.0.send_unsent_reports()
    }

    /// Deletes all queued unsent reports on the device without sending them.
    pub fn delete_unsent_reports(&self) -> Result<(), FirebaseError> {
        self.0.delete_unsent_reports()
    }
}

fn analytics_consent(allowed: bool) -> Consent {
    Consent::new()
        .analytics_storage(allowed)
        .ad_storage(false)
        .ad_user_data(false)
        .ad_personalization(false)
}

/// Native startup helpers for CNG projects that restore consent after launch.
pub mod analytics_startup {
    /// Swift helper. Call `FirebaseAnalyticsStartup.close()` before configuring Firebase.
    pub const IOS_SOURCE: &str = include_str!("startup/FirebaseAnalyticsStartup.swift");

    /// Register `rs.masamune.logger.firebase.AnalyticsStartupProvider` with
    /// `android:exported="false"` and an init order higher than FirebaseInitProvider.
    pub const ANDROID_SOURCE: &str = include_str!("startup/AnalyticsStartupProvider.java");
}

/// Sets (or, with `None`, clears) the Analytics user ID attached to later events, like
/// `FirebaseAnalytics.setUserId`.
///
/// Call it on the UI thread after Firebase is initialized (for example after creating the
/// [`FirebaseLoggerAdapter`]). Pass an opaque, non-personal ID (such as the auth uid), never an
/// email address or a name.
pub fn set_user_id(id: Option<&str>) -> Result<(), FirebaseError> {
    Analytics::instance()?.set_user_id(id)
}

fn event_params(event: &LogEvent) -> Vec<(String, Param)> {
    event
        .params()
        .iter()
        .map(|(key, value)| (key.clone(), param(value)))
        .collect()
}

fn param(value: &LogValue) -> Param {
    match value {
        LogValue::String(value) => Param::String(value.clone()),
        LogValue::Int(value) => Param::Integer(*value),
        LogValue::Float(value) => Param::Double(*value),
        LogValue::Bool(value) => Param::from(*value),
    }
}

fn screen_view(analytics: Analytics, params: &[(String, Param)]) -> Result<(), FirebaseError> {
    let text = |key: &str| {
        params.iter().find_map(|(name, value)| match value {
            Param::String(value) if name == key => Some(value.as_str()),
            _ => None,
        })
    };
    match text("screen_name") {
        Some(screen) => analytics.log_screen_view(screen, text("screen_class")),
        None => analytics.log_event("screen_view", params.to_vec()),
    }
}

#[derive(Debug)]
struct HandledError;

impl HandledError {
    // Both paths accept only the fixed classification. The original error and context can
    // contain request URLs, server payloads, or other unstructured application data.
    fn for_reporting(_error: &dyn Error, _context: Option<&str>) -> Self {
        Self
    }

    fn report(&self) -> ErrorReport {
        ErrorReport {
            name: "HandledError".into(),
            reason: self.to_string(),
            frames: Vec::new(),
        }
    }
}

impl std::fmt::Display for HandledError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("handled_error")
    }
}

impl Error for HandledError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advertising_remains_denied_for_both_consent_states() {
        for allowed in [false, true] {
            let consent = analytics_consent(allowed);
            assert_eq!(consent.analytics_storage, Some(allowed));
            assert_eq!(consent.ad_storage, Some(false));
            assert_eq!(consent.ad_user_data, Some(false));
            assert_eq!(consent.ad_personalization, Some(false));
        }
    }

    #[test]
    fn converts_values() {
        assert_eq!(param(&"a".into()), Param::String("a".into()));
        assert_eq!(param(&3.into()), Param::Integer(3));
        assert_eq!(param(&1.5.into()), Param::Double(1.5));
        assert_eq!(param(&true.into()), Param::Integer(1));
    }

    #[test]
    fn handled_error_uses_the_same_fixed_reason_on_both_paths() {
        #[derive(Debug)]
        struct WrappedError(std::io::Error);

        impl std::fmt::Display for WrappedError {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("dummy HTTP response body")
            }
        }

        impl Error for WrappedError {
            fn source(&self) -> Option<&(dyn Error + 'static)> {
                Some(&self.0)
            }
        }

        let error = WrappedError(std::io::Error::other("dummy private URL"));
        let safe_error = HandledError::for_reporting(&error, Some("dummy private context"));
        let report = safe_error.report();
        assert_eq!(safe_error.to_string(), "handled_error");
        assert!(safe_error.source().is_none());
        assert_eq!(report.name, "HandledError");
        assert_eq!(report.reason, safe_error.to_string());
        assert!(report.frames.is_empty());
    }

    #[test]
    fn diagnostic_report_sanitizes_classification_and_frames() {
        let report = DiagnosticReport::new(
            "storage_error",
            vec![
                DiagnosticFrame {
                    symbol: "mizuhiki::pages::delete::execute".into(),
                    file: Some("src/pages/delete.rs".into()),
                    line: Some(616),
                },
                DiagnosticFrame {
                    symbol: "bad symbol with secret token=abc".into(),
                    file: Some("src/pages/delete.rs".into()),
                    line: Some(10),
                },
                DiagnosticFrame {
                    symbol: "mizuhiki::boot::restore".into(),
                    file: Some("/Users/private/secret.rs".into()),
                    line: Some(0),
                },
            ],
        );
        assert_eq!(report.classification(), "storage_error");
        assert_eq!(report.frames().len(), 2);
        assert_eq!(
            report.frames()[0].symbol,
            "mizuhiki::pages::delete::execute"
        );
        assert_eq!(
            report.frames()[0].file.as_deref(),
            Some("src/pages/delete.rs")
        );
        assert_eq!(report.frames()[0].line, Some(616));
        assert_eq!(report.frames()[1].symbol, "mizuhiki::boot::restore");
        assert_eq!(report.frames()[1].file, None);
        assert_eq!(report.frames()[1].line, None);

        let error_report = report.to_error_report();
        assert_eq!(error_report.name, "HandledError");
        assert_eq!(error_report.reason, "storage_error");
        assert_eq!(error_report.frames.len(), 2);

        let fallback = DiagnosticReport::new("https://secret.example/token?a=1", Vec::new());
        assert_eq!(fallback.classification(), "handled_error");
    }
}
