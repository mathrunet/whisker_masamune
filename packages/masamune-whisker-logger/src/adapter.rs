use log::{LevelFilter, Record};
use std::error::Error;

/// One output of the application logger, registered with [`crate::LoggerConfig`].
///
/// Methods are called synchronously on the logging thread, so long work should be
/// queued. Logging from inside an adapter is dropped instead of recursing.
pub trait LoggerAdapter: Send + Sync + 'static {
    /// Further narrows the flavor's level for [`Self::record`]; it cannot widen it.
    fn level(&self) -> LevelFilter {
        LevelFilter::Trace
    }

    /// A log macro record that passed both the global and this adapter's level.
    fn record(&self, record: &Record<'_>);

    /// An analytics event, delivered in every flavor.
    fn send(&self, event: &LogEvent) {
        let _ = event;
    }

    /// A handled error, delivered in every flavor.
    fn error(&self, error: &(dyn Error + 'static), context: Option<&str>) {
        let _ = (error, context);
    }

    /// A [`crate::start_trace`] began; `id` pairs it with [`Self::trace_stop`].
    ///
    /// Adapters that watch durations (such as slow-operation detectors) measure the time
    /// themselves; the default ignores traces.
    fn trace_start(&self, id: u64, name: &str) {
        let _ = (id, name);
    }

    /// The trace started with the same `id` finished (its guard was dropped).
    fn trace_stop(&self, id: u64, name: &str) {
        let _ = (id, name);
    }

    fn flush(&self) {}
}

/// A value of an event parameter.
#[derive(Clone, Debug, PartialEq)]
pub enum LogValue {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
}

macro_rules! value_from {
    ($variant:ident: $($ty:ty),*) => {$(
        impl From<$ty> for LogValue {
            fn from(value: $ty) -> Self {
                Self::$variant(value.into())
            }
        }
    )*};
}
value_from!(String: String, &str, &String);
value_from!(Int: i8, i16, i32, i64, u8, u16, u32);
value_from!(Float: f32, f64);
value_from!(Bool: bool);

impl std::fmt::Display for LogValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::String(value) => write!(f, "{value:?}"),
            Self::Int(value) => write!(f, "{value}"),
            Self::Float(value) => write!(f, "{value}"),
            Self::Bool(value) => write!(f, "{value}"),
        }
    }
}

/// A named event with parameters, such as an analytics event.
#[derive(Clone, Debug, PartialEq)]
pub struct LogEvent {
    name: String,
    params: Vec<(String, LogValue)>,
}

impl LogEvent {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            params: Vec::new(),
        }
    }

    pub fn param(mut self, key: impl Into<String>, value: impl Into<LogValue>) -> Self {
        self.params.push((key.into(), value.into()));
        self
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn params(&self) -> &[(String, LogValue)] {
        &self.params
    }
}

impl std::fmt::Display for LogEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)?;
        for (index, (key, value)) in self.params.iter().enumerate() {
            let separator = if index == 0 { " {" } else { ", " };
            write!(f, "{separator}{key}: {value}")?;
        }
        if !self.params.is_empty() {
            f.write_str("}")?;
        }
        Ok(())
    }
}

/// A typed event; implement it for event structs and pass them to [`crate::send`].
pub trait Loggable {
    fn to_event(&self) -> LogEvent;
}

impl Loggable for LogEvent {
    fn to_event(&self) -> LogEvent {
        self.clone()
    }
}
