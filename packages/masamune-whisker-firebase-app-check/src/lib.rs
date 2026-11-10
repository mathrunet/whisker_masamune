//! Firebase App Check.
//!
//! Release builds use device attestation (Play Integrity on Android); debug builds use the debug provider.

use masamune_whisker_bridge::NativeModule;
pub use masamune_whisker_bridge::{NativeError, NativeResult};
use serde_json::json;

fn module() -> NativeModule {
    NativeModule::new(env!("CARGO_PKG_NAME"), "FirebaseAppCheck")
}

/// Activates App Check. In debug builds, passing `debug_token` uses that fixed token.
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
