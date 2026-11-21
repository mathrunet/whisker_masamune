#![doc = include_str!("../README.md")]

use masamune_whisker_logger::{LevelFilter, LogEvent, LogValue, LoggerAdapter};
use std::error::Error;
use whisker::RuntimeDispatcher;
use whisker_firebase::analytics::{Analytics, Param};
use whisker_firebase::crashlytics::{Crashlytics, ErrorReport};

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
        let params: Vec<(String, Param)> = event
            .params()
            .iter()
            .map(|(key, value)| (key.clone(), param(value)))
            .collect();
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
}
