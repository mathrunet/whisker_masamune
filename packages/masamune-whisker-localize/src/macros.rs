//! `localize!`: typed strings written as a Rust trait with one implementation per language.

/// Declares the supported languages and generates `LANGUAGES`, `register()` and `l()`.
///
/// Write the strings as a trait and implement it once per language. A language that misses a
/// string does not compile. Arguments, plurals and word order are plain Rust. Methods take
/// concrete argument types (`&str`, `u32`, …) so the trait stays object safe.
///
/// ```ignore
/// pub trait Strings {
///     fn ok(&self) -> &'static str;
///     fn hello(&self, name: &str) -> String;
///     fn days_left(&self, days: u32) -> String;
/// }
///
/// pub struct En;
/// impl Strings for En {
///     fn ok(&self) -> &'static str { "OK" }
///     fn hello(&self, name: &str) -> String { format!("Hello, {name}") }
///     fn days_left(&self, days: u32) -> String {
///         match days { 1 => "1 day left".into(), _ => format!("{days} days left") }
///     }
/// }
/// // impl Strings for Ja { … }
///
/// // The first language is the fallback.
/// masamune_whisker_localize::localize! { Strings; "en_US" => En, "ja_JP" => Ja }
///
/// // Startup: register(); init_locale(&system_languages(), None); follow_system();
/// // Screens: l().hello("Taro")
/// ```
#[macro_export]
macro_rules! localize {
    ($strings:path; $($tag:literal => $value:expr),+ $(,)?) => {
        /// Supported languages; the first is the fallback.
        pub const LANGUAGES: &[&str] = &[$($tag),+];

        static TABLE: &[&(dyn $strings + Sync)] = &[$(&$value),+];

        /// Registers [`LANGUAGES`] with masamune-whisker-localize. Call once at startup.
        pub fn register() {
            $crate::register_languages(LANGUAGES);
        }

        /// Strings of the current language.
        pub fn l() -> &'static dyn $strings {
            TABLE[$crate::language_index().min(TABLE.len() - 1)]
        }
    };
}

#[cfg(test)]
mod tests {
    use crate::{language_tag, set_system_languages};

    pub trait Strings {
        fn ok(&self) -> &'static str;
        fn hello(&self, name: &str) -> String;
    }

    struct En;
    impl Strings for En {
        fn ok(&self) -> &'static str {
            "OK"
        }
        fn hello(&self, name: &str) -> String {
            format!("Hello, {name}")
        }
    }

    struct Ja;
    impl Strings for Ja {
        fn ok(&self) -> &'static str {
            "了解"
        }
        fn hello(&self, name: &str) -> String {
            format!("こんにちは、{name}さん")
        }
    }

    localize! { Strings; "en_US" => En, "ja_JP" => Ja }

    // The only test that touches the global language state.
    #[test]
    fn follows_the_current_language() {
        register();
        assert_eq!(LANGUAGES, ["en_US", "ja_JP"]);
        set_system_languages(&["ja".into()]);
        assert_eq!(language_tag(), "ja_JP");
        assert_eq!(l().ok(), "了解");
        assert_eq!(l().hello("Taro"), "こんにちは、Taroさん");
        set_system_languages(&["fr_FR".into()]);
        assert_eq!(l().hello("Taro"), "Hello, Taro");
    }
}
