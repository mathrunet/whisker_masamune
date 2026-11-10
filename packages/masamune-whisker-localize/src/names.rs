//! Names of languages written in the language itself, for language pickers.

/// Name of `tag` (`"ja"`, `"ja_JP"`, `"zh-Hant-TW"`) in that language, or `""` when unknown.
pub fn language_name(tag: &str) -> &'static str {
    let mut parts = tag.split(['_', '-']);
    let code = parts.next().unwrap_or(tag).to_ascii_lowercase();
    if code == "zh" {
        let traditional = parts
            .any(|part| ["hant", "tw", "hk", "mo"].contains(&part.to_ascii_lowercase().as_str()));
        return if traditional {
            "繁體中文"
        } else {
            "简体中文"
        };
    }
    match code.as_str() {
        "ar" => "العربية",
        "de" => "Deutsch",
        "en" => "English",
        "es" => "Español",
        "fr" => "Français",
        "hi" => "हिन्दी",
        "id" => "Bahasa Indonesia",
        "it" => "Italiano",
        "ja" => "日本語",
        "ko" => "한국어",
        "ms" => "Bahasa Melayu",
        "nl" => "Nederlands",
        "pl" => "Polski",
        "pt" => "Português",
        "ru" => "Русский",
        "sv" => "Svenska",
        "th" => "ไทย",
        "tr" => "Türkçe",
        "uk" => "Українська",
        "vi" => "Tiếng Việt",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_languages() {
        assert_eq!(language_name("ja"), "日本語");
        assert_eq!(language_name("en_US"), "English");
        assert_eq!(language_name("zh_CN"), "简体中文");
        assert_eq!(language_name("zh-Hant-TW"), "繁體中文");
        assert_eq!(language_name("xx"), "");
    }
}
