//! Build-script generator of typed localization tables.
//!
//! Reads the Masamune `localize.yaml` format used by Flutter apps:
//!
//! ```yaml
//! localize:
//!   - key: "Hello {name}"
//!     en_US: "Hello {name}"
//!     ja_JP: "こんにちは {name}"
//! ```
//!
//! and writes `OUT_DIR/localize.rs` with a `Localize` type whose methods follow the Flutter
//! generator's naming (`l().helloWorld()`; placeholders become arguments):
//!
//! ```ignore
//! // build.rs
//! fn main() {
//!     masamune_whisker_localize_build::LocalizeBuilder::new(&["en_US", "ja_JP"])
//!         .file("../localize.base.yaml")
//!         .file("../localize.app.yaml")
//!         .generate()
//!         .unwrap();
//! }
//!
//! // src/i18n.rs
//! include!(concat!(env!("OUT_DIR"), "/localize.rs"));
//! // prepare(): i18n::register();  then l().helloWorld()
//! ```
//!
//! The generated code reads the current language from `masamune_whisker_appearance::locale`
//! (configurable with [`LocalizeBuilder::runtime_path`]).

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde::Deserialize;

const RESERVED: [&str; 12] = [
    "in", "do", "is", "new", "default", "class", "switch", "case", "for", "type", "use", "match",
];

#[derive(Deserialize)]
struct File {
    localize: Vec<BTreeMap<String, String>>,
}

fn camel(segment: &str) -> String {
    let words: Vec<&str> = segment
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    if words.is_empty() {
        return String::new();
    }
    if words.len() == 1 && words[0].chars().all(|c| !c.is_ascii_lowercase()) {
        return words[0].to_ascii_lowercase();
    }
    let mut out = String::new();
    for (i, word) in words.iter().enumerate() {
        let mut chars = word.chars();
        let Some(first) = chars.next() else { continue };
        if i == 0 {
            out.push(first.to_ascii_lowercase());
        } else {
            out.push(first.to_ascii_uppercase());
        }
        out.extend(chars);
    }
    out
}

fn split_placeholders(key: &str) -> (Vec<String>, Vec<String>) {
    let mut segments = Vec::new();
    let mut params = Vec::new();
    let mut current = String::new();
    let mut chars = key.chars();
    while let Some(c) = chars.next() {
        if c == '{' {
            let mut name = String::new();
            for n in chars.by_ref() {
                if n == '}' {
                    break;
                }
                name.push(n);
            }
            segments.push(std::mem::take(&mut current));
            if !params.contains(&name) {
                params.push(name);
            }
        } else {
            current.push(c);
        }
    }
    segments.push(current);
    (segments, params)
}

/// Method name generated for `key` (same rules as the Flutter generator):
/// the words before the first placeholder (or of the next segment when the key starts with one)
/// in camelCase; a single all-caps word becomes lowercase (`"OK"` → `ok`).
pub fn method_name(key: &str) -> String {
    // A literal `\n` (backslash + n) is not part of a word.
    let key = key.replace("\\n", " ");
    let (segments, _) = split_placeholders(&key);
    let segment = segments
        .iter()
        .find(|s| !camel(s).is_empty())
        .cloned()
        .unwrap_or_default();
    let mut name = camel(&segment);
    if name.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        name = format!("_{name}");
    }
    if RESERVED.contains(&name.as_str()) {
        name.push('_');
    }
    name
}

fn param_ident(name: &str) -> String {
    let ident = camel(name);
    let ident = if ident.is_empty() {
        "value".to_string()
    } else {
        ident
    };
    match ident.as_str() {
        "do" | "type" | "match" | "in" | "use" => format!("{ident}_"),
        _ => ident,
    }
}

fn unescape(value: &str) -> String {
    value.replace("\\n", "\n")
}

/// Generates `localize.rs` from localize YAML files.
#[derive(Debug, Clone)]
pub struct LocalizeBuilder {
    languages: Vec<String>,
    files: Vec<PathBuf>,
    runtime_path: String,
    output_name: String,
}

