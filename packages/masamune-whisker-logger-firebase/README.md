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
| `report_error(err, context)` | Crashlytics non-fatal `HandledError` with the fixed reason `handled_error` |
| `record_panics()` | Crashlytics crash report for Rust panics |

Set the Analytics user ID with the free function `set_user_id` (on the UI thread):

```rust,ignore
masamune_whisker_logger_firebase::set_user_id(Some(&uid))?; // after sign-in
masamune_whisker_logger_firebase::set_user_id(None)?;       // after sign-out
```

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

Handled error reports omit the error text, cause chain and context. Analytics
events, explicit log breadcrumbs and panic messages can still leave the device.
Keep tokens, passwords, personal data and full payloads out of those inputs.


## Consent-based Analytics

Start with Analytics collection disabled in the native project. When the app
restores or receives consent, use `FirebaseAnalyticsAdapter::enable_without_advertising`
and pass only approved events to `send`. On withdrawal, call `stop_and_reset`.
Keep the Firebase logger registered with `without_analytics()` if other app
events should stay outside this path.

The `analytics_startup` module provides native sources that switch off
collection at process startup before the app restores its consent. Keep
advertising identifiers disabled independently in the app's native project.

## Manual Crashlytics dispatch

Start with Crashlytics collection disabled in the native project (`collection_enabled = Some(false)`).
Use `FirebaseCrashlyticsAdapter::new()` to initialize Crashlytics independently of Analytics with
automatic collection disabled, record only allowlisted `DiagnosticReport` payloads (fixed
classification and vetted code locations), and explicitly call `check_for_unsent_reports`,
`send_unsent_reports`, or `delete_unsent_reports` after verifying session and deletion state.
