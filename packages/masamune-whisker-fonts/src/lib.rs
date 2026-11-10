//! Registers bundled fonts so they can be used with `font_family("Name")`.
//!
//! The app bundles font files with whisker-asset and passes paths obtained from `whisker_asset::resolve`
//! (`file:///android_asset/…` on Android, `file://<app bundle>/whisker_assets/…` on iOS).

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
///
/// On iOS the files are registered for the app process with CoreText. Because the whisker iOS Host
/// looks fonts up by the names stored inside the font files, the module maps each registered
/// family name to the PostScript name of its face (the face assigned to weight 400 by default,
/// and the face assigned to the requested weight when a weight is set), so any family name works
/// as on Android. Weights without an assigned face use the face of the closest assigned weight.
pub fn register(families: &[FontFamily]) -> NativeResult<Vec<String>> {
    module().call("register", &serde_json::json!({ "families": families }))
}
