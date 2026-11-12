//! Persisted session and threshold settings (`AIDebugSettings` / `_AIDebugSettingsStore`).

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Duration;

use serde_json::{Value, json};

use crate::types::{AiDebugModel, AiDebugPermissionMode};

/// Models, permission modes and slow-operation thresholds chosen in the settings dialog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiDebugSettings {
    /// Provider shared by every request (`codex`, `claude`, …), or `None` for automatic selection.
    pub provider: Option<String>,
    /// Model of instructions sent from the panel.
    pub manual_model: AiDebugModel,
    pub manual_permission_mode: AiDebugPermissionMode,
    /// Model of unexpected-error incidents.
    pub error_model: AiDebugModel,
    pub error_permission_mode: AiDebugPermissionMode,
    /// Model of slow-operation incidents.
    pub performance_model: AiDebugModel,
    pub performance_permission_mode: AiDebugPermissionMode,
    /// Model load threshold (`masamune.model.load|…` traces). Zero disables it.
    pub model_load_timeout: Duration,
    /// Progress indicator threshold (`masamune.indicator.show|…` traces). Zero disables it.
    pub indicator_timeout: Duration,
}

impl Default for AiDebugSettings {
    fn default() -> Self {
        Self {
            provider: None,
            manual_model: AiDebugModel::Sol,
            manual_permission_mode: AiDebugPermissionMode::Plan,
            error_model: AiDebugModel::Sol,
            error_permission_mode: AiDebugPermissionMode::BypassPermissions,
            performance_model: AiDebugModel::Sol,
            performance_permission_mode: AiDebugPermissionMode::BypassPermissions,
            model_load_timeout: Duration::from_secs(5),
            indicator_timeout: Duration::from_secs(10),
        }
    }
}

impl AiDebugSettings {
    /// JSON with the same keys as the Flutter package.
    pub fn to_json(&self) -> Value {
        json!({
            "provider": self.provider,
            "manualModel": self.manual_model.as_str(),
            "manualPermissionMode": self.manual_permission_mode.as_str(),
            "errorModel": self.error_model.as_str(),
            "errorPermissionMode": self.error_permission_mode.as_str(),
            "performanceModel": self.performance_model.as_str(),
            "performancePermissionMode": self.performance_permission_mode.as_str(),
            "modelLoadTimeoutMs": self.model_load_timeout.as_millis() as u64,
            "indicatorTimeoutMs": self.indicator_timeout.as_millis() as u64,
        })
    }

    /// Reads saved JSON; unknown or invalid values fall back to `fallback`. A present non-string
    /// `provider` (such as `null`) means automatic.
    pub fn from_json(json: &Value, fallback: &Self) -> Self {
        let text = |key: &str| json.get(key).and_then(Value::as_str);
        let model = |key: &str, default: AiDebugModel| {
            text(key).and_then(AiDebugModel::parse).unwrap_or(default)
        };
        let mode = |key: &str, default: AiDebugPermissionMode| {
            text(key)
                .and_then(AiDebugPermissionMode::parse)
                .unwrap_or(default)
        };
        let duration = |key: &str, default: Duration| {
            json.get(key)
                .and_then(Value::as_f64)
                .filter(|ms| *ms > 0.0)
                .map(|ms| Duration::from_millis(ms as u64))
                .unwrap_or(default)
        };
        Self {
            provider: match json.get("provider") {
                Some(value) => value.as_str().map(str::to_owned),
                None => fallback.provider.clone(),
            },
            manual_model: model("manualModel", fallback.manual_model),
            manual_permission_mode: mode("manualPermissionMode", fallback.manual_permission_mode),
            error_model: model("errorModel", fallback.error_model),
            error_permission_mode: mode("errorPermissionMode", fallback.error_permission_mode),
            performance_model: model("performanceModel", fallback.performance_model),
            performance_permission_mode: mode(
                "performancePermissionMode",
                fallback.performance_permission_mode,
            ),
            model_load_timeout: duration("modelLoadTimeoutMs", fallback.model_load_timeout),
            indicator_timeout: duration("indicatorTimeoutMs", fallback.indicator_timeout),
        }
    }
}

