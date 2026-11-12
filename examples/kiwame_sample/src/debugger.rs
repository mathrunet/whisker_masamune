//! AI debugger. Enabled only in debug builds of the dev and test flavors.

use masamune_whisker_ai_debugger::{AiDebugController, AiDebuggerConfig, ai_debugger_config};
use masamune_whisker_logger::Flavor;

thread_local! {
    static DEBUGGER: AiDebugController = create();
}

/// The controller shared by the whole app.
pub fn controller() -> AiDebugController {
    DEBUGGER.with(Clone::clone)
}

fn create() -> AiDebugController {
    // An invalid flavor is reported by `prepare`; keep the debugger off meanwhile.
    let flavor = crate::config::flavor().unwrap_or(Flavor::Prod);
    AiDebugController::new(ai_debugger_config!(flavor)).unwrap_or_else(|error| {
        log::warn!("ai debugger disabled: {error}");
        AiDebugController::new(AiDebuggerConfig::new(Flavor::Prod)).expect("disabled ai debugger")
    })
}
