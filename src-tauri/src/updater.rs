use crate::http;
use futures_util::StreamExt;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;

const DEFAULT_REPO: &str = "lionesslie/BuranLauncher";

#[derive(Clone)]
struct PendingUpdate {
    version: String,
    asset_name: String,
    download_url: String,
    installer_path: Option<PathBuf>,
}

fn pending() -> &'static Mutex<Option<PendingUpdate>> {
    static PENDING: OnceLock<Mutex<Option<PendingUpdate>>> = OnceLock::new();
    PENDING.get_or_init(|| Mutex::new(None))
}

fn normalize_repo(value: &str) -> Option<String> {
    let trimmed = value.trim().trim_end_matches('/');
    if trimmed.is_empty() || trimmed.contains("BURAYA_GITHUB_KULLANICI_ADINIZI_YAZIN") {
        return None;
    }
    let repo = trimmed
        .strip_prefix("https://github.com/")
        .or_else(|| trimmed.strip_prefix("http://github.com/"))
        .or_else(|| trimmed.strip_prefix("github.com/"))
        .unwrap_or(trimmed)
        .trim_end_matches(".git");
    let mut parts = repo.split('/');
    let owner = parts.next()?;
    let name = parts.next()?;
    if owner.is_empty() || name.is_empty() || parts.next().is_some() {
        return None;
    }
    Some(format!("{owner}/{name}"))
}

fn repo() -> String {
    std::env::var("BURAN_GITHUB_REPO")
        .ok()
        .and_then(|value| normalize_repo(&value))
        .unwrap_or_else(|| DEFAULT_REPO.to_string())
}

fn emit_status(app: &AppHandle, payload: Value) {
    let _ = app.emit("updater:status", payload);
}

pub fn get_current_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

fn is_dev_or_unconfigured() -> bool {
    cfg!(debug_assertions) || repo().contains("BURAYA_GITHUB_KULLANICI_ADINIZI_YAZIN")
}

fn select_asset(release: &Value) -> Option<(String, String)> {
    let assets = release.get("assets").and_then(Value::as_array)?;
    let candidates: Vec<(String, String)> = assets
        .iter()
        .filter_map(|asset| {
            let name = asset.get("name").and_then(Value::as_str)?.to_string();
            let url = asset
                .get("browser_download_url")
                .and_then(Value::as_str)?
                .to_string();
            Some((name, url))
        })
        .collect();
    let lower_name = |name: &str| name.to_lowercase();
    let preferred: Vec<&str> = if cfg!(windows) {
        vec![".exe", ".msi"]
    } else if cfg!(target_os = "macos") {
        vec![".dmg"]
    } else {
        vec![".appimage", ".deb"]
    };
    for extension in preferred {
        if let Some(asset) = candidates
            .iter()
            .find(|(name, _)| lower_name(name).ends_with(extension))
        {
            return Some(asset.clone());
        }
    }
    candidates.first().cloned()
}

pub async fn check_for_updates(app: &AppHandle) -> Result<Value, String> {
    if is_dev_or_unconfigured() {
        return Ok(json!({ "dev": true }));
    }
    emit_status(app, json!({ "stage": "checking" }));
    let repo = repo();
    let url = format!("https://api.github.com/repos/{repo}/releases/latest");
    let release = match http::get_json(&url).await {
        Ok(release) => release,
        Err(err) => {
            let message = format!("Güncelleme kontrolü başarısız: {err}");
            emit_status(app, json!({ "stage": "error", "message": message }));
            return Err(message);
        }
    };

    let tag = release
        .get("tag_name")
        .and_then(Value::as_str)
        .ok_or_else(|| "Sürüm etiketi bulunamadı".to_string())?;
    let latest_version = semver::Version::parse(tag.trim_start_matches('v'))
        .map_err(|err| format!("Geçersiz sürüm etiketi ({tag}): {err}"))?;
    let current_version = semver::Version::parse(env!("CARGO_PKG_VERSION"))
        .map_err(|err| format!("Geçersiz mevcut sürüm: {err}"))?;
    if latest_version <= current_version {
        emit_status(app, json!({ "stage": "none" }));
        return Ok(json!({ "updateAvailable": false, "version": current_version.to_string() }));
    }

    let Some((asset_name, download_url)) = select_asset(&release) else {
        let message = "Bu sürüm için uygun kurulum dosyası bulunamadı".to_string();
        emit_status(app, json!({ "stage": "error", "message": message }));
        return Err(message);
    };

    let version = latest_version.to_string();
    *pending().lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(PendingUpdate {
        version: version.clone(),
        asset_name: asset_name.clone(),
        download_url: download_url.clone(),
        installer_path: None,
    });
    emit_status(
        app,
        json!({ "stage": "available", "version": version }),
    );
    Ok(json!({
        "updateAvailable": true,
        "version": version,
        "tag": tag,
        "assetName": asset_name,
        "downloadUrl": download_url,
        "releaseNotes": release.get("body").cloned().unwrap_or(Value::Null),
        "publishedAt": release.get("published_at").cloned().unwrap_or(Value::Null)
    }))
}