/// Where settings are saved. The default is `whisker-local-store`.
pub trait AiDebugSettingsStore {
    fn load(&self, key: &str) -> Result<Option<String>, String>;
    fn save(&self, key: &str, value: &str) -> Result<(), String>;
}

/// `whisker-local-store` (UserDefaults, SharedPreferences, localStorage).
pub struct LocalSettingsStore;

impl AiDebugSettingsStore for LocalSettingsStore {
    fn load(&self, key: &str) -> Result<Option<String>, String> {
        whisker_local_store::WhiskerLocalStore::load(key.to_owned()).map_err(|e| e.to_string())
    }

    fn save(&self, key: &str, value: &str) -> Result<(), String> {
        whisker_local_store::WhiskerLocalStore::save(key.to_owned(), value.to_owned())
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

/// In-memory settings, for tests.
#[derive(Default)]
pub struct MemorySettingsStore {
    values: RefCell<HashMap<String, String>>,
}

impl AiDebugSettingsStore for MemorySettingsStore {
    fn load(&self, key: &str) -> Result<Option<String>, String> {
        Ok(self.values.borrow().get(key).cloned())
    }

    fn save(&self, key: &str, value: &str) -> Result<(), String> {
        self.values
            .borrow_mut()
            .insert(key.to_owned(), value.to_owned());
        Ok(())
    }
}

/// Settings key of a project (`masamune_ai_debugger.settings.v2.<projectId>`).
pub(crate) fn settings_key(project_id: &str) -> String {
    let suffix = if project_id.is_empty() {
        "default"
    } else {
        project_id
    };
    format!("masamune_ai_debugger.settings.v2.{suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_migrates() {
        let settings = AiDebugSettings {
            provider: Some("claude".into()),
            manual_model: AiDebugModel::Astra,
            indicator_timeout: Duration::from_millis(2500),
            ..Default::default()
        };
        let fallback = AiDebugSettings::default();
        assert_eq!(
            AiDebugSettings::from_json(&settings.to_json(), &fallback),
            settings
        );

        let legacy = json!({
            "manualModel": "opus",
            "errorModel": "haiku",
            "performanceModel": "mythos",
            "manualPermissionMode": "unknown",
            "modelLoadTimeoutMs": 0,
            "indicatorTimeoutMs": "x",
        });
        let fallback = AiDebugSettings {
            provider: Some("codex".into()),
            ..Default::default()
        };
        let migrated = AiDebugSettings::from_json(&legacy, &fallback);
        assert_eq!(migrated.manual_model, AiDebugModel::Sol);
        assert_eq!(migrated.error_model, AiDebugModel::Luna);
        assert_eq!(migrated.performance_model, AiDebugModel::Astra);
        assert_eq!(migrated.manual_permission_mode, AiDebugPermissionMode::Plan);
        assert_eq!(
            migrated.error_permission_mode,
            AiDebugPermissionMode::BypassPermissions
        );
        assert_eq!(
            migrated.performance_permission_mode,
            AiDebugPermissionMode::BypassPermissions
        );
        assert_eq!(migrated.model_load_timeout, Duration::from_secs(5));
        assert_eq!(migrated.indicator_timeout, Duration::from_secs(10));
        // A missing provider keeps the fallback; an explicit null means automatic.
        assert_eq!(migrated.provider.as_deref(), Some("codex"));
        let automatic = AiDebugSettings::from_json(&json!({ "provider": null }), &fallback);
        assert_eq!(automatic.provider, None);
    }

    #[test]
    fn keys_are_per_project() {
        assert_eq!(settings_key(""), "masamune_ai_debugger.settings.v2.default");
        assert_eq!(
            settings_key("Users-me-app"),
            "masamune_ai_debugger.settings.v2.Users-me-app"
        );
    }
}
