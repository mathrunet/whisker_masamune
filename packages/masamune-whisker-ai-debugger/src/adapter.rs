use std::sync::atomic::{AtomicBool, Ordering};

use masamune_whisker_adapter::{AdapterError, MasamuneAdapter};

use crate::AiDebugController;

static PANIC_HOOK: AtomicBool = AtomicBool::new(false);

/// AI debugger for [`masamune_whisker_adapter::MasamuneAdapters`] (Masamune's
/// `AIDebuggerMasamuneAdapter`): installs the panic hook while the app prepares and hands the
/// controller to the root `AiDebugger` through [`Self::primary`].
///
/// Install the adapters after Crashlytics' `record_panics` so both hooks run, and still add
/// [`AiDebugController::logger_adapter`] to the `LoggerConfig` before installing.
#[derive(Clone)]
pub struct AiDebuggerMasamuneAdapter {
    pub controller: AiDebugController,
}

impl AiDebuggerMasamuneAdapter {
    pub fn new(controller: AiDebugController) -> Self {
        Self { controller }
    }
}

impl MasamuneAdapter for AiDebuggerMasamuneAdapter {
    fn prepare(&self) -> Result<(), AdapterError> {
        // The hook chains onto the previous one, so installing it again on every re-run of `app()`
        // would report each panic several times.
        if self.controller.enabled() && !PANIC_HOOK.swap(true, Ordering::SeqCst) {
            self.controller.install_panic_hook();
        }
        Ok(())
    }
}
