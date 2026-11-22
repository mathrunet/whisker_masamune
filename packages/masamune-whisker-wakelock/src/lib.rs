#![doc = include_str!("../README.md")]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use masamune_whisker_bridge::NativeModule;
pub use masamune_whisker_bridge::{NativeError, NativeResult};
use serde_json::json;

fn module() -> NativeModule {
    NativeModule::new(env!("CARGO_PKG_NAME"), "Wakelock")
}

/// Keeps the screen on or lets it sleep again.
///
/// Implemented by [`NativeWakelockAdapter`] (the OS) and [`RuntimeWakelockAdapter`] (in memory,
/// for tests and previews). Use it through a [`Wakelock`] handle.
pub trait WakelockAdapter {
    /// Keeps the screen on (`true`) or restores the normal sleep timer (`false`). Returns the new
    /// state.
    fn set_enabled(&self, enabled: bool) -> NativeResult<bool>;

    /// Whether the screen is kept on.
    fn is_enabled(&self) -> NativeResult<bool>;
}

/// The OS wake lock.
///
/// - iOS: `UIApplication.shared.isIdleTimerDisabled`.
/// - Android: `WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON` on the window of the current
///   activity, set on the UI thread. The flag is applied again when a new activity attaches (for
///   example after a configuration change), and a call made before the first activity exists is
///   applied once it attaches.
///
/// Both only affect the app while it is in the foreground; the OS restores the sleep timer when
/// the app leaves the screen and applies the setting again when it returns. No permission is
/// needed.
#[derive(Debug, Clone, Copy, Default)]
pub struct NativeWakelockAdapter;

impl WakelockAdapter for NativeWakelockAdapter {
    fn set_enabled(&self, enabled: bool) -> NativeResult<bool> {
        module().call("setEnabled", &json!({ "enabled": enabled }))
    }

    fn is_enabled(&self) -> NativeResult<bool> {
        module().call("isEnabled", &json!({}))
    }
}

/// In-memory wake lock for tests and previews.
///
/// Clones share the same state. [`RuntimeWakelockAdapter::history`] records every
/// [`WakelockAdapter::set_enabled`] call in order.
#[derive(Debug, Clone, Default)]
pub struct RuntimeWakelockAdapter {
    enabled: Rc<Cell<bool>>,
    history: Rc<RefCell<Vec<bool>>>,
}

impl RuntimeWakelockAdapter {
    pub fn new() -> Self {
        Self::default()
    }

    /// The values passed to [`WakelockAdapter::set_enabled`], oldest first.
    pub fn history(&self) -> Vec<bool> {
        self.history.borrow().clone()
    }
}

impl WakelockAdapter for RuntimeWakelockAdapter {
    fn set_enabled(&self, enabled: bool) -> NativeResult<bool> {
        self.enabled.set(enabled);
        self.history.borrow_mut().push(enabled);
        Ok(enabled)
    }

    fn is_enabled(&self) -> NativeResult<bool> {
        Ok(self.enabled.get())
    }
}

/// Handle to a [`WakelockAdapter`]. Cheap to clone; clones share the adapter.
///
/// ```no_run
/// use masamune_whisker_wakelock::{RuntimeWakelockAdapter, Wakelock};
///
/// // Production: the OS. Tests and the `test` flavor: an in-memory adapter.
/// let wakelock = Wakelock::native();
/// let wakelock = Wakelock::new(RuntimeWakelockAdapter::new());
/// wakelock.enable()?;
/// assert!(wakelock.is_enabled()?);
/// # Ok::<(), masamune_whisker_wakelock::NativeError>(())
/// ```
#[derive(Clone)]
pub struct Wakelock {
    adapter: Rc<dyn WakelockAdapter>,
}

impl Wakelock {
    /// A handle to `adapter`.
    pub fn new(adapter: impl WakelockAdapter + 'static) -> Self {
        Self {
            adapter: Rc::new(adapter),
        }
    }

    /// A handle to the OS wake lock ([`NativeWakelockAdapter`]).
    pub fn native() -> Self {
        Self::new(NativeWakelockAdapter)
    }

    /// Keeps the screen on.
    pub fn enable(&self) -> NativeResult<()> {
        self.set_enabled(true).map(drop)
    }

    /// Lets the screen sleep again.
    pub fn disable(&self) -> NativeResult<()> {
        self.set_enabled(false).map(drop)
    }

    /// [`Wakelock::enable`] or [`Wakelock::disable`]. Returns the new state.
    pub fn set_enabled(&self, enabled: bool) -> NativeResult<bool> {
        self.adapter.set_enabled(enabled)
    }

    /// Whether the screen is kept on.
    pub fn is_enabled(&self) -> NativeResult<bool> {
        self.adapter.is_enabled()
    }
}

impl std::fmt::Debug for Wakelock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Wakelock").finish_non_exhaustive()
    }
}

/// Keeps the screen on with the OS wake lock.
pub fn enable() -> NativeResult<()> {
    Wakelock::native().enable()
}

/// Lets the screen sleep again (OS wake lock).
pub fn disable() -> NativeResult<()> {
    Wakelock::native().disable()
}

/// Whether the OS wake lock keeps the screen on.
pub fn is_enabled() -> NativeResult<bool> {
    Wakelock::native().is_enabled()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_adapter_tracks_the_state() {
        let adapter = RuntimeWakelockAdapter::new();
        let wakelock = Wakelock::new(adapter.clone());
        assert!(!wakelock.is_enabled().unwrap());
        wakelock.enable().unwrap();
        assert!(wakelock.is_enabled().unwrap());
        assert!(wakelock.clone().is_enabled().unwrap());
        wakelock.disable().unwrap();
        assert!(!wakelock.is_enabled().unwrap());
        assert!(wakelock.set_enabled(true).unwrap());
        assert_eq!(adapter.history(), vec![true, false, true]);
    }
}
