//! CNG plugin that declares the microphone permission of the voice input (feature `voice`).
//!
//! - iOS: `NSMicrophoneUsageDescription` (iOS terminates the app without it once the microphone
//!   is used).
//! - Android: `android.permission.RECORD_AUDIO`.
//!
//! It writes nothing unless the app turns it on, so enable it only for the flavors that build the
//! `voice` feature (dev and test) and leave prod untouched:
//!
//! ```ignore
//! use masamune_whisker_ai_debugger::cng::MasamuneAiDebuggerPlugin;
//!
//! let flavor = std::env::var("MY_APP_FLAVOR").unwrap_or_default();
//! if matches!(flavor.as_str(), "dev" | "test") {
//!     app.plugin::<MasamuneAiDebuggerPlugin>(|c| {
//!         c.voice_input(true)
//!             .microphone_usage("Used for voice input of the AI debugger (development builds only).");
//!     });
//! }
//! ```
//!
//! The plugin lives in this crate (which every debugger app depends on) instead of the optional
//! `masamune-whisker-audio-capture`, because whisker-cng builds plugin binaries without the app's
//! features.

use serde::{Deserialize, Serialize};
use whisker_plugin::{GenerateContext, Operation, PlistValue, Plugin, PluginConfig, Target};

/// Android permission added to the manifest.
pub const RECORD_AUDIO: &str = "android.permission.RECORD_AUDIO";

/// Default `NSMicrophoneUsageDescription`.
pub const DEFAULT_MICROPHONE_USAGE: &str =
    "Used for voice input of the AI debugger (development builds only).";

/// Settings of [`MasamuneAiDebuggerPlugin`].
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct MasamuneAiDebuggerPluginConfig {
    /// Declares the microphone permission (default false: nothing is written).
    #[serde(default)]
    pub voice_input: bool,
    /// `NSMicrophoneUsageDescription` (default [`DEFAULT_MICROPHONE_USAGE`]).
    #[serde(default)]
    pub microphone_usage: Option<String>,
}

impl MasamuneAiDebuggerPluginConfig {
    /// Whether to declare the microphone permission for the voice input.
    pub fn voice_input(&mut self, enabled: bool) -> &mut Self {
        self.voice_input = enabled;
        self
    }

    /// Why the app uses the microphone (`NSMicrophoneUsageDescription`). Takes effect only with
    /// [`Self::voice_input`].
    pub fn microphone_usage(&mut self, description: impl Into<String>) -> &mut Self {
        self.microphone_usage = Some(description.into());
        self
    }
}

impl PluginConfig for MasamuneAiDebuggerPluginConfig {
    const NAME: &'static str = "masamune-whisker-ai-debugger";
}

/// The plugin. Register it with `app.plugin::<MasamuneAiDebuggerPlugin>(|c| …)`.
pub struct MasamuneAiDebuggerPlugin;

impl Plugin for MasamuneAiDebuggerPlugin {
    type Config = MasamuneAiDebuggerPluginConfig;

    fn apply(
        &self,
        ctx: &mut GenerateContext,
        cfg: &MasamuneAiDebuggerPluginConfig,
    ) -> anyhow::Result<()> {
        if !cfg.voice_input {
            return Ok(());
        }
        if let Some(android) = ctx.android.as_mut() {
            let permissions = &mut android.manifest.permissions;
            if !permissions.iter().any(|existing| existing == RECORD_AUDIO) {
                permissions.push(RECORD_AUDIO.to_string());
                ctx.journal.record(
                    MasamuneAiDebuggerPluginConfig::NAME,
                    Target::Android,
                    "manifest.permissions",
                    Operation::ArrayPush { count: 1 },
                );
            }
        }
        if let Some(ios) = ctx.ios.as_mut() {
            let key = "NSMicrophoneUsageDescription";
            let exists = ios.info_plist.contains_key(key);
            // A default never replaces a description another plugin (or the app) wrote.
            let value = match &cfg.microphone_usage {
                Some(value) => Some(value.clone()),
                None if exists => None,
                None => Some(DEFAULT_MICROPHONE_USAGE.to_string()),
            };
            if let Some(value) = value {
                ios.info_plist
                    .insert(key.to_string(), PlistValue::String(value));
                ctx.journal.record(
                    MasamuneAiDebuggerPluginConfig::NAME,
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
    fn writes_nothing_by_default() {
        let mut ctx = context();
        let mut cfg = MasamuneAiDebuggerPluginConfig::default();
        // A description alone does not turn the permission on.
        cfg.microphone_usage("mic");
        MasamuneAiDebuggerPlugin.apply(&mut ctx, &cfg).unwrap();
        assert!(ctx.android.unwrap().manifest.permissions.is_empty());
        assert!(ctx.ios.unwrap().info_plist.is_empty());
    }

    #[test]
    fn declares_the_microphone_once() {
        let mut ctx = context();
        let mut cfg = MasamuneAiDebuggerPluginConfig::default();
        cfg.voice_input(true);
        MasamuneAiDebuggerPlugin.apply(&mut ctx, &cfg).unwrap();
        MasamuneAiDebuggerPlugin.apply(&mut ctx, &cfg).unwrap();
        assert_eq!(
            ctx.android.unwrap().manifest.permissions,
            vec![RECORD_AUDIO.to_string()]
        );
        assert_eq!(
            ctx.ios.unwrap().info_plist["NSMicrophoneUsageDescription"],
            PlistValue::String(DEFAULT_MICROPHONE_USAGE.into())
        );
    }

    #[test]
    fn explicit_descriptions_override_and_defaults_keep_existing_values() {
        let mut ctx = context();
        let plist = &mut ctx.ios.as_mut().unwrap().info_plist;
        plist.insert(
            "NSMicrophoneUsageDescription".into(),
            PlistValue::String("app".into()),
        );
        let mut cfg = MasamuneAiDebuggerPluginConfig::default();
        cfg.voice_input(true);
        MasamuneAiDebuggerPlugin.apply(&mut ctx, &cfg).unwrap();
        assert_eq!(
            ctx.ios.as_ref().unwrap().info_plist["NSMicrophoneUsageDescription"],
            PlistValue::String("app".into())
        );
        cfg.microphone_usage("debugger");
        MasamuneAiDebuggerPlugin.apply(&mut ctx, &cfg).unwrap();
        assert_eq!(
            ctx.ios.unwrap().info_plist["NSMicrophoneUsageDescription"],
            PlistValue::String("debugger".into())
        );
    }
}
