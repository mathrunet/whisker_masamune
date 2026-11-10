//! Localization for Whisker apps, all in one crate.
//!
//! - [`localize!`]: typed strings written as a Rust trait with one implementation per language
//!   (generates `LANGUAGES`, `register()` and `l()`).
//! - [`init_locale`] / [`change_language`] / [`LanguagePref`]: follow the system or pin one
//!   language, saved across launches. The first registered language is the fallback.
//! - [`system_languages`] / [`follow_system`]: system languages on Android, iOS and the Web,
//!   and rebuilding the UI when they change.
//! - [`language_name`]: names of languages for language pickers.
//! - `LocalizeBuilder` (feature `build`): generates `l()` from Masamune localize YAML in
//!   `build.rs` for apps ported from Flutter.
//! - [`cng::MasamuneLocalize`]: declares the languages (and localized `Info.plist` values) of the
//!   iOS app so system UI follows the device language.
//!
//! Screens read `l()` while rendering, so a language change is applied by rebuilding everything
//! under `masamune_whisker_appearance::AppRoot`.
//!
//! ```ignore
//! #[whisker::main]
//! fn app() {
//!     i18n::register();
//!     masamune_whisker_localize::init_locale(&masamune_whisker_localize::system_languages(), None);
//!     masamune_whisker_localize::follow_system();
//!     render! { Root }
//! }
//! ```

#[cfg(feature = "build")]
mod build;
pub mod cng;
#[cfg(feature = "runtime")]
mod locale;
#[cfg(feature = "runtime")]
mod macros;
mod names;
#[cfg(feature = "runtime")]
mod system;

#[cfg(feature = "build")]
pub use build::{LocalizeBuilder, method_name};
#[cfg(feature = "runtime")]
pub use locale::{
    LanguagePref, index_of, init_locale, language, language_index, language_pref, language_tag,
    languages, register_languages, set_language_pref, set_system_languages,
};
pub use names::language_name;
#[cfg(feature = "runtime")]
pub use system::{
    NativeError, NativeResult, Subscription, SystemLocale, follow_system,
    on_system_languages_changed, system_languages, system_locales,
};

/// Selects and saves a language preference, rebuilding the UI when the language changes.
#[cfg(feature = "runtime")]
pub fn change_language(pref: LanguagePref) {
    if set_language_pref(pref) {
        masamune_whisker_appearance::rebuild();
    }
}
