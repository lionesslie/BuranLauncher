use crate::paths::AppPaths;
use crate::store::Store;
use base64::Engine;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

fn mime_for(path: &Path) -> Option<&'static str> {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_lowercase()
        .as_str()
    {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "webp" => Some("image/webp"),
        "gif" => Some("image/gif"),
        _ => None,
    }
}

fn file_to_data_url(path: &Path) -> Option<String> {
    let mime = mime_for(path)?;
    let bytes = fs::read(path).ok()?;
    Some(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

pub fn get_background_state(store: &Store, paths: &AppPaths) -> Value {
    let settings = store.settings();
    let enabled = settings
        .get("backgroundEnabled")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let configured = settings
        .get("backgroundPath")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(std::path::PathBuf::from);
    let target = configured
        .filter(|path| path.exists())
        .or_else(|| paths.default_background());
    let data_url = if enabled {
        target.and_then(|path| file_to_data_url(&path))
    } else {
        None
    };
    json!({ "enabled": enabled, "dataUrl": data_url })
}

pub fn set_custom_background(
    store: &Store,
    paths: &AppPaths,
    source_path: &Path,
) -> Result<Value, String> {
    let mime = mime_for(source_path)
        .ok_or_else(|| format!(
            "Desteklenmeyen görsel formatı: {}",
            source_path
                .extension()
                .and_then(|extension| extension.to_str())
                .unwrap_or("bilinmiyor")
        ))?;
    let extension = source_path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("png")
        .to_lowercase();
    let destination = paths.backgrounds_dir().join(format!("custom.{extension}"));
    fs::copy(source_path, &destination)
        .map_err(|err| format!("Arkaplan kopyalanamadı: {err}"))?;
    store.update_settings(&json!({
        "backgroundPath": destination.to_string_lossy().to_string()
    }));
    let _ = mime;
    Ok(get_background_state(store, paths))
}

pub fn reset_background(store: &Store, paths: &AppPaths) -> Value {
    store.update_settings(&json!({ "backgroundPath": Value::Null }));
    get_background_state(store, paths)
}