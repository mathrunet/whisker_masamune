//! Registers bundled fonts so they can be used with `font_family("Name")`.
//!
//! The app bundles font files with whisker-asset and passes paths obtained from `whisker_asset::resolve`.

use masamune_whisker_bridge::NativeModule;
pub use masamune_whisker_bridge::{NativeError, NativeResult};
use serde::Serialize;

fn module() -> NativeModule {
    NativeModule::new(env!("CARGO_PKG_NAME"), "Fonts")
}

/// A single font file and the weights (100-900) assigned to it.
#[derive(Debug, Clone, Serialize)]
pub struct FontFile {
    pub path: String,
    pub weights: Vec<u16>,
}

/// A family name and the files that make up the family.
#[derive(Debug, Clone, Serialize)]
pub struct FontFamily {
    pub family: String,
    pub files: Vec<FontFile>,
}

impl FontFamily {
    pub fn new(family: impl Into<String>) -> Self {
        Self {
            family: family.into(),
            files: Vec::new(),
        }
    }

    pub fn file(mut self, path: impl Into<String>, weights: &[u16]) -> Self {
        self.files.push(FontFile {
            path: path.into(),
            weights: weights.to_vec(),
        });
        self
    }
}

/// Registers fonts and returns the names of the families that were registered. Already registered families are not reloaded.
///
/// Call this before the first Text is measured (at the start of `app()`).
pub fn register(families: &[FontFamily]) -> NativeResult<Vec<String>> {
    module().call("register", &serde_json::json!({ "families": families }))
}
