//! Web Host implementation for `masamune-whisker-localize`: `navigator.languages` and the
//! `languagechange` event.

use std::cell::RefCell;

use whisker_web::wasm_bindgen::JsCast;
use whisker_web::wasm_bindgen::closure::Closure;
use whisker_web::{ModuleDefinition, WhiskerModule, WhiskerValue};

const MODULE_NAME: &str = "masamune-whisker-localize:Localize";
const CHANGED_EVENT: &str = "languagesChanged";

thread_local! {
    static LISTENER: RefCell<Option<Closure<dyn FnMut()>>> = const { RefCell::new(None) };
}

struct LocalizeModule;

#[whisker_web::WhiskerModule]
impl WhiskerModule for LocalizeModule {
    type Definition = ModuleDefinition;

    fn definition() -> Self::Definition {
        ModuleDefinition::new()
            .name(MODULE_NAME)
            .function("languages", |_, _| {
                WhiskerValue::String(serde_json::json!({ "value": locales() }).to_string())
            })
            .event(CHANGED_EVENT)
            .on_start_observing(CHANGED_EVENT, |emitter| {
                let send = {
                    let emitter = emitter.clone();
                    move || {
                        let payload = serde_json::json!({ "locales": locales() }).to_string();
                        emitter.emit(CHANGED_EVENT, WhiskerValue::String(payload));
                    }
                };
                LISTENER.with(|slot| {
                    if slot.borrow().is_some() {
                        return;
                    }
                    let Some(window) = web_sys::window() else {
                        return;
                    };
                    let listener = Closure::<dyn FnMut()>::new(send.clone());
                    if window
                        .add_event_listener_with_callback(
                            "languagechange",
                            listener.as_ref().unchecked_ref(),
                        )
                        .is_ok()
                    {
                        *slot.borrow_mut() = Some(listener);
                    }
                });
                send();
            })
            .on_stop_observing(CHANGED_EVENT, |_| {
                LISTENER.with(|slot| {
                    let Some(listener) = slot.borrow_mut().take() else {
                        return;
                    };
                    if let Some(window) = web_sys::window() {
                        let _ = window.remove_event_listener_with_callback(
                            "languagechange",
                            listener.as_ref().unchecked_ref(),
                        );
                    }
                });
            })
    }
}

/// Browser languages as `{language, country}`, most preferred first.
fn locales() -> Vec<serde_json::Value> {
    let Some(navigator) = web_sys::window().map(|window| window.navigator()) else {
        return Vec::new();
    };
    let mut tags: Vec<String> = navigator
        .languages()
        .iter()
        .filter_map(|value| value.as_string())
        .collect();
    if tags.is_empty() {
        tags.extend(navigator.language());
    }
    tags.iter()
        .map(|tag| {
            let (language, country) = split_tag(tag);
            serde_json::json!({ "language": language, "country": country })
        })
        .collect()
}

/// Splits a BCP 47 tag (`ja-JP`, `zh-Hant-TW`, `es-419`) into its language and region.
fn split_tag(tag: &str) -> (String, String) {
    let mut parts = tag.split(['-', '_']);
    let language = parts.next().unwrap_or_default().to_ascii_lowercase();
    let country = parts
        .find(|part| {
            (part.len() == 2 && part.chars().all(|c| c.is_ascii_alphabetic()))
                || (part.len() == 3 && part.chars().all(|c| c.is_ascii_digit()))
        })
        .map(str::to_ascii_uppercase)
        .unwrap_or_default();
    (language, country)
}

#[cfg(test)]
mod tests {
    use super::split_tag;

    #[test]
    fn splits_tags() {
        assert_eq!(split_tag("ja-JP"), ("ja".into(), "JP".into()));
        assert_eq!(split_tag("en"), ("en".into(), String::new()));
        assert_eq!(split_tag("zh-Hant-TW"), ("zh".into(), "TW".into()));
        assert_eq!(split_tag("es-419"), ("es".into(), "419".into()));
    }
}
