//! Logger adapter and panic hook (`lib/src/ai_debug_logger_adapter.dart` and the adapter's
//! `FlutterError.onError` / `PlatformDispatcher.onError` hooks).
//!
//! Both may run on any thread, so they only forward reports to the controller's UI-thread queue.

use std::backtrace::Backtrace;
use std::error::Error;

use chrono::Utc;
use futures_channel::mpsc::UnboundedSender;
use masamune_whisker_logger::{LevelFilter, LogEvent, LogValue, LoggerAdapter};
use serde_json::{Map, Value, json};
use tokio::time::Instant;

use crate::controller::{AiDebugController, LOG_TARGET, Message};

/// Sends logger events, handled errors and traces to the AI debugger.
///
/// Register it with `LoggerConfig::adapter`. It forwards:
///
/// - `send` / `send_event` and log records (Info and above) as breadcrumbs,
/// - `report_error` as exception incidents (or `error` breadcrumbs when
///   [`crate::AiDebuggerConfig::report_handled_errors`] is false),
/// - `start_trace` as slow-operation checks for `masamune.model.load|…` and
///   `masamune.indicator.show|…` traces.
///
/// In prod it drops everything.
pub struct AiDebuggerLoggerAdapter {
    sender: Option<UnboundedSender<Message>>,
}

impl AiDebuggerLoggerAdapter {
    fn forward(&self, message: Message) {
        if let Some(sender) = &self.sender {
            let _ = sender.unbounded_send(message);
        }
    }
}

impl AiDebugController {
    /// The logger adapter of this debugger.
    pub fn logger_adapter(&self) -> AiDebuggerLoggerAdapter {
        AiDebuggerLoggerAdapter {
            sender: self.sender(),
        }
    }

    /// Reports panics as exception incidents, after calling the hook installed before it
    /// (install it after Crashlytics' `record_panics` so both run). Does nothing in prod.
    pub fn install_panic_hook(&self) {
        let Some(sender) = self.sender() else {
            return;
        };
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            previous(info);
            let payload = info
                .payload()
                .downcast_ref::<&str>()
                .map(|s| (*s).to_owned())
                .or_else(|| info.payload().downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "panic".to_owned());
            let message = match info.location() {
                Some(location) => format!("{payload} at {location}"),
                None => payload,
            };
            let _ = sender.unbounded_send(Message::Panic {
                message,
                stack: Backtrace::force_capture().to_string(),
            });
        }));
    }
}

impl LoggerAdapter for AiDebuggerLoggerAdapter {
    fn level(&self) -> LevelFilter {
        LevelFilter::Info
    }

    fn record(&self, record: &log::Record<'_>) {
        if record.target().starts_with(LOG_TARGET) {
            return;
        }
        let severity = match record.level() {
            log::Level::Error => "error",
            log::Level::Warn => "warning",
            _ => "info",
        };
        self.forward(Message::Log {
            name: "log".into(),
            params: json!({
                "level": record.level().as_str(),
                "target": record.target(),
                "message": record.args().to_string(),
            }),
            severity,
        });
    }

    fn send(&self, event: &LogEvent) {
        let params: Map<String, Value> = event
            .params()
            .iter()
            .map(|(key, value)| (key.clone(), log_value(value)))
            .collect();
        self.forward(Message::Log {
            name: event.name().to_owned(),
            params: Value::Object(params),
            severity: "info",
        });
    }

    fn error(&self, error: &(dyn Error + 'static), context: Option<&str>) {
        let message = match context {
            Some(context) => format!("{context}: {error}"),
            None => error.to_string(),
        };
        self.forward(Message::HandledError {
            message,
            stack: Backtrace::force_capture().to_string(),
        });
    }

    fn trace_start(&self, id: u64, name: &str) {
        self.forward(Message::TraceStart {
            id,
            name: name.to_owned(),
            wall: Utc::now(),
            at: Instant::now(),
        });
    }

    fn trace_stop(&self, id: u64, name: &str) {
        self.forward(Message::TraceStop {
            id,
            name: name.to_owned(),
            at: Instant::now(),
        });
    }
}

fn log_value(value: &LogValue) -> Value {
    match value {
        LogValue::String(s) => s.clone().into(),
        LogValue::Int(i) => (*i).into(),
        LogValue::Float(f) => json!(f),
        LogValue::Bool(b) => (*b).into(),
    }
}