impl LocalizeBuilder {
    /// `languages` are the YAML columns (`["en_US", "ja_JP", …]`); the first is the fallback.
    pub fn new(languages: &[&str]) -> Self {
        Self {
            languages: languages.iter().map(|l| (*l).to_owned()).collect(),
            files: Vec::new(),
            runtime_path: "::masamune_whisker_appearance::locale".into(),
            output_name: "localize.rs".into(),
        }
    }

    /// Adds a YAML file (relative paths are resolved against `CARGO_MANIFEST_DIR`). Later files
    /// override earlier keys.
    pub fn file(mut self, path: impl AsRef<Path>) -> Self {
        self.files.push(path.as_ref().to_path_buf());
        self
    }

    /// Module providing `language_index()` and `register_languages()`.
    pub fn runtime_path(mut self, path: impl Into<String>) -> Self {
        self.runtime_path = path.into();
        self
    }

    /// File name written into `OUT_DIR` (default `localize.rs`).
    pub fn output_name(mut self, name: impl Into<String>) -> Self {
        self.output_name = name.into();
        self
    }

    /// Generated source for the given YAML contents (no file system access).
    pub fn render(&self, sources: &[&str]) -> Result<String, String> {
        let langs = &self.languages;
        let mut entries: BTreeMap<String, (String, Vec<String>)> = BTreeMap::new();
        for source in sources {
            let file: File = serde_yaml::from_str(source).map_err(|e| e.to_string())?;
            for item in file.localize {
                let Some(key) = item.get("key").cloned() else {
                    continue;
                };
                let name = method_name(&key);
                if name.is_empty() {
                    continue;
                }
                let fallback = item.get(&langs[0]).cloned().unwrap_or_else(|| key.clone());
                let values = langs
                    .iter()
                    .map(|lang| unescape(item.get(lang).unwrap_or(&fallback)))
                    .collect::<Vec<_>>();
                entries.insert(name, (key, values));
            }
        }

        let n = langs.len();
        let rt = &self.runtime_path;
        let mut out = String::new();
        let w = &mut out;
        let _ = writeln!(
            w,
            "// Generated by masamune-whisker-localize-build. Do not edit."
        );
        let _ = writeln!(w, "pub const LANGUAGES: [&str; {n}] = {langs:?};");
        let _ = writeln!(w, "/// Strings of the current language. Get it with [`l`].");
        let _ = writeln!(w, "#[derive(Clone, Copy, Debug, PartialEq, Eq)]");
        let _ = writeln!(w, "pub struct Localize {{ index: usize }}");
        let _ = writeln!(
            w,
            "/// Registers [`LANGUAGES`] with the appearance runtime. Call once at startup."
        );
        let _ = writeln!(
            w,
            "pub fn register() {{ {rt}::register_languages(&LANGUAGES); }}"
        );
        let _ = writeln!(w, "/// Strings of the current language.");
        let _ = writeln!(
            w,
            "pub fn l() -> Localize {{ Localize {{ index: {rt}::language_index().min(LANGUAGES.len() - 1) }} }}"
        );
        let _ = writeln!(w, "#[allow(non_snake_case, dead_code)]");
        let _ = writeln!(w, "impl Localize {{");
        // Helper names contain `_`, which generated (camelCase) names never do, so they cannot clash.
        let _ = writeln!(w, "    fn table_index(&self) -> usize {{ self.index }}");
        let _ = writeln!(w, "    /// Language tag (`ja_JP`).");
        let _ = writeln!(
            w,
            "    pub fn language_tag(&self) -> &'static str {{ LANGUAGES[self.index] }}"
        );
        let _ = writeln!(w, "    /// Language code (`ja`).");
        let _ = writeln!(
            w,
            "    pub fn language_code(&self) -> &'static str {{ LANGUAGES[self.index].split('_').next().unwrap_or(\"en\") }}"
        );
        let _ = writeln!(w, "    /// Country code (`JP`).");
        let _ = writeln!(
            w,
            "    pub fn country_code(&self) -> &'static str {{ LANGUAGES[self.index].split_once('_').map_or(\"\", |(_, c)| c) }}"
        );
        for (name, (key, values)) in &entries {
            let (_, params) = split_placeholders(key);
            let table = values
                .iter()
                .map(|v| format!("{v:?}"))
                .collect::<Vec<_>>()
                .join(", ");
            let _ = writeln!(w, "    /// {}", key.replace('\n', " "));
            if params.is_empty() {
                let _ = writeln!(w, "    pub fn {name}(&self) -> &'static str {{");
                let _ = writeln!(w, "        const T: [&str; {n}] = [{table}];");
                let _ = writeln!(w, "        T[self.table_index()]");
                let _ = writeln!(w, "    }}");
            } else {
                let args = params
                    .iter()
                    .map(|p| format!("{}: impl std::fmt::Display", param_ident(p)))
                    .collect::<Vec<_>>()
                    .join(", ");
                let _ = writeln!(w, "    pub fn {name}(&self, {args}) -> String {{");
                let _ = writeln!(w, "        const T: [&str; {n}] = [{table}];");
                let _ = writeln!(w, "        let mut s = T[self.table_index()].to_string();");
                for p in &params {
                    let _ = writeln!(
                        w,
                        "        s = s.replace({:?}, &{}.to_string());",
                        format!("{{{p}}}"),
                        param_ident(p)
                    );
                }
                let _ = writeln!(w, "        s");
                let _ = writeln!(w, "    }}");
            }
        }
        let _ = writeln!(w, "}}");
        Ok(out)
    }

