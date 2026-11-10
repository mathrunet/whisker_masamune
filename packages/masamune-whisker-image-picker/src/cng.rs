//! CNG plugin that writes the iOS privacy strings the image picker needs.
//!
//! Taking a photo requires `NSCameraUsageDescription` (iOS terminates the app without it). The
//! photo library picker runs out of process and needs no permission, but App Store review may ask
//! for `NSPhotoLibraryUsageDescription`, so it can be set too.
//!
//! ```ignore
//! use masamune_whisker_image_picker::cng::MasamuneImagePicker;
//!
//! app.plugin::<MasamuneImagePicker>(|c| {
//!     c.camera_usage("Take a profile photo.")
//!         .photo_library_usage("Choose a profile photo.");
//! });
//! ```
//!
//! Android needs no setup, so the plugin does nothing there.

use serde::{Deserialize, Serialize};
use whisker_plugin::{GenerateContext, Operation, PlistValue, Plugin, PluginConfig, Target};

/// Settings of [`MasamuneImagePicker`].
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct MasamuneImagePickerConfig {
    /// `NSCameraUsageDescription`.
    #[serde(default)]
    pub camera_usage: Option<String>,
    /// `NSPhotoLibraryUsageDescription`.
    #[serde(default)]
    pub photo_library_usage: Option<String>,
}

impl MasamuneImagePickerConfig {
    /// Why the app uses the camera (`NSCameraUsageDescription`, required for `pick_image(true, …)`).
    pub fn camera_usage(&mut self, description: impl Into<String>) -> &mut Self {
        self.camera_usage = Some(description.into());
        self
    }

    /// Why the app reads the photo library (`NSPhotoLibraryUsageDescription`).
    pub fn photo_library_usage(&mut self, description: impl Into<String>) -> &mut Self {
        self.photo_library_usage = Some(description.into());
        self
    }
}

impl PluginConfig for MasamuneImagePickerConfig {
    const NAME: &'static str = "masamune-whisker-image-picker";
}

/// The plugin. Register it with `app.plugin::<MasamuneImagePicker>(|c| …)`.
pub struct MasamuneImagePicker;

impl Plugin for MasamuneImagePicker {
    type Config = MasamuneImagePickerConfig;

    fn apply(
        &self,
        ctx: &mut GenerateContext,
        cfg: &MasamuneImagePickerConfig,
    ) -> anyhow::Result<()> {
        let Some(ios) = ctx.ios.as_mut() else {
            return Ok(());
        };
        for (key, value) in [
            ("NSCameraUsageDescription", &cfg.camera_usage),
            ("NSPhotoLibraryUsageDescription", &cfg.photo_library_usage),
        ] {
            let Some(value) = value else { continue };
            ios.info_plist
                .insert(key.to_string(), PlistValue::String(value.clone()));
            ctx.journal.record(
                MasamuneImagePickerConfig::NAME,
                Target::Ios,
                &format!("info_plist.{key}"),
                Operation::Set,
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use whisker_plugin::IosProjectIr;

    #[test]
    fn writes_configured_descriptions() {
        let mut ctx = GenerateContext {
            ios: Some(IosProjectIr::default()),
            ..Default::default()
        };
        let mut cfg = MasamuneImagePickerConfig::default();
        cfg.camera_usage("camera");
        MasamuneImagePicker.apply(&mut ctx, &cfg).unwrap();
        let plist = ctx.ios.unwrap().info_plist;
        assert_eq!(
            plist["NSCameraUsageDescription"],
            PlistValue::String("camera".into())
        );
        assert!(!plist.contains_key("NSPhotoLibraryUsageDescription"));
    }

    #[test]
    fn android_is_a_no_op() {
        let mut ctx = GenerateContext::default();
        let mut cfg = MasamuneImagePickerConfig::default();
        cfg.camera_usage("camera").photo_library_usage("library");
        MasamuneImagePicker.apply(&mut ctx, &cfg).unwrap();
        assert!(ctx.ios.is_none());
    }
}
