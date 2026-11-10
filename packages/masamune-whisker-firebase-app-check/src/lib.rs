//! Firebase App Check.
//!
//! Release builds use device attestation (Play Integrity on Android, App Attest on iOS); debug builds
//! use the debug provider.
//!
//! On iOS the provider factory has to be installed before `FirebaseApp.configure()`. The module
//! installs a gated factory while Whisker registers native modules at app launch (before any Rust
//! code runs), and [`activate`] opens it, so call [`activate`] as on Android. Add the
//! [`cng::MasamuneFirebaseAppCheck`] plugin to whisker.rs for the App Attest entitlement.

pub mod cng;

use masamune_whisker_bridge::NativeModule;
pub use masamune_whisker_bridge::{NativeError, NativeResult};
use serde_json::json;

fn module() -> NativeModule {
    NativeModule::new(env!("CARGO_PKG_NAME"), "FirebaseAppCheck")
}

/// Activates App Check. In debug builds, passing `debug_token` uses that fixed token.
///
/// On iOS "debug build" means the Debug configuration (or the `debug` provider forced by the CNG
/// plugin), the token is stored under the `GACAppCheckDebugToken` user default that the Firebase
/// debug provider reads, and the `FIRAAppCheckDebugToken` environment variable still takes
/// precedence. This call also configures the default Firebase app when it is not configured yet.
pub fn activate(debug_token: Option<&str>) -> NativeResult<bool> {
    module().call(
        "activate",
        &json!({ "debugToken": debug_token.unwrap_or("") }),
    )
}

/// App Check token (the value for the `X-Firebase-AppCheck` header).
pub async fn token(force_refresh: bool) -> NativeResult<String> {
    module()
        .call_async("token", &json!({ "forceRefresh": force_refresh }))
        .await
}
