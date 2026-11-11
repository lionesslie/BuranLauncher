use crate::http::now_millis;
use crate::paths::AppPaths;
use crate::store::Store;
use serde_json::{json, Value};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use uuid::Uuid;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};



pub fn list_instances(store: &Store) -> Value {
    Value::Array(store.array("instances"))
}

pub fn get_instance(store: &Store, id: &str) -> Result<Value, String> {
    store
        .array("instances")
        .into_iter()
        .find(|item| item.get("id").and_then(Value::as_str) == Some(id))
        .ok_or_else(|| format!("Profil bulunamadı: {id}"))
}

pub fn create_instance(store: &Store, paths: &AppPaths, data: &Value) -> Result<Value, String> {
    let name = data
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if name.is_empty() {
        return Err("Profil adı boş olamaz".to_string());
    }
    let version = data
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if version.is_empty() {
        return Err("Minecraft sürümü seçilmeli".to_string());
    }

    let instance = json!({
        "id": Uuid::new_v4().to_string(),
        "name": name,
        "icon": data.get("icon").and_then(Value::as_str).unwrap_or("grass"),
        "version": version,
        "versionType": data.get("versionType").and_then(Value::as_str).unwrap_or("release"),
        "loader": data.get("loader").and_then(Value::as_str).unwrap_or("vanilla"),
        "loaderVersion": data.get("loaderVersion").cloned().unwrap_or(Value::Null),
        "createdAt": now_millis(),
        "lastPlayed": Value::Null,
        "totalPlaytimeMs": 0
    });

    let id = instance["id"].as_str().unwrap_or_default().to_string();
    paths.ensure_instance_folders(&id);
    store.push_array("instances", instance.clone());
    crate::logger::info(&format!("Yeni instance oluşturuldu: {} ({})", name, id));
    Ok(instance)
}

pub fn update_instance(store: &Store, id: &str, patch: &Value) -> Result<Value, String> {
    let mut instances = store.array("instances");
    let index = instances
        .iter()
        .position(|item| item.get("id").and_then(Value::as_str) == Some(id))
        .ok_or_else(|| format!("Profil bulunamadı: {id}"))?;

    let mut updated = instances[index].clone();
    if let (Some(target), Some(patch_map)) = (updated.as_object_mut(), patch.as_object()) {
        for (key, value) in patch_map {
            target.insert(key.clone(), value.clone());
        }
    }
    updated["id"] = Value::String(id.to_string());
    instances[index] = updated.clone();
    store.set("instances", Value::Array(instances));
    Ok(updated)
}

pub fn duplicate_instance(
    store: &Store,
    paths: &AppPaths,
    id: &str,
    new_name: Option<String>,
) -> Result<Value, String> {
    let original = get_instance(store, id)?;
    let name = new_name.unwrap_or_else(|| {
        format!(
            "{} (Kopya)",
            original
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("Profil")
        )
    });
    let copy = create_instance(
        store,
        paths,
        &json!({
            "name": name,
            "icon": original.get("icon").cloned().unwrap_or(json!("grass")),
            "version": original.get("version").cloned().unwrap_or(json!("")),
            "versionType": original.get("versionType").cloned().unwrap_or(json!("release")),
            "loader": original.get("loader").cloned().unwrap_or(json!("vanilla")),
            "loaderVersion": original.get("loaderVersion").cloned().unwrap_or(Value::Null)
        }),
    )?;

    let source = paths.instance_dir(id);
    let copy_id = copy["id"].as_str().unwrap_or_default().to_string();
    let destination = paths.instance_dir(&copy_id);
    copy_dir_recursive(&source, &destination)?;
    Ok(copy)
}

pub fn delete_instance(store: &Store, paths: &AppPaths, id: &str) -> Result<(), String> {
    store.map_array("instances", |instances| {
        instances.retain(|item| item.get("id").and_then(Value::as_str) != Some(id));
    });
    let dir = paths.instance_dir(id);
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(|err| format!("Profil klasörü silinemedi: {err}"))?;
    }
    crate::logger::info(&format!("Instance silindi: {id}"));
    Ok(())
}

pub fn record_play_session(store: &Store, id: &str, duration_ms: u64) {
    let mut instances = store.array("instances");
    if let Some(index) = instances
        .iter()
        .position(|item| item.get("id").and_then(Value::as_str) == Some(id))
    {
        let previous = instances[index]
            .get("totalPlaytimeMs")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        instances[index]["lastPlayed"] = json!(now_millis());
        instances[index]["totalPlaytimeMs"] = json!(previous + duration_ms);
        store.set("instances", Value::Array(instances));
    }
}

