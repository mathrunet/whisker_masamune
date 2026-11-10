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
        if let Some(context) = context {
            let line = format!("context: {context}");
            self.on_ui(move || {
                let _ = crashlytics.log(&line);
            });
        }
        if whisker::runtime_dispatcher().is_some() {
            let _ = crashlytics.record_error(error);
            return;
        }
        // The UI thread's backtrace would be misleading, so off-thread reports carry none.
        let report = ErrorReport {
            name: "Error".into(),
            reason: reason(error),
            frames: Vec::new(),
        };
        self.on_ui(move || {
            let _ = crashlytics.record_report(&report);
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

fn reason(error: &(dyn Error + 'static)) -> String {
    let mut reason = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        reason.push_str(&format!("\nCaused by: {cause}"));
        source = cause.source();
    }
    reason
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct Outer(std::fmt::Error);

    impl std::fmt::Display for Outer {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("sync failed")
        }
    }

    impl Error for Outer {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            Some(&self.0)
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
    fn includes_source_chain() {
        assert_eq!(
            reason(&Outer(std::fmt::Error)),
            "sync failed\nCaused by: an error occurred when formatting an argument"
        );
    }
}
