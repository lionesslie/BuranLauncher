use crate::http;
use serde_json::Value;
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

const MANIFEST_URL: &str = "https://launchermeta.mojang.com/mc/game/version_manifest_v2.json";
const CACHE_TTL: Duration = Duration::from_secs(10 * 60);

fn manifest_cache() -> &'static Mutex<Option<(Instant, Value)>> {
    static CACHE: OnceLock<Mutex<Option<(Instant, Value)>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

pub async fn fetch_version_manifest(force: bool) -> Result<Value, String> {
    let cache = manifest_cache();
    if !force {
        let guard = cache.lock().await;
        if let Some((cached_at, manifest)) = guard.as_ref() {
            if cached_at.elapsed() < CACHE_TTL {
                return Ok(manifest.clone());
            }
        }
    }

    match http::get_json(MANIFEST_URL).await {
        Ok(manifest) => {
            let mut guard = cache.lock().await;
            *guard = Some((Instant::now(), manifest.clone()));
            Ok(manifest)
        }
        Err(err) => {
            crate::logger::error(&format!("Sürüm manifesti indirilemedi: {err}"));
            let guard = cache.lock().await;
            if let Some((_, manifest)) = guard.as_ref() {
                Ok(manifest.clone())
            } else {
                Err(format!("Manifest alınamadı: {err}"))
            }
        }
    }
}

pub async fn list_versions(version_type: &str) -> Result<Value, String> {
    let manifest = fetch_version_manifest(false).await?;
    let versions = manifest
        .get("versions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if version_type == "all" {
        return Ok(Value::Array(versions));
    }
    Ok(Value::Array(
        versions
            .into_iter()
            .filter(|item| item.get("type").and_then(Value::as_str) == Some(version_type))
            .collect(),
    ))
}

pub async fn get_version_detail(version_id: &str) -> Result<Value, String> {
    let manifest = fetch_version_manifest(false).await?;
    let entry = manifest
        .get("versions")
        .and_then(Value::as_array)
        .and_then(|versions| {
            versions
                .iter()
                .find(|item| item.get("id").and_then(Value::as_str) == Some(version_id))
        })
        .ok_or_else(|| format!("Sürüm bulunamadı: {version_id}"))?;
    let url = entry
        .get("url")
        .and_then(Value::as_str)
        .ok_or_else(|| "Sürüm URL'si bulunamadı".to_string())?;
    http::get_json(url)
        .await
        .map_err(|err| format!("Sürüm detayı alınamadı: {err}"))
}