use crate::store::Store;
use serde_json::{json, Value};
use std::sync::OnceLock;

const SUPPORTED_LANGS: [&str; 2] = ["tr", "ru"];
const DEFAULT_LANG: &str = "tr";
const TR_JSON: &str = include_str!("../../locales/tr.json");
const RU_JSON: &str = include_str!("../../locales/ru.json");

fn translations_for(lang: &str) -> &'static Value {
    static TR: OnceLock<Value> = OnceLock::new();
    static RU: OnceLock<Value> = OnceLock::new();
    match lang {
        "ru" => RU.get_or_init(|| serde_json::from_str(RU_JSON).unwrap_or_else(|_| json!({}))),
        _ => TR.get_or_init(|| serde_json::from_str(TR_JSON).unwrap_or_else(|_| json!({}))),
    }
}

pub fn detect_system_lang() -> String {
    let locale = sys_locale::get_locale()
        .unwrap_or_default()
        .to_lowercase();
    if locale.starts_with("ru") {
        "ru".to_string()
    } else if locale.starts_with("tr") {
        "tr".to_string()
    } else {
        DEFAULT_LANG.to_string()
    }
}

pub fn current_lang(store: &Store) -> String {
    if let Some(saved) = store.get_string("language") {
        if SUPPORTED_LANGS.contains(&saved.as_str()) {
            return saved;
        }
    }
    detect_system_lang()
}

pub fn set_lang(store: &Store, lang: &str) -> Result<String, String> {
    if !SUPPORTED_LANGS.contains(&lang) {
        return Err(format!("Desteklenmeyen dil: {lang}"));
    }
    store.set_language(Some(lang.to_string()));
    Ok(lang.to_string())
}

pub fn get_translations(store: &Store) -> Value {
    let lang = current_lang(store);
    json!({
        "lang": lang,
        "translations": translations_for(&lang)
    })
}

