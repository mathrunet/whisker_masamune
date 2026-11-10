use masamune_whisker_logger::{
    Flavor, Level, LevelFilter, LogEntry, LogEvent, Loggable, LoggerAdapter, LoggerConfig,
    RuntimeLoggerAdapter, debug, info, report_error, send, send_event, start_trace, warn,
};

#[derive(Debug)]
struct SyncError;

impl std::fmt::Display for SyncError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("network unavailable")
    }
}

impl std::error::Error for SyncError {}

struct Purchase {
    value: i64,
}

impl Loggable for Purchase {
    fn to_event(&self) -> LogEvent {
        LogEvent::new("purchase")
            .param("currency", "JPY")
            .param("value", self.value)
    }
}

struct WarnOnly(RuntimeLoggerAdapter);

impl LoggerAdapter for WarnOnly {
    fn level(&self) -> LevelFilter {
        LevelFilter::Warn
    }
    fn record(&self, record: &log::Record<'_>) {
        self.0.record(record);
    }
}

/// Logs from inside an adapter must be dropped instead of recursing.
struct Reentrant(RuntimeLoggerAdapter);

impl LoggerAdapter for Reentrant {
    fn record(&self, record: &log::Record<'_>) {
        warn!("nested");
        self.0.record(record);
    }
    fn send(&self, event: &LogEvent) {
        send_event("nested", [("k", 1)]);
        self.0.send(event);
    }
}

fn messages(adapter: &RuntimeLoggerAdapter) -> Vec<(Level, String)> {
    adapter
        .entries()
        .into_iter()
        .filter_map(|entry| match entry {
            LogEntry::Record { level, message, .. } => Some((level, message)),
            _ => None,
        })
        .collect()
}

// One test owns the process-wide logger; integration test files run in separate processes.
#[test]
fn adapters_receive_logs() {
    let all = RuntimeLoggerAdapter::default();
    let narrow = RuntimeLoggerAdapter::default();
    LoggerConfig::new(Flavor::Dev)
        .adapter(all.clone())
        .adapter(WarnOnly(narrow.clone()))
        .init()
        .unwrap();
    debug!("dev detail");
    warn!("code=network_unavailable");
    send(&Purchase { value: 1200 });
    send_event("tutorial_begin", Vec::<(String, i64)>::new());
    report_error(&SyncError, Some("sync_failed"));

    assert_eq!(
        messages(&all),
        [
            (Level::Debug, "dev detail".into()),
            (Level::Warn, "code=network_unavailable".into())
        ]
    );
    assert_eq!(
        messages(&narrow),
        [(Level::Warn, "code=network_unavailable".into())]
    );
    let entries = all.entries();
    assert_eq!(
        entries[2],
        LogEntry::Event(
            LogEvent::new("purchase")
                .param("currency", "JPY")
                .param("value", 1200)
        )
    );
    assert_eq!(entries[3], LogEntry::Event(LogEvent::new("tutorial_begin")));
    assert_eq!(
        entries[4],
        LogEntry::Error {
            message: "network unavailable".into(),
            context: Some("sync_failed".into())
        }
    );
    // Adapters that only record logs keep the default no-op event/error handling.
    assert_eq!(narrow.entries().len(), 1);

    // Prod: records below Warn are dropped, events and errors are still delivered.
    let prod = RuntimeLoggerAdapter::default();
    LoggerConfig::new(Flavor::Prod)
        .adapter(prod.clone())
        .init()
        .unwrap();
    debug!("dev detail");
    info!("dev progress");
    warn!("code=network_unavailable");
    send_event("login", [("method", "google")]);
    report_error(&SyncError, None);
    assert_eq!(
        prod.entries(),
        [
            LogEntry::Record {
                level: Level::Warn,
                target: module_path!().into(),
                message: "code=network_unavailable".into()
            },
            LogEntry::Event(LogEvent::new("login").param("method", "google")),
            LogEntry::Error {
                message: "network unavailable".into(),
                context: None
            },
        ]
    );
    // Reinitializing replaced the adapters.
    assert_eq!(all.entries().len(), 5);

    // Off silences records only.
    let off = RuntimeLoggerAdapter::default();
    LoggerConfig::new(Flavor::Dev)
        .level(LevelFilter::Off)
        .adapter(off.clone())
        .init()
        .unwrap();
    warn!("hidden");
    send_event("login", [("method", "google")]);
    assert_eq!(off.entries().len(), 1);

    let reentrant = RuntimeLoggerAdapter::default();
    LoggerConfig::new(Flavor::Dev)
        .adapter(Reentrant(reentrant.clone()))
        .init()
        .unwrap();
    warn!("outer");
    send_event("outer", [("k", 1)]);
    assert_eq!(
        reentrant.entries(),
        [
            LogEntry::Record {
                level: Level::Warn,
                target: module_path!().into(),
                message: "outer".into()
            },
            LogEntry::Event(LogEvent::new("outer").param("k", 1)),
        ]
    );

    // Traces reach adapters in every flavor and stop when the guard drops.
    let traces = RuntimeLoggerAdapter::default();
    LoggerConfig::new(Flavor::Prod)
        .adapter(traces.clone())
        .init()
        .unwrap();
    let guard = start_trace("masamune.model.load|document|load|users");
    let id = guard.id();
    assert_eq!(guard.name(), "masamune.model.load|document|load|users");
    drop(guard);
    let name = "masamune.model.load|document|load|users".to_owned();
    assert_eq!(
        traces.entries(),
        [
            LogEntry::TraceStart {
                id,
                name: name.clone()
            },
            LogEntry::TraceStop { id, name },
        ]
    );
    assert_ne!(start_trace("a").id(), id);
}

#[test]
fn runtime_adapter_limit() {
    let adapter = RuntimeLoggerAdapter::new(2);
    for name in ["a", "b", "c"] {
        adapter.send(&LogEvent::new(name));
    }
    assert_eq!(
        adapter.entries(),
        [
            LogEntry::Event(LogEvent::new("b")),
            LogEntry::Event(LogEvent::new("c"))
        ]
    );
    adapter.clear();
    assert!(adapter.entries().is_empty());
}

#[test]
fn event_display() {
    let event = LogEvent::new("purchase")
        .param("currency", "JPY")
        .param("value", 1200)
        .param("first", true);
    assert_eq!(
        event.to_string(),
        r#"purchase {currency: "JPY", value: 1200, first: true}"#
    );
    assert_eq!(LogEvent::new("login").to_string(), "login");
}