    /// Reads the files, writes `OUT_DIR/<output_name>` and prints `rerun-if-changed`.
    pub fn generate(self) -> std::io::Result<()> {
        let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default());
        let mut sources = Vec::new();
        for path in &self.files {
            let path = if path.is_relative() {
                manifest.join(path)
            } else {
                path.clone()
            };
            println!("cargo:rerun-if-changed={}", path.display());
            sources.push(std::fs::read_to_string(&path).map_err(|e| {
                std::io::Error::new(e.kind(), format!("read {}: {e}", path.display()))
            })?);
        }
        let sources: Vec<&str> = sources.iter().map(String::as_str).collect();
        let out = self
            .render(&sources)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let out_dir = PathBuf::from(
            std::env::var("OUT_DIR").map_err(|e| std::io::Error::other(e.to_string()))?,
        );
        std::fs::write(out_dir.join(&self.output_name), out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_methods_like_flutter() {
        assert_eq!(method_name("OK"), "ok");
        assert_eq!(method_name("App Title"), "appTitle");
        assert_eq!(method_name("Today's NANSURU"), "todaySNANSURU");
        assert_eq!(method_name("{count} days left"), "daysLeft");
        assert_eq!(method_name("Hello {name}, welcome"), "hello");
        assert_eq!(method_name("New"), "new_");
        assert_eq!(method_name("3 days"), "_3Days");
        assert_eq!(method_name("Line\\nBreak"), "lineBreak");
    }

    #[test]
    fn renders_tables() {
        let yaml = r#"
localize:
  - key: "Hello {name}"
    en_US: "Hello {name}"
    ja_JP: "こんにちは{name}"
  - key: "OK"
    en_US: "Okay"
"#;
        let out = LocalizeBuilder::new(&["en_US", "ja_JP"])
            .render(&[yaml])
            .unwrap();
        assert!(out.contains("pub const LANGUAGES: [&str; 2] = [\"en_US\", \"ja_JP\"];"));
        assert!(out.contains("pub fn hello(&self, name: impl std::fmt::Display) -> String"));
        assert!(out.contains("const T: [&str; 2] = [\"Okay\", \"Okay\"];"));
        assert!(out.contains("::masamune_whisker_appearance::locale::language_index()"));
    }
}
