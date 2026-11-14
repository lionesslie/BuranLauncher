use crate::http;
use serde_json::{json, Value};

const JAVA_PATCH_NOTES_URL: &str = "https://launchercontent.mojang.com/v2/javaPatchNotes.json";
const IMAGE_BASE: &str = "https://launchercontent.mojang.com";

/// Minecraft'ın resmi Java sürüm notlarını (patch/snapshot) getirir.
/// Feed: launchercontent.mojang.com (Minecraft Launcher'ın kendi kullandığı kaynak).
pub async fn fetch_java_patch_notes(limit: usize) -> Result<Value, String> {
    let response = http::get_json(JAVA_PATCH_NOTES_URL).await?;
    let entries = response
        .get("entries")
        .and_then(Value::as_array)
        .ok_or_else(|| "Sürüm notu listesi beklenen biçimde değil".to_string())?;
    let items: Vec<Value> = entries
        .iter()
        .take(limit)
        .filter_map(|entry| {
            let title = entry.get("title").and_then(Value::as_str)?;
            let date = entry.get("date").and_then(Value::as_str).unwrap_or("");
            let short_text = entry
                .get("shortText")
                .and_then(Value::as_str)
                .unwrap_or("")
                .replace("\\r\\n", "\n");
            let image = entry
                .pointer("/image/url")
                .and_then(Value::as_str)
                .map(|url| format!("{IMAGE_BASE}{url}"));
            Some(json!({
                "id": entry.get("id").cloned().unwrap_or(Value::Null),
                "title": title,
                "version": entry.get("version").cloned().unwrap_or(Value::Null),
                "type": entry.get("type").cloned().unwrap_or(Value::Null),
                "date": date,
                "shortText": short_text,
                "image": image,
            }))
        })
        .collect();
    Ok(json!(items))
}
