# masamune-whisker-adapter

Plugin settings and startup work registered in one place, like Flutter Masamune's
`MasamuneAdapter` and the `masamuneAdapters` list. Each plugin crate provides a
`*MasamuneAdapter` holding its settings; the app lists them once and installs them while
preparing the app:

```rust,ignore
use masamune_whisker_adapter::MasamuneAdapters;
use masamune_whisker_local_notification::{LocalNotificationMasamuneAdapter, NotificationConfig};

fn adapters() -> MasamuneAdapters {
    MasamuneAdapters::new().adapter(LocalNotificationMasamuneAdapter::new(NotificationConfig {
        channel_id: "app_notification",
        channel_name: "App",
        channel_description: "App notifications",
        small_icon: None,
    }))
}

#[whisker::main]
fn app() -> Element {
    logger::init();
    adapters().install();
    render! { Root }
}
```

`install` calls every adapter's `prepare` in order, then spawns every `start`:

- `prepare` is synchronous setup that must happen before the first screen (for example
  creating the notification channel).
- `start` is background work (for example re-arming saved notifications). It belongs to the
  owner that called `install`, so it keeps running across screens and the boot screen, and it
  does not delay startup. The boot screen does not run adapters; the app's boot task stays the
  place for work the first screen depends on.

Failures are reported through `masamune_whisker_logger::report_error` and do not stop the other
adapters, so initialize the logger first.

Plugins and the app read the installed settings with `XxxMasamuneAdapter::primary()`, which
returns `None` until `install` ran in the current runtime.

## Rules

- Adapters live per runtime. `app()` runs again after some hot reloads and when Android recreates
  the activity; `install` then replaces the adapters and cancels their unfinished start work, so
  `prepare` and `start` must be safe to repeat.
- Add each adapter type once. A duplicate panics in debug builds and is dropped with an error log
  in release builds.
- Use it on the UI thread only. Contexts provided inside `prepare` are not visible to screens.
- Build-time settings (permissions, `Info.plist`, `AndroidManifest.xml`) stay in `whisker.rs`,
  which runs as a separate program.
- Data access (`masamune-whisker-functions`, `-storage`, `-model`) keeps explicit adapters passed
  to its handles instead.
