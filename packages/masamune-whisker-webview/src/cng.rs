//! CNG plugin that declares what pages inside [`crate::MasamuneWebView`] may ask for.
//!
//! - Android: `CAMERA`, `RECORD_AUDIO` and `MODIFY_AUDIO_SETTINGS`, so `getUserMedia` can be
//!   granted (see [`crate::WebViewMasamuneAdapter::grant_media_permissions`]).
//! - iOS: `NSCameraUsageDescription`, `NSMicrophoneUsageDescription` and
//!   `NSPhotoLibraryUsageDescription`, used by `getUserMedia` and by `<input type="file">`.
//!
//! ```ignore
//! use masamune_whisker_webview::cng::MasamuneWebViewPlugin;
//!
//! app.plugin::<MasamuneWebViewPlugin>(|c| {
//!     c.camera_usage("Used for video calls in the web app.")
//!         .microphone_usage("Used for voice input in the web app.");
//! });
//! ```
//!
//! Descriptions that are not set fall back to short English texts. A default never replaces a
//! value another plugin (or the app) already wrote; an explicit description does.

use serde::{Deserialize, Serialize};
use whisker_plugin::{GenerateContext, Operation, PlistValue, Plugin, PluginConfig, Target};

/// Android permissions added to the manifest.
pub const ANDROID_PERMISSIONS: [&str; 3] = [
    "android.permission.CAMERA",
    "android.permission.RECORD_AUDIO",
    "android.permission.MODIFY_AUDIO_SETTINGS",
];

/// Default `NSCameraUsageDescription`.
pub const DEFAULT_CAMERA_USAGE: &str =
    "Web pages in this app can use the camera when you allow it.";
/// Default `NSMicrophoneUsageDescription`.
pub const DEFAULT_MICROPHONE_USAGE: &str =
    "Web pages in this app can use the microphone when you allow it.";
/// Default `NSPhotoLibraryUsageDescription`.
pub const DEFAULT_PHOTO_LIBRARY_USAGE: &str = "Web pages in this app can upload photos you choose.";

/// Settings of [`MasamuneWebViewPlugin`].
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct MasamuneWebViewConfig {
    /// `NSCameraUsageDescription` (default [`DEFAULT_CAMERA_USAGE`]).
    #[serde(default)]
    pub camera_usage: Option<String>,
    /// `NSMicrophoneUsageDescription` (default [`DEFAULT_MICROPHONE_USAGE`]).
    #[serde(default)]
    pub microphone_usage: Option<String>,
    /// `NSPhotoLibraryUsageDescription` (default [`DEFAULT_PHOTO_LIBRARY_USAGE`]).
    #[serde(default)]
    pub photo_library_usage: Option<String>,
}

impl MasamuneWebViewConfig {
    /// Why pages use the camera (`NSCameraUsageDescription`).
    pub fn camera_usage(&mut self, description: impl Into<String>) -> &mut Self {
        self.camera_usage = Some(description.into());
        self
    }

    /// Why pages use the microphone (`NSMicrophoneUsageDescription`).
    pub fn microphone_usage(&mut self, description: impl Into<String>) -> &mut Self {
        self.microphone_usage = Some(description.into());
        self
    }

    /// Why pages read the photo library (`NSPhotoLibraryUsageDescription`).
    pub fn photo_library_usage(&mut self, description: impl Into<String>) -> &mut Self {
        self.photo_library_usage = Some(description.into());
        self
    }
}

impl PluginConfig for MasamuneWebViewConfig {
    const NAME: &'static str = "masamune-whisker-webview";
}

/// The plugin. Register it with `app.plugin::<MasamuneWebViewPlugin>(|c| …)`.
pub struct MasamuneWebViewPlugin;

impl Plugin for MasamuneWebViewPlugin {
    type Config = MasamuneWebViewConfig;

