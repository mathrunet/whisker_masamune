//! Build-time settings.

use masamune_whisker_logger::{Flavor, InvalidFlavor};

/// `KIWAME_SAMPLE_FLAVOR` at compile time: `dev`, `test` or `prod` (unset means prod).
pub const FLAVOR: Option<&str> = option_env!("KIWAME_SAMPLE_FLAVOR");

/// The flavor of this build.
pub fn flavor() -> Result<Flavor, InvalidFlavor> {
    Flavor::from_env(FLAVOR)
}

/// Whether backends are in-memory runtime adapters (`test`), so E2E runs need no Firebase
/// project, network or accounts and always start from the same state.
pub fn uses_runtime_adapters() -> bool {
    FLAVOR == Some("test")
}

/// REST endpoint of the counter API for dev and prod builds.
pub const API_ENDPOINT: &str = match option_env!("KIWAME_SAMPLE_API_ENDPOINT") {
    Some(endpoint) => endpoint,
    None => "http://127.0.0.1:8787",
};
