#![doc = include_str!("../README.md")]

mod adapter;
pub mod cng;
mod config;
mod controller;
mod logger_adapter;
mod overlay;
mod redact;
mod settings;
mod transport;
mod types;
mod voice;

pub use adapter::AiDebuggerMasamuneAdapter;
pub use config::{AiDebugConfigError, AiDebugSpawner, AiDebuggerConfig};
pub use controller::{AiDebugController, INDICATOR_TRACE_PREFIX, MODEL_LOAD_TRACE_PREFIX};
pub use logger_adapter::AiDebuggerLoggerAdapter;
pub use overlay::{AiDebugger, AiDebuggerProps};
pub use redact::redact;
pub use settings::{
    AiDebugSettings, AiDebugSettingsStore, LocalSettingsStore, MemorySettingsStore,
};
pub use transport::{AiDebugError, AiDebugTransport, HttpTransport};
pub use types::{
    AiDebugAccount, AiDebugAuth, AiDebugAuthBootState, AiDebugAuthRestoreState,
    AiDebugAuthSnapshot, AiDebugAuthState, AiDebugCallbackResult, AiDebugCamera,
    AiDebugCameraPicture, AiDebugContextProvider, AiDebugContextSnapshot, AiDebugModel,
    AiDebugPermissionMode, AiDebugPurchase, AiDebugPurchaseProduct,
};
pub use voice::protocol::{AiDebugSpeechToken, VOICE_INPUT_LABEL, insert_transcript};

/// Paths used by [`ai_debugger_config!`]; not part of the public API.
#[doc(hidden)]
pub mod __private {
    pub use masamune_whisker_logger::Flavor;
}
