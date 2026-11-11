use crate::http;
use crate::paths::AppPaths;
use serde_json::{json, Value};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::{AppHandle, Emitter};
use zip::ZipArchive;

const ADOPTIUM_API: &str = "https://api.adoptium.net/v3";

pub fn get_platform_info() -> Result<(String, String), String> {
    let platform = match std::env::consts::OS {
        "windows" => "windows",
        "linux" => "linux",
        other => return Err(format!("Desteklenmeyen platform: {other}")),
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "aarch64",
        _ => "x64",
    };
    Ok((platform.to_string(), arch.to_string()))
}

pub fn detect_system_java() -> Option<String> {
    let mut command = if cfg!(windows) {
        let mut cmd = Command::new("where");
        cmd.arg("java");
        cmd
    } else {
        let mut cmd = Command::new("which");
        cmd.arg("java");
        cmd
    };

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }

    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(ToString::to_string)
}

pub fn required_java_version_for_mc(mc_version: &str) -> u32 {
    let parts: Vec<u32> = mc_version
        .split('.')
        .map(|part| part.parse::<u32>().unwrap_or(0))
        .collect();
    let major = *parts.first().unwrap_or(&1);
    let minor = *parts.get(1).unwrap_or(&0);
    if major == 1 && minor >= 20 {
        21
    } else if major == 1 && minor >= 18 {
        17
    } else if major == 1 && minor >= 17 {
        16
    } else {
        8
    }
}

fn java_executable(base: &Path) -> PathBuf {
    if cfg!(windows) {
        base.join("bin").join("java.exe")
    } else {
        base.join("bin").join("java")
    }
}

pub fn find_cached_java(paths: &AppPaths, major: u32) -> Option<PathBuf> {
    let install_dir = paths.java_root().join(format!("jdk-{major}"));
    if !install_dir.exists() {
        return None;
    }
    let entries = fs::read_dir(&install_dir).ok()?;
    for entry in entries.flatten() {
        if entry.path().is_dir() {
            let candidate = java_executable(&entry.path());
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    None
}

pub async fn download_java<F>(paths: &AppPaths, major: u32, progress: F) -> Result<PathBuf, String>
where
    F: Fn(Value),
{
    let (platform, arch) = get_platform_info()?;
    let install_dir = paths.java_install_dir(major);

    progress(json!({ "stage": "fetchingMeta" }));
    let meta_url = format!(
        "{ADOPTIUM_API}/assets/latest/{major}/hotspot?architecture={arch}&image_type=jdk&os={platform}&vendor=eclipse"
    );
    let assets = http::get_json(&meta_url)
        .await
        .map_err(|err| format!("Adoptium API hatası: {err}"))?;
    let asset = assets
        .as_array()
        .and_then(|items| items.first())
        .ok_or_else(|| format!("Java {major} için uygun paket bulunamadı"))?;
    let package = &asset["binary"]["package"];
    let download_url = package
        .get("link")
        .and_then(Value::as_str)
        .ok_or_else(|| "Java indirme bağlantısı bulunamadı".to_string())?;
    let package_name = package
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("jdk-archive.zip");
    let archive_path = install_dir.join(package_name);

    progress(json!({ "stage": "downloading" }));
    let bytes = http::download_bytes(download_url)
        .await
        .map_err(|err| format!("Java indirilemedi: {err}"))?;
    fs::write(&archive_path, bytes).map_err(|err| format!("Java arşivi yazılamadı: {err}"))?;

    progress(json!({ "stage": "extracting" }));
    if package_name.ends_with(".zip") {
        extract_zip(&archive_path, &install_dir)?;
    } else {
        extract_tar_gz(&archive_path, &install_dir)?;
    }
    let _ = fs::remove_file(&archive_path);

    if let Some(cached) = find_cached_java(paths, major) {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&cached, fs::Permissions::from_mode(0o755));
        }
        progress(json!({ "stage": "done" }));
        return Ok(cached);
    }

    Err("İndirilen Java paketi içinde java çalıştırılabilir dosyası bulunamadı".to_string())
}

pub async fn download_and_install_java(
    app: &AppHandle,
    paths: &AppPaths,
    major: u32,
) -> Result<String, String> {
    let java = download_java(paths, major, |payload| {
        let _ = app.emit("java:progress", payload);
    })
    .await?;
    Ok(java.to_string_lossy().to_string())
}

pub async fn ensure_java_for_version<F>(
    paths: &AppPaths,
    mc_version: &str,
    progress: F,
) -> Result<PathBuf, String>
where
    F: Fn(Value),
{
    let required = required_java_version_for_mc(mc_version);
    if let Some(cached) = find_cached_java(paths, required) {
        progress(json!({ "stage": "cached", "javaMajor": required }));
        return Ok(cached);
    }
    progress(json!({ "stage": "autoDownloadStart", "javaMajor": required }));
    download_java(paths, required, |payload| {
        let mut payload = payload;
        payload["javaMajor"] = json!(required);
        progress(payload);
    })
    .await
}

fn extract_zip(archive_path: &Path, destination: &Path) -> Result<(), String> {
    let file = File::open(archive_path).map_err(|err| format!("Java arşivi açılamadı: {err}"))?;
    let mut archive = ZipArchive::new(file).map_err(|err| format!("Geçersiz zip: {err}"))?;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|err| format!("Java arşiv girdisi okunamadı: {err}"))?;
        let Some(relative) = entry.enclosed_name() else {
            continue;
        };
        let target = destination.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&target).map_err(|err| format!("KLASÖR OLUŞTURULAMADI: {err}"))?;
        } else {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)
                    .map_err(|err| format!("Klasör oluşturulamadı: {err}"))?;
            }
            let mut output =
                File::create(&target).map_err(|err| format!("Dosya yazılamadı: {err}"))?;
            let mut buffer = [0u8; 8192];
            loop {
                let read = entry
                    .read(&mut buffer)
                    .map_err(|err| format!("Arşiv okunamadı: {err}"))?;
                if read == 0 {
                    break;
                }
                output
                    .write_all(&buffer[..read])
                    .map_err(|err| format!("Dosya yazılamadı: {err}"))?;
            }
        }
    }
    Ok(())
}

fn extract_tar_gz(archive_path: &Path, destination: &Path) -> Result<(), String> {
    let file = File::open(archive_path).map_err(|err| format!("Java arşivi açılamadı: {err}"))?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    archive
        .unpack(destination)
        .map_err(|err| format!("Java arşivi çıkarılamadı: {err}"))
}