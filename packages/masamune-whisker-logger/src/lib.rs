#![doc = include_str!("../README.md")]

mod adapter;
mod console;
mod runtime;

pub use adapter::{LogEvent, LogValue, Loggable, LoggerAdapter};
pub use console::ConsoleLoggerAdapter;
pub use log::{Level, LevelFilter, debug, error, info, log, log_enabled, trace, warn};
pub use runtime::{LogEntry, RuntimeLoggerAdapter};

use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, RwLock};

/// Application environment, independent of Cargo's build profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flavor {
    Dev,
    Prod,
}

impl Flavor {
    /// Unset means prod; unknown values fail instead of enabling development logs.
    pub fn from_env(value: Option<&str>) -> Result<Self, InvalidFlavor> {
        match value {
            Some("dev") => Ok(Self::Dev),
            None | Some("prod") => Ok(Self::Prod),
            Some(_) => Err(InvalidFlavor),
        }
    }

    pub const fn level(self) -> LevelFilter {
        match self {
            Self::Dev => LevelFilter::Debug,
            Self::Prod => LevelFilter::Warn,
        }
    }
}

/// An unrecognized flavor value (the original value is intentionally omitted).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidFlavor;

impl std::fmt::Display for InvalidFlavor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("expected dev or prod flavor")
    }
}

impl std::error::Error for InvalidFlavor {}

/// Another logger already owns the global log facade.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InitError;

impl std::fmt::Display for InitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("another logger is already installed")
    }
}

impl std::error::Error for InitError {}

type Adapters = Arc<[Arc<dyn LoggerAdapter>]>;

/// The flavor's level plus the adapters that receive every record, event and error.
pub struct LoggerConfig {
    level: LevelFilter,
    adapters: Vec<Arc<dyn LoggerAdapter>>,
}

impl LoggerConfig {
    /// Starts without adapters; add [`ConsoleLoggerAdapter`] to keep console output.
    pub fn new(flavor: Flavor) -> Self {
        Self {
            level: flavor.level(),
            adapters: Vec::new(),
        }
    }

    /// Overrides the flavor's level; `Off` silences records but not events or errors.
    ///
    /// Raising the level above Warn enables development records even in production.
    pub fn level(mut self, level: LevelFilter) -> Self {
        self.level = level;
        self
    }

    pub fn adapter(mut self, adapter: impl LoggerAdapter) -> Self {
        self.adapters.push(Arc::new(adapter));
        self
    }

    /// Installs the logger or replaces the previous configuration; safe to repeat on remount.
    pub fn init(self) -> Result<(), InitError> {
        (*INSTALLED.get_or_init(|| log::set_logger(&LOGGER).map_err(|_| InitError)))?;
        *ADAPTERS.write().unwrap_or_else(|e| e.into_inner()) = Some(self.adapters.into());
        log::set_max_level(self.level);
        Ok(())
    }
}

/// Install console-only logging with the flavor's default level.
pub fn init(flavor: Flavor) -> Result<(), InitError> {
    init_with_level(flavor.level())
}

/// Install console-only logging at `level`, including `Off` to silence all records.
///
/// Raising the level above Warn enables development records even in production.
pub fn init_with_level(level: LevelFilter) -> Result<(), InitError> {
    LoggerConfig {
        level,
        adapters: vec![Arc::new(ConsoleLoggerAdapter)],
    }
    .init()
}

/// Send a typed event to every adapter, in every flavor.
pub fn send(loggable: &impl Loggable) {
    let event = loggable.to_event();
    dispatch(|adapter| adapter.send(&event));
}

/// Send an event built from a name and parameters, in every flavor.
pub fn send_event<K, V>(name: &str, params: impl IntoIterator<Item = (K, V)>)
where
    K: Into<String>,
    V: Into<LogValue>,
{
    let event = params
        .into_iter()
        .fold(LogEvent::new(name), |event, (key, value)| {
            event.param(key, value)
        });
    dispatch(|adapter| adapter.send(&event));
}

/// Report a handled error to every adapter, in every flavor.
///
/// `context` should be a fixed code such as `"sync_failed"`, not user data.
pub fn report_error(error: &(dyn std::error::Error + 'static), context: Option<&str>) {
    dispatch(|adapter| adapter.error(error, context));
}

/// Starts a named trace that ends when the returned guard is dropped.
///
/// Adapters receive [`LoggerAdapter::trace_start`] and [`LoggerAdapter::trace_stop`] in every
/// flavor. Use a fixed name made of codes (for example `masamune.model.load|document|load|users`),
/// never user data.
pub fn start_trace(name: impl Into<String>) -> TraceGuard {
    let guard = TraceGuard {
        id: NEXT_TRACE.fetch_add(1, Ordering::Relaxed),
        name: name.into(),
    };
    dispatch(|adapter| adapter.trace_start(guard.id, &guard.name));
    guard
}

/// A running trace from [`start_trace`]; dropping it stops the trace.
#[must_use = "the trace stops when the guard is dropped"]
#[derive(Debug)]
pub struct TraceGuard {
    id: u64,
    name: String,
}

impl TraceGuard {
    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

impl Drop for TraceGuard {
    fn drop(&mut self) {
        dispatch(|adapter| adapter.trace_stop(self.id, &self.name));
    }
}

static NEXT_TRACE: AtomicU64 = AtomicU64::new(1);

struct Logger;
static LOGGER: Logger = Logger;
static INSTALLED: OnceLock<Result<(), InitError>> = OnceLock::new();
static ADAPTERS: RwLock<Option<Adapters>> = RwLock::new(None);

thread_local! {
    static DISPATCHING: Cell<bool> = const { Cell::new(false) };
}

struct DispatchGuard;

impl Drop for DispatchGuard {
    fn drop(&mut self) {
        DISPATCHING.with(|flag| flag.set(false));
    }
}

fn dispatch(mut each: impl FnMut(&dyn LoggerAdapter)) {
    if DISPATCHING.with(|flag| flag.replace(true)) {
        return;
    }
    let _guard = DispatchGuard;
    // Clone the snapshot so adapters run without holding the lock.
    let adapters = ADAPTERS.read().unwrap_or_else(|e| e.into_inner()).clone();
    for adapter in adapters.iter().flat_map(|adapters| adapters.iter()) {
        each(adapter.as_ref());
    }
}

impl log::Log for Logger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            dispatch(|adapter| {
                if record.level() <= adapter.level() {
                    adapter.record(record);
                }
            });
        }
    }

    fn flush(&self) {
        dispatch(|adapter| adapter.flush());
    }
}
