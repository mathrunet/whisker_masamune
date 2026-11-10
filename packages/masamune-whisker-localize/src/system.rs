//! System languages from the OS (Android, iOS) or the browser, and following their changes.

use masamune_whisker_bridge::NativeModule;
pub use masamune_whisker_bridge::{NativeError, NativeResult, Subscription};
use serde::Deserialize;

use crate::locale::set_system_languages;

fn module() -> NativeModule {
    NativeModule::new(env!("CARGO_PKG_NAME"), "Localize")
}

/// One system language (`ja` / `JP`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemLocale {
    pub language: String,
    #[serde(default)]
    pub country: String,
}

impl SystemLocale {
    /// `ja_JP`, or `ja` without a country.
    pub fn tag(&self) -> String {
        if self.country.is_empty() {
            self.language.clone()
        } else {
            format!("{}_{}", self.language, self.country)
        }
    }
}

#[derive(Deserialize)]
struct LanguagesChanged {
    #[serde(default)]
    locales: Vec<SystemLocale>,
}

fn tags(locales: &[SystemLocale]) -> Vec<String> {
    locales.iter().map(SystemLocale::tag).collect()
}

/// System languages, most preferred first.
pub fn system_locales() -> NativeResult<Vec<SystemLocale>> {
    module().call("languages", &serde_json::json!({}))
}

/// System languages as tags (`["ja_JP", "en_US"]`), most preferred first. Empty when the host
/// cannot report them.
pub fn system_languages() -> Vec<String> {
    match system_locales() {
        Ok(locales) => tags(&locales),
        Err(error) => {
            log::warn!("system languages unavailable: {error}");
            Vec::new()
        }
    }
}

/// Receives the system languages as tags whenever they change. The current value is also
/// delivered right after subscribing.
pub fn on_system_languages_changed(handler: impl Fn(Vec<String>) + 'static) -> Subscription {
    module().subscribe("languagesChanged", move |event: LanguagesChanged| {
        handler(tags(&event.locales));
    })
}

whisker::runtime_local! {
    static SUBSCRIPTION: Subscription = on_system_languages_changed(apply);
}

fn apply(languages: Vec<String>) {
    if !languages.is_empty() && set_system_languages(&languages) {
        log::info!("localize: system language changed; rebuilding");
        masamune_whisker_appearance::rebuild();
    }
}

/// Follows the system languages, rebuilding the UI when they change the current language. Call
/// once on the UI thread after [`crate::init_locale`].
///
/// Add `locale` to the activity's `configChanges` (masamune-whisker-screen's
/// `ConfigChanges::LOCALE`), otherwise Android restarts the app instead. iOS restarts the app
/// when the system language changes, so there the change arrives as a fresh launch.
pub fn follow_system() {
    // Start from the current languages so the first event does not look like a change.
    let languages = system_languages();
    if !languages.is_empty() {
        set_system_languages(&languages);
    }
    SUBSCRIPTION.with(|_| {});
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_locales() {
        let locales: LanguagesChanged = serde_json::from_str(
            r#"{"locales":[{"language":"ja","country":"JP"},{"language":"en"}]}"#,
        )
        .unwrap();
        assert_eq!(tags(&locales.locales), ["ja_JP", "en"]);
    }
}