pub fn export_instance(
    store: &Store,
    paths: &AppPaths,
    id: &str,
    destination: &Path,
) -> Result<String, String> {
    let instance = get_instance(store, id)?;
    let dir = paths.instance_dir(id);
    let file = File::create(destination).map_err(|err| format!("Arşiv oluşturulamadı: {err}"))?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    add_directory_to_zip(&mut zip, &dir, "data", options)?;
    zip.start_file("instance.json", options)
        .map_err(|err| format!("Arşive yazılamadı: {err}"))?;
    let meta = serde_json::to_string_pretty(&instance).unwrap_or_default();
    zip.write_all(meta.as_bytes())
        .map_err(|err| format!("Arşive yazılamadı: {err}"))?;
    zip.finish()
        .map_err(|err| format!("Arşiv tamamlanamadı: {err}"))?;
    Ok(destination.to_string_lossy().to_string())
}

pub fn import_instance(store: &Store, paths: &AppPaths, zip_path: &Path) -> Result<Value, String> {
    let file = File::open(zip_path).map_err(|err| format!("Arşiv açılamadı: {err}"))?;
    let mut archive = ZipArchive::new(file).map_err(|err| format!("Geçersiz zip: {err}"))?;

    let meta: Value = {
        let mut entry = archive
            .by_name("instance.json")
            .map_err(|_| "Geçersiz profil arşivi: instance.json bulunamadı".to_string())?;
        let mut text = String::new();
        entry
            .read_to_string(&mut text)
            .map_err(|err| format!("instance.json okunamadı: {err}"))?;
        serde_json::from_str(&text).map_err(|err| format!("instance.json ayrıştırılamadı: {err}"))?
    };

    let mut create_payload = meta.clone();
    create_payload["name"] = json!(format!(
        "{} (İçe Aktarıldı)",
        meta.get("name").and_then(Value::as_str).unwrap_or("Profil")
    ));
    let created = create_instance(store, paths, &create_payload)?;
    let instance_id = created["id"].as_str().unwrap_or_default().to_string();
    let destination = paths.instance_dir(&instance_id);

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
        if components.len() < 2 || components.first().and_then(|c| c.as_os_str().to_str()) != Some("data") {
            continue;
        }
        let mut relative = PathBuf::new();
        for component in &components[1..] {
            relative.push(component.as_os_str());
        }
        let target = destination.join(relative);
        if !is_safe_child(&destination, &target) {
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|err| format!("Klasör oluşturulamadı: {err}"))?;
        }
        let mut output =
            File::create(&target).map_err(|err| format!("Dosya yazılamadı: {err}"))?;
        std::io::copy(&mut entry, &mut output)
            .map_err(|err| format!("Dosya kopyalanamadı: {err}"))?;
    }

    Ok(created)
}

fn add_directory_to_zip(
    zip: &mut ZipWriter<File>,
    root: &Path,
    prefix: &str,
    options: SimpleFileOptions,
) -> Result<(), String> {
    if !root.exists() {
        return Ok(());
    }
    let entries = fs::read_dir(root).map_err(|err| format!("Klasör okunamadı: {err}"))?;
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        let zip_name = format!("{prefix}/{name}");
        if path.is_dir() {
            zip.add_directory(&zip_name, options)
                .map_err(|err| format!("Arşive klasör eklenemedi: {err}"))?;
            add_directory_to_zip(zip, &path, &zip_name, options)?;
        } else if path.is_file() {
            zip.start_file(&zip_name, options)
                .map_err(|err| format!("Arşive dosya eklenemedi: {err}"))?;
            let mut file = File::open(&path).map_err(|err| format!("Dosya açılamadı: {err}"))?;
            std::io::copy(&mut file, zip)
                .map_err(|err| format!("Arşive yazılamadı: {err}"))?;
        }
    }
    Ok(())
}

pub fn copy_dir_recursive(source: &Path, destination: &Path) -> Result<(), String> {
    if !source.exists() {
        return Ok(());
    }
    fs::create_dir_all(destination).map_err(|err| format!("Klasör oluşturulamadı: {err}"))?;
    for entry in fs::read_dir(source)
        .map_err(|err| format!("Klasör okunamadı: {err}"))?
        .flatten()
    {
        let path = entry.path();
        let target = destination.join(entry.file_name());
        if path.is_dir() {
            copy_dir_recursive(&path, &target)?;
        } else if path.is_file() {
            fs::copy(&path, &target)
                .map_err(|err| format!("Dosya kopyalanamadı: {err}"))?;
        }
    }
    Ok(())
}

pub fn is_safe_child(root: &Path, target: &Path) -> bool {
    let normalized_root = root.to_path_buf();
    let normalized_target = target.to_path_buf();
    normalized_target
        .components()
        .all(|component| !matches!(component, Component::ParentDir))
        && normalized_target.starts_with(normalized_root)
}