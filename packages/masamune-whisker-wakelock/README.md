# masamune-whisker-wakelock

Keeps the screen awake while the app is in the foreground (the counterpart of Flutter's
`wakelock_plus`), for example on a player, kiosk or signage screen.

- **iOS**: `UIApplication.shared.isIdleTimerDisabled`.
- **Android**: `FLAG_KEEP_SCREEN_ON` on the current activity's window, set on the UI thread and
  applied again when a new activity attaches (configuration changes, a call made before the first
  activity exists).

No permission or setup is needed. The OS lets the screen sleep again while the app is in the
background and applies the setting again when the app returns.

## Usage

```rust,ignore
use masamune_whisker_wakelock as wakelock;

// The OS wake lock.
wakelock::enable()?;
assert!(wakelock::is_enabled()?);
wakelock::disable()?;
```

Pass a `Wakelock` handle to code that should run in tests or in the `test` flavor without the
OS:

```rust,ignore
use masamune_whisker_wakelock::{RuntimeWakelockAdapter, Wakelock};

let wakelock = if flavor == Flavor::Test {
    Wakelock::new(RuntimeWakelockAdapter::new())
} else {
    Wakelock::native()
};
wakelock.enable()?;

// In a unit test:
let adapter = RuntimeWakelockAdapter::new();
let wakelock = Wakelock::new(adapter.clone());
wakelock.enable()?;
assert_eq!(adapter.history(), vec![true]);
```

- `WakelockAdapter`: `set_enabled` / `is_enabled`. Implemented by `NativeWakelockAdapter` (the
  OS) and `RuntimeWakelockAdapter` (in memory; clones share the state and record every call).
- `Wakelock`: cheap-to-clone handle with `enable`, `disable`, `set_enabled` and `is_enabled`.
- `enable`, `disable`, `is_enabled`: shortcuts for `Wakelock::native()`.

## Behavior and limits

- The setting belongs to the app, not to a screen: disable it when the screen that needs it goes
  away (for example in an `on_cleanup` of that screen's component).
- Android: `is_enabled` returns the requested state; the flag reaches the window on the UI thread
  right after the call (or once an activity attaches).
- iOS: `is_enabled` reads `isIdleTimerDisabled` on the main thread, and the requested state from
  other threads.
