//! CNG plugin that writes the iOS camera usage description the scanner needs.
//!
//! iOS terminates an app that opens the camera without `NSCameraUsageDescription`.
//!
//! ```ignore
//! use masamune_whisker_barcode_scanner::cng::MasamuneBarcodeScanner;
//!
//! app.plugin::<MasamuneBarcodeScanner>(|c| {
//!     c.camera_usage("Scan the QR code on your ticket.");
//! });
//! ```
//!
//! Without a description, a short English default is written unless another plugin (or the
//! app) already set one; an explicit description always replaces it. Android declares the
//! `CAMERA` permission in the library manifest, so the plugin does nothing there.

use serde::{Deserialize, Serialize};
use whisker_plugin::{GenerateContext, Operation, PlistValue, Plugin, PluginConfig, Target};

const CAMERA_KEY: &str = "NSCameraUsageDescription";

/// Default `NSCameraUsageDescription`.
pub const DEFAULT_CAMERA_USAGE: &str = "The camera is used to scan QR codes.";

/// Settings of [`MasamuneBarcodeScanner`].
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct MasamuneBarcodeScannerConfig {
    /// `NSCameraUsageDescription` (default [`DEFAULT_CAMERA_USAGE`]).
    #[serde(default)]
    pub camera_usage: Option<String>,
}

impl MasamuneBarcodeScannerConfig {
    /// Why the app uses the camera (`NSCameraUsageDescription`).
    pub fn camera_usage(&mut self, description: impl Into<String>) -> &mut Self {
        self.camera_usage = Some(description.into());
        self
    }
}

impl PluginConfig for MasamuneBarcodeScannerConfig {
    const NAME: &'static str = "masamune-whisker-barcode-scanner";
}

/// The plugin. Register it with `app.plugin::<MasamuneBarcodeScanner>(|c| …)`.
pub struct MasamuneBarcodeScanner;

impl Plugin for MasamuneBarcodeScanner {
    type Config = MasamuneBarcodeScannerConfig;

    fn apply(
        &self,
        ctx: &mut GenerateContext,
        cfg: &MasamuneBarcodeScannerConfig,
    ) -> anyhow::Result<()> {
        let Some(ios) = ctx.ios.as_mut() else {
            return Ok(());
        };
        let exists = ios.info_plist.contains_key(CAMERA_KEY);
        let value = match &cfg.camera_usage {
            Some(value) => value.clone(),
            None if exists => return Ok(()),
            None => DEFAULT_CAMERA_USAGE.to_string(),
        };
        ios.info_plist
            .insert(CAMERA_KEY.to_string(), PlistValue::String(value));
        ctx.journal.record(
            MasamuneBarcodeScannerConfig::NAME,
            Target::Ios,
            &format!("info_plist.{CAMERA_KEY}"),
            if exists {
                Operation::Override
            } else {
                Operation::Set
            },
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use whisker_plugin::IosProjectIr;

    fn context() -> GenerateContext {
        GenerateContext {
            ios: Some(IosProjectIr::default()),
            ..Default::default()
        }
    }

    #[test]
    fn writes_default_and_explicit_descriptions() {
        let mut ctx = context();
        MasamuneBarcodeScanner
            .apply(&mut ctx, &MasamuneBarcodeScannerConfig::default())
            .unwrap();
        assert_eq!(
            ctx.ios.unwrap().info_plist[CAMERA_KEY],
            PlistValue::String(DEFAULT_CAMERA_USAGE.into())
        );

        let mut ctx = context();
        let mut cfg = MasamuneBarcodeScannerConfig::default();
        cfg.camera_usage("Scan tickets.");
        MasamuneBarcodeScanner.apply(&mut ctx, &cfg).unwrap();
        assert_eq!(
            ctx.ios.unwrap().info_plist[CAMERA_KEY],
            PlistValue::String("Scan tickets.".into())
        );
    }

    #[test]
    fn default_keeps_an_existing_description() {
        let mut ctx = context();
        ctx.ios.as_mut().unwrap().info_plist.insert(
            CAMERA_KEY.into(),
            PlistValue::String("from image picker".into()),
        );
        MasamuneBarcodeScanner
            .apply(&mut ctx, &MasamuneBarcodeScannerConfig::default())
            .unwrap();
        assert_eq!(
            ctx.ios.unwrap().info_plist[CAMERA_KEY],
            PlistValue::String("from image picker".into())
        );
    }

    #[test]
    fn android_is_a_no_op() {
        let mut ctx = GenerateContext::default();
        MasamuneBarcodeScanner
            .apply(&mut ctx, &MasamuneBarcodeScannerConfig::default())
            .unwrap();
        assert!(ctx.ios.is_none());
    }
}
