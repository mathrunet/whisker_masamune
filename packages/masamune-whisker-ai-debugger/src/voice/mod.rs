//! Real-time voice input for the instruction field (feature `voice`).
//!
//! The protocol helpers are always compiled so they can be tested; the session, the microphone
//! (`masamune-whisker-audio-capture`) and the WebSocket client exist only with the feature.

#[cfg(any(feature = "voice", test))]
pub(crate) mod core;
#[cfg(feature = "voice")]
pub(crate) mod platform;
pub(crate) mod protocol;
#[cfg(feature = "voice")]
pub(crate) mod session;
