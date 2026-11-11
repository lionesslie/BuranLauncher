use crate::instance_manager;
use crate::paths::AppPaths;
use base64::Engine;
use serde_json::{json, Value};
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

fn saves_dir(paths: &AppPaths, instance_id: &str) -> PathBuf {
    paths.instance_dir(instance_id).join("saves")
}

fn folder_size(path: &Path) -> u64 {
    let Ok(entries) = fs::read_dir(path) else {
        return 0;
    };
    let mut total = 0u64;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            total += folder_size(&path);
        } else if let Ok(metadata) = entry.metadata() {
            total += metadata.len();
        }
    }
    total
}

pub fn list_worlds(paths: &AppPaths, instance_id: &str) -> Value {
    let saves = saves_dir(paths, instance_id);
    let Ok(entries) = fs::read_dir(&saves) else {
        return Value::Array(Vec::new());
    };
    let mut worlds = Vec::new();
    for entry in entries.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let folder_name = entry.file_name().to_string_lossy().to_string();
        let world_dir = entry.path();
        let icon = fs::read(world_dir.join("icon.png"))
            .ok()
            .map(|bytes| {
                format!(
                    "data:image/png;base64,{}",
                    base64::engine::general_purpose::STANDARD.encode(bytes)
                )
            });
        let last_modified = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_millis() as u64);
        worlds.push(json!({
            "folderName": folder_name,
            "name": folder_name,
            "icon": icon,
            "sizeBytes": folder_size(&world_dir),
            "lastModified": last_modified
        }));
    }
    worlds.sort_by(|a, b| {
        b.get("lastModified")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            .cmp(&a.get("lastModified").and_then(Value::as_u64).unwrap_or(0))
    });
    Value::Array(worlds)
}

pub fn delete_world(paths: &AppPaths, instance_id: &str, folder_name: &str) -> Result<(), String> {
    let target = saves_dir(paths, instance_id).join(folder_name);
    if !target.exists() {
        return Err("Dünya bulunamadı".to_string());
    }
    fs::remove_dir_all(target).map_err(|err| format!("Dünya silinemedi: {err}"))
}

pub fn open_world_folder(
    paths: &AppPaths,
    instance_id: &str,
    folder_name: &str,
) -> Result<(), String> {
    let target = saves_dir(paths, instance_id).join(folder_name);
    if !target.exists() {
        return Err("Dünya bulunamadı".to_string());
    }
    open::that(target).map_err(|err| format!("Klasör açılamadı: {err}"))
}

fn add_world_to_zip(
    zip: &mut ZipWriter<File>,
    root: &Path,
    prefix: &str,
    options: SimpleFileOptions,
) -> Result<(), String> {
    for entry in fs::read_dir(root)
        .map_err(|err| format!("Klasör okunamadı: {err}"))?
        .flatten()
    {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        let zip_name = format!("{prefix}/{name}");
        if path.is_dir() {
            zip.add_directory(&zip_name, options)
                .map_err(|err| format!("Arşive klasör eklenemedi: {err}"))?;
            add_world_to_zip(zip, &path, &zip_name, options)?;
        } else if path.is_file() {
            zip.start_file(&zip_name, options)
                .map_err(|err| format!("Arşive dosya eklenemedi: {err}"))?;
            let mut file = File::open(&path).map_err(|err| format!("Dosya açılamadı: {err}"))?;
            std::io::copy(&mut file, zip).map_err(|err| format!("Arşive yazılamadı: {err}"))?;
        }
    }
    Ok(())
}

pub fn backup_world(
    paths: &AppPaths,
    instance_id: &str,
    folder_name: &str,
    destination: &Path,
) -> Result<String, String> {
    let world_dir = saves_dir(paths, instance_id).join(folder_name);
    if !world_dir.exists() {
        return Err("Dünya bulunamadı".to_string());
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("Klasör oluşturulamadı: {err}"))?;
    }
    let file = File::create(destination).map_err(|err| format!("Arşiv oluşturulamadı: {err}"))?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    add_world_to_zip(&mut zip, &world_dir, folder_name, options)?;
    zip.finish()
        .map_err(|err| format!("Arşiv tamamlanamadı: {err}"))?;
    Ok(destination.to_string_lossy().to_string())
}

pub fn import_world(paths: &AppPaths, instance_id: &str, zip_path: &Path) -> Result<String, String> {
    let saves = saves_dir(paths, instance_id);
    fs::create_dir_all(&saves).map_err(|err| format!("Klasör oluşturulamadı: {err}"))?;
    let file = File::open(zip_path).map_err(|err| format!("Arşiv açılamadı: {err}"))?;
    let mut archive = ZipArchive::new(file).map_err(|err| format!("Geçersiz zip: {err}"))?;

    let mut root_dirs = std::collections::HashSet::new();
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|err| format!("Arşiv girdisi okunamadı: {err}"))?;
        if let Some(name) = entry.enclosed_name() {
            let parts: Vec<_> = name.components().collect();
            if parts.len() > 1 {
                if let Some(first) = parts.first() {
                    root_dirs.insert(first.as_os_str().to_string_lossy().to_string());
                }
            }
        }
    }

    let zip_stem = zip_path
        .file_stem()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "Dünya".to_string());
    let strip_root = root_dirs.len() == 1;
    let base_name = if strip_root {
        root_dirs.into_iter().next().unwrap_or(zip_stem)
    } else {
        zip_stem
    };
    let mut final_name = base_name.clone();
    let mut counter = 1;
    while saves.join(&final_name).exists() {
        final_name = format!("{base_name} ({counter})");
        counter += 1;
    }
    let destination = saves.join(&final_name);
    fs::create_dir_all(&destination).map_err(|err| format!("Klasör oluşturulamadı: {err}"))?;

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|err| format!("Arşiv girdisi okunamadı: {err}"))?;
        if entry.is_dir() {
            continue;
        }
        let Some(enclosed) = entry.enclosed_name() else {
            continue;
        };
        let components: Vec<_> = enclosed.components().collect();
        let relative = if strip_root {
            if components.len() <= 1 {
                continue;
            }
            let mut path = PathBuf::new();
            for component in &components[1..] {
                path.push(component.as_os_str());
            }
            path
        } else {
            PathBuf::from(enclosed.as_os_str())
        };
        if relative.as_os_str().is_empty() {
            continue;
        }
        let target = destination.join(&relative);
        if !instance_manager::is_safe_child(&destination, &target) {
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|err| format!("Klasör oluşturulamadı: {err}"))?;
        }
        let mut output = File::create(&target).map_err(|err| format!("Dosya yazılamadı: {err}"))?;
        std::io::copy(&mut entry, &mut output)
            .map_err(|err| format!("Dosya çıkarılamadı: {err}"))?;
    }
    crate::logger::info(&format!(
        "Dünya içe aktarıldı: {} (instance {instance_id})",
        final_name
    ));
    Ok(final_name)
}