# masamune-whisker-logger-firebase

Firebase Analytics and Crashlytics as a
[`masamune-whisker-logger`](../masamune-whisker-logger/README.md) adapter, like
`FirebaseLoggerAdapter` in `masamune_logger_firebase`. Android and iOS only.

```rust,ignore
use masamune_whisker_logger::{ConsoleLoggerAdapter, Flavor, LoggerConfig};
use masamune_whisker_logger_firebase::FirebaseLoggerAdapter;

fn prepare() -> Result<(), Box<dyn std::error::Error>> {
    let flavor = Flavor::from_env(option_env!("MY_APP_FLAVOR"))?;
    LoggerConfig::new(flavor)
        .adapter(ConsoleLoggerAdapter::new())
        .adapter(FirebaseLoggerAdapter::new()?.record_panics())
        .init()?;
    Ok(())
}
```

| Logger call | Firebase |
|---|---|
| `warn!` / `error!` (`breadcrumb_level`, default Warn) | Crashlytics breadcrumb (`log`) |
| `send` / `send_event` | Analytics `log_event` |
| `send_event("screen_view", [("screen_name", ..)])` | Analytics `log_screen_view` (optional `screen_class`) |
| `report_error(err, context)` | Crashlytics non-fatal `record_error`; `context` becomes a breadcrumb |
| `record_panics()` | Crashlytics crash report for Rust panics |

Breadcrumbs are still limited by the flavor's level, so prod never sends Debug/Info.
`without_analytics()` / `without_crashlytics()` disable one service.

Create the adapter in the app's startup preparation: Firebase is initialized there and
native modules are bound only on the runtime's UI thread. Calls from other threads are
posted to that runtime; errors reported off the UI thread have no stack frames.
Firebase failures, such as invalid event names, are ignored because logging cannot
return errors. Use Analytics names: up to 40 letters, digits and underscores, starting
with a letter, at most 25 parameters, string values up to 100 characters.

Add `whisker-firebase-analytics` and `whisker-firebase-crashlytics` to the app's
`[patch.crates-io]` alongside the other fork crates.

## Sensitive data

Events, parameters, breadcrumbs and error messages leave the device. Never include
tokens, passwords, personal data or full payloads. Use fixed codes for `context` and
report only errors whose `Display` output is safe to send.
