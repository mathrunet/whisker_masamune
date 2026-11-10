use crate::{LogEvent, LoggerAdapter};
use log::{Level, Record};
use std::collections::VecDeque;
use std::error::Error;
use std::sync::{Arc, Mutex, MutexGuard};

/// One entry kept by [`RuntimeLoggerAdapter`].
#[derive(Clone, Debug, PartialEq)]
pub enum LogEntry {
    Record {
        level: Level,
        target: String,
        message: String,
    },
    Event(LogEvent),
    Error {
        message: String,
        context: Option<String>,
    },
    /// A trace from [`crate::start_trace`] began.
    TraceStart {
        id: u64,
        name: String,
    },
    /// The trace with the same id finished.
    TraceStop {
        id: u64,
        name: String,
    },
}

/// Keeps the latest entries in memory, mainly to assert logging in tests.
///
/// Clones share the same buffer, so keep one clone to read what was logged.
#[derive(Clone, Debug)]
pub struct RuntimeLoggerAdapter {
    entries: Arc<Mutex<VecDeque<LogEntry>>>,
    limit: usize,
}

impl Default for RuntimeLoggerAdapter {
    fn default() -> Self {
        Self::new(100)
    }
}

impl RuntimeLoggerAdapter {
    /// Keeps up to `limit` entries, dropping the oldest first.
    pub fn new(limit: usize) -> Self {
        Self {
            entries: Arc::default(),
            limit,
        }
    }

    pub fn entries(&self) -> Vec<LogEntry> {
        self.lock().iter().cloned().collect()
    }

    pub fn clear(&self) {
        self.lock().clear();
    }

    fn push(&self, entry: LogEntry) {
        let mut entries = self.lock();
        if self.limit == 0 {
            return;
        }
        if entries.len() == self.limit {
            entries.pop_front();
        }
        entries.push_back(entry);
    }

    fn lock(&self) -> MutexGuard<'_, VecDeque<LogEntry>> {
        self.entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl LoggerAdapter for RuntimeLoggerAdapter {
    fn record(&self, record: &Record<'_>) {
        self.push(LogEntry::Record {
            level: record.level(),
            target: record.target().to_owned(),
            message: record.args().to_string(),
        });
    }

    fn send(&self, event: &LogEvent) {
        self.push(LogEntry::Event(event.clone()));
    }

    fn error(&self, error: &(dyn Error + 'static), context: Option<&str>) {
        self.push(LogEntry::Error {
            message: error.to_string(),
            context: context.map(str::to_owned),
        });
    }

    fn trace_start(&self, id: u64, name: &str) {
        self.push(LogEntry::TraceStart {
            id,
            name: name.to_owned(),
        });
    }

    fn trace_stop(&self, id: u64, name: &str) {
        self.push(LogEntry::TraceStop {
            id,
            name: name.to_owned(),
        });
    }
}