    fn apply(&self, ctx: &mut GenerateContext, cfg: &MasamuneWebViewConfig) -> anyhow::Result<()> {
        if let Some(android) = ctx.android.as_mut() {
            let permissions = &mut android.manifest.permissions;
            let mut added = 0;
            for permission in ANDROID_PERMISSIONS {
                if !permissions.iter().any(|existing| existing == permission) {
                    permissions.push(permission.to_string());
                    added += 1;
                }
            }
            if added > 0 {
                ctx.journal.record(
                    MasamuneWebViewConfig::NAME,
                    Target::Android,
                    "manifest.permissions",
                    Operation::ArrayPush { count: added },
                );
            }
        }
        if let Some(ios) = ctx.ios.as_mut() {
            for (key, explicit, fallback) in [
                (
                    "NSCameraUsageDescription",
                    &cfg.camera_usage,
                    DEFAULT_CAMERA_USAGE,
                ),
                (
                    "NSMicrophoneUsageDescription",
                    &cfg.microphone_usage,
                    DEFAULT_MICROPHONE_USAGE,
                ),
                (
                    "NSPhotoLibraryUsageDescription",
                    &cfg.photo_library_usage,
                    DEFAULT_PHOTO_LIBRARY_USAGE,
                ),
            ] {
                let exists = ios.info_plist.contains_key(key);
                let value = match explicit {
                    Some(value) => value.clone(),
                    None if exists => continue,
                    None => fallback.to_string(),
                };
                ios.info_plist
                    .insert(key.to_string(), PlistValue::String(value));
                ctx.journal.record(
                    MasamuneWebViewConfig::NAME,
                    Target::Ios,
                    &format!("info_plist.{key}"),
                    if exists {
                        Operation::Override
                    } else {
                        Operation::Set
                    },
                );
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use whisker_plugin::{AndroidProjectIr, IosProjectIr};

    fn context() -> GenerateContext {
        GenerateContext {
            ios: Some(IosProjectIr::default()),
            android: Some(AndroidProjectIr::default()),
            ..Default::default()
        }
    }

    #[test]
    fn adds_android_permissions_once() {
        let mut ctx = context();
        ctx.android
            .as_mut()
            .unwrap()
            .manifest
            .permissions
            .push("android.permission.CAMERA".into());
        let cfg = MasamuneWebViewConfig::default();
        MasamuneWebViewPlugin.apply(&mut ctx, &cfg).unwrap();
        MasamuneWebViewPlugin.apply(&mut ctx, &cfg).unwrap();
        assert_eq!(
            ctx.android.unwrap().manifest.permissions,
            ANDROID_PERMISSIONS.map(String::from).to_vec()
        );
    }

    #[test]
    fn writes_default_and_explicit_descriptions() {
        let mut ctx = context();
        let mut cfg = MasamuneWebViewConfig::default();
        cfg.camera_usage("camera");
        MasamuneWebViewPlugin.apply(&mut ctx, &cfg).unwrap();
        let plist = ctx.ios.unwrap().info_plist;
        assert_eq!(
            plist["NSCameraUsageDescription"],
            PlistValue::String("camera".into())
        );
        assert_eq!(
            plist["NSMicrophoneUsageDescription"],
            PlistValue::String(DEFAULT_MICROPHONE_USAGE.into())
        );
        assert_eq!(
            plist["NSPhotoLibraryUsageDescription"],
            PlistValue::String(DEFAULT_PHOTO_LIBRARY_USAGE.into())
        );
    }

    #[test]
    fn defaults_keep_existing_values() {
        let mut ctx = context();
        ctx.ios.as_mut().unwrap().info_plist.insert(
            "NSCameraUsageDescription".into(),
            PlistValue::String("from image picker".into()),
        );
        MasamuneWebViewPlugin
            .apply(&mut ctx, &MasamuneWebViewConfig::default())
            .unwrap();
        assert_eq!(
            ctx.ios.unwrap().info_plist["NSCameraUsageDescription"],
            PlistValue::String("from image picker".into())
        );
    }
}
