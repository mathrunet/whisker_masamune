# masamune-whisker-logger

Flavor-aware application logs using the standard `log` facade, sent to every
registered adapter like `katana_logger`'s `LoggerAdapter`. Initialize in startup
preparation, before other application work; libraries only emit records.

```rust
use masamune_whisker_logger::{Flavor, debug, info, warn};

fn prepare() -> Result<(), Box<dyn std::error::Error>> {
    let flavor = Flavor::from_env(option_env!("MY_APP_FLAVOR"))?;
    masamune_whisker_logger::init(flavor)?;
    debug!("cache initialized");
    info!("startup completed");
    warn!("sync unavailable: code=network_unavailable");
    Ok(())
}
```

Unset flavor means dev; only `dev` and `prod` are accepted. A typo returns an error
without including its value. Do not ignore initialization errors or fall back to dev.
The app must set its own `<APP>_FLAVOR=prod` at compile time for production, regardless
of Cargo's debug/release profile; the dependency cannot read the app's `option_env!`.

| Flavor | Default output |
|---|---|
| Dev | Error, Warn, Info, Debug |
| Prod | Error, Warn |

Trace is opt-in. `init_with_level(LevelFilter::Off)` disables all records;
`init_with_level(LevelFilter::Trace)` enables all levels, including development
records regardless of flavor. Do not enable development levels in production.
Disabled macro arguments are not evaluated. Filtering suppresses output, but does
not promise to remove string literals from the binary.

## Adapters

`init(flavor)` installs console output only. Register several outputs with
`LoggerConfig`; one call then reaches all of them:

```rust
use masamune_whisker_logger::{
    ConsoleLoggerAdapter, Flavor, LogEvent, Loggable, LoggerConfig, report_error, send,
    send_event, warn,
};

struct Purchase {
    value: i64,
}

impl Loggable for Purchase {
    fn to_event(&self) -> LogEvent {
        LogEvent::new("purchase").param("currency", "JPY").param("value", self.value)
    }
}

fn prepare() -> Result<(), Box<dyn std::error::Error>> {
    LoggerConfig::new(Flavor::from_env(option_env!("MY_APP_FLAVOR"))?)
        .adapter(ConsoleLoggerAdapter::new())
        // .adapter(masamune_whisker_logger_firebase::FirebaseLoggerAdapter::new()?)
        .init()?;
    warn!("sync unavailable: code=network_unavailable");
    send(&Purchase { value: 1200 });
    send_event("tutorial_begin", [("step", 1)]);
    report_error(&std::fmt::Error, Some("export_failed"));
    Ok(())
}
```

| Call | Delivered to adapters |
|---|---|
| Log macros | `record`, when the level passes the flavor's level and the adapter's `level()` |
| `send` / `send_event` | `send`, in every flavor (analytics) |
| `report_error` | `error`, in every flavor (crash reporting) |
| `start_trace` | `trace_start`, then `trace_stop` when the guard drops, in every flavor |

`LoggerConfig::level(LevelFilter::Off)` silences records but not events or errors.
`ConsoleLoggerAdapter` prints events only while Debug is enabled (dev).
`RuntimeLoggerAdapter` keeps the latest entries in memory for tests; clones share
them. Implement `LoggerAdapter` (`Send + Sync`) for other outputs; logging from
inside an adapter is dropped instead of recursing. `masamune-whisker-logger-firebase`
provides Firebase Analytics and Crashlytics.

Repeated initialization replaces the level and adapters without panicking. A different
global logger returns `InitError` without changing its level. Configuration is
process-wide; initialize before starting concurrent application work.

Native output uses stderr, which Whisker's development runtime captures and forwards
to `whisker run`. Android also writes to logcat with the `masamune-app` tag so production
warnings survive even when stderr is discarded. During development logcat can also
show Whisker's forwarded stderr copy. iOS/macOS use the host's stderr console. Web
uses the browser console at the record's level. These are `ConsoleLoggerAdapter`'s
outputs; remote outputs need their own adapter.

Use this package's macros for application logs. Existing libraries using `log::`
share the same backend and filter. `println!`, `eprintln!`, `dbg!`, native SDK logs,
framework diagnostics and panic hooks bypass this filter.

## Traces

`start_trace(name)` marks the start of a long operation and returns a guard; dropping the
guard stops it. Adapters receive both ends with the same id and measure durations
themselves (`masamune-whisker-ai-debugger` reports traces that exceed a threshold). The
default adapter methods ignore traces.

```rust
async fn load_profile() {
    let _trace = masamune_whisker_logger::start_trace("app.profile.load");
    // ... awaits ...
} // the trace stops here
```

`masamune-whisker-model` traces `CachedStore::load` / `query` as
`masamune.model.load|<document|collection>|<load|query>|<database>.<table>`, and
`masamune-whisker-ui` traces visible progress indicators as `masamune.indicator.show|<name>`.
Trace names must be fixed codes, never user data.

## Sensitive data

Never pass tokens, passwords, authorization headers, personal data, or full request/
response bodies to a log macro, event parameter or error context, even in dev. Events
and errors are sent in prod and remote adapters upload them. Filtering is not redaction. Debug/Trace
are for diagnostic state, Info for development progress; production Warn/Error must
contain only minimal operational information. Prefer fixed error codes and explicitly
allowed fields over raw `Display`/`Debug` of errors, URLs, models or payloads. Do not
raise a diagnostic message to Warn/Error to make it visible in prod.