pub async fn download_update(app: &AppHandle) -> Result<Value, String> {
    if cfg!(debug_assertions) {
        return Ok(json!({ "dev": true }));
    }
    let update = pending()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
        .ok_or_else(|| "Önce güncelleme kontrolü yapın".to_string())?;

    let download_dir = std::env::temp_dir().join("BuranLauncher-Update");
    std::fs::create_dir_all(&download_dir)
        .map_err(|err| format!("Geçici klasör oluşturulamadı: {err}"))?;
    let destination = download_dir.join(&update.asset_name);
    let partial = destination.with_extension(format!(
        "{}.part",
        destination
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("download")
    ));

    let response = http::client()
        .get(&update.download_url)
        .send()
        .await
        .map_err(|err| format!("Güncelleme indirilemedi: {err}"))?;
    if !response.status().is_success() {
        let message = format!(
            "Güncelleme indirilemedi: HTTP {}",
            response.status().as_u16()
        );
        emit_status(app, json!({ "stage": "error", "message": message }));
        return Err(message);
    }
    let total = response.content_length().unwrap_or(0);
    let mut downloaded = 0u64;
    let mut stream = response.bytes_stream();
    let mut file = tokio::fs::File::create(&partial)
        .await
        .map_err(|err| format!("Güncelleme dosyası oluşturulamadı: {err}"))?;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|err| format!("Güncelleme indirilemedi: {err}"))?;
        file.write_all(&chunk)
            .await
            .map_err(|err| format!("Güncelleme yazılamadı: {err}"))?;
        downloaded += chunk.len() as u64;
        let percent = if total > 0 {
            ((downloaded as f64 / total as f64) * 100.0).round() as u64
        } else {
            0
        };
        emit_status(
            app,
            json!({ "stage": "downloading", "percent": percent }),
        );
    }
    file.flush()
        .await
        .map_err(|err| format!("Güncelleme dosyası kapatılamadı: {err}"))?;
    drop(file);
    tokio::fs::rename(&partial, &destination)
        .await
        .map_err(|err| format!("Güncelleme dosyası taşınamadı: {err}"))?;

    if let Some(update) = pending()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .as_mut()
    {
        update.installer_path = Some(destination.clone());
    }
    emit_status(
        app,
        json!({ "stage": "downloaded", "version": update.version }),
    );
    Ok(json!({ "path": destination.to_string_lossy().to_string() }))
}

pub fn quit_and_install(app: &AppHandle) -> Result<(), String> {
    let path = pending()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .as_ref()
        .and_then(|update| update.installer_path.clone())
        .ok_or_else(|| "Güncelleme dosyası indirilmedi".to_string())?;
    if !path.exists() {
        return Err("Güncelleme dosyası bulunamadı".to_string());
    }

    if cfg!(windows) {
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("")
            .to_lowercase();
        if extension == "msi" {
            std::process::Command::new("msiexec")
                .arg("/i")
                .arg(&path)
                .spawn()
                .map_err(|err| format!("Kurulum başlatılamadı: {err}"))?;
        } else {
            std::process::Command::new(&path)
                .spawn()
                .map_err(|err| format!("Kurulum başlatılamadı: {err}"))?;
        }
    } else {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755));
        }
        open::that(&path).map_err(|err| format!("Kurulum açılamadı: {err}"))?;
    }

    app.exit(0);
    Ok(())
}
pub async fn list_releases(limit: usize) -> Result<Value, String> {
    let url = format!(
        "https://api.github.com/repos/{}/releases?per_page={limit}",
        repo()
    );
    let response = http::client()
        .get(&url)
        .header("User-Agent", "BuranLauncher")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|err| format!("Sürüm notları alınamadı: {err}"))?;
    let releases: Vec<Value> = response
        .json()
        .await
        .map_err(|err| format!("Sürüm notları çözümlenemedi: {err}"))?;
    let items: Vec<Value> = releases
        .iter()
        .take(limit)
        .map(|release| {
            json!({
                "tag": release.get("tag_name").and_then(Value::as_str).unwrap_or(""),
                "name": release.get("name").and_then(Value::as_str).unwrap_or(""),
                "date": release.get("published_at").and_then(Value::as_str).unwrap_or(""),
                "body": release.get("body").and_then(Value::as_str).unwrap_or(""),
                "url": release.get("html_url").and_then(Value::as_str).unwrap_or("")
            })
        })
        .collect();
    Ok(json!({
        "repoUrl": format!("https://github.com/{}/releases", repo()),
        "items": items
    }))
}
