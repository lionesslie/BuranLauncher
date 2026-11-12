use crate::http;
use crate::instance_manager;
use crate::paths::AppPaths;
use crate::store::Store;
use serde_json::{json, Map, Value};
use std::collections::{HashSet, VecDeque};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use zip::ZipArchive;

const API_BASE: &str = "https://api.modrinth.com/v2";

/// Projenin gerçek görünen adını çeker (best-effort; hata olursa None döner).
async fn project_title(id_or_slug: &str) -> Option<String> {
    api_get(&format!("/project/{id_or_slug}"))
        .await
        .ok()
        .and_then(|project| {
            project
                .get("title")
                .and_then(Value::as_str)
                .map(ToString::to_string)
                .filter(|title| !title.trim().is_empty())
        })
}

async fn api_get(endpoint: &str) -> Result<Value, String> {
    http::get_json(&format!("{API_BASE}{endpoint}"))
        .await
        .map_err(|err| format!("Modrinth API hatası ({endpoint}): {err}"))
}

pub async fn search(
    query: &str,
    project_type: &str,
    loader: Option<&str>,
    game_version: Option<&str>,
    sort: &str,
    offset: u64,
    limit: u64,
) -> Result<Value, String> {
    let mut facets = vec![vec![format!("project_type:{project_type}")]];
    if let Some(loader) = loader.filter(|value| !value.is_empty()) {
        facets.push(vec![format!("categories:{loader}")]);
    }
    if let Some(version) = game_version.filter(|value| !value.is_empty()) {
        facets.push(vec![format!("versions:{version}")]);
    }
    let facets_json = serde_json::to_string(&facets).unwrap_or_else(|_| "[]".to_string());
    let mut url = reqwest::Url::parse(&format!("{API_BASE}/search"))
        .map_err(|err| format!("Geçersiz API adresi: {err}"))?;
    url.query_pairs_mut()
        .append_pair("query", query)
        .append_pair("facets", &facets_json)
        .append_pair("index", sort)
        .append_pair("offset", &offset.to_string())
        .append_pair("limit", &limit.to_string());
    http::get_json(url.as_str())
        .await
        .map_err(|err| format!("Modrinth araması başarısız: {err}"))
}

pub async fn get_project(id_or_slug: &str) -> Result<Value, String> {
    api_get(&format!("/project/{id_or_slug}")).await
}

async fn get_project_versions(
    id_or_slug: &str,
    loader: Option<&str>,
    game_version: Option<&str>,
) -> Result<Value, String> {
    let mut url = reqwest::Url::parse(&format!("{API_BASE}/project/{id_or_slug}/version"))
        .map_err(|err| format!("Geçersiz API adresi: {err}"))?;
    {
        let mut pairs = url.query_pairs_mut();
        if let Some(loader) = loader.filter(|value| !value.is_empty()) {
            let encoded = serde_json::to_string(&[loader]).unwrap_or_else(|_| "[]".to_string());
            pairs.append_pair("loaders", &encoded);
        }
        if let Some(version) = game_version.filter(|value| !value.is_empty()) {
            let encoded = serde_json::to_string(&[version]).unwrap_or_else(|_| "[]".to_string());
            pairs.append_pair("game_versions", &encoded);
        }
    }
    http::get_json(url.as_str())
        .await
        .map_err(|err| format!("Modrinth sürümleri alınamadı: {err}"))
}

pub async fn list_project_versions(
    id_or_slug: &str,
    loader: Option<&str>,
    game_version: Option<&str>,
) -> Result<Value, String> {
    get_project_versions(id_or_slug, loader, game_version).await
}

fn meta_path(paths: &AppPaths, instance_id: &str, project_type: &str) -> PathBuf {
    let filename = if project_type == "mod" {
        "mods-meta.json".to_string()
    } else {
        format!("{project_type}-meta.json")
    };
    paths.instance_dir(instance_id).join(filename)
}

fn load_meta(paths: &AppPaths, instance_id: &str, project_type: &str) -> Map<String, Value> {
    let path = meta_path(paths, instance_id, project_type);
    fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default()
}

fn save_meta(
    paths: &AppPaths,
    instance_id: &str,
    project_type: &str,
    meta: &Map<String, Value>,
) {
    let path = meta_path(paths, instance_id, project_type);
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(
        path,
        serde_json::to_string_pretty(&Value::Object(meta.clone())).unwrap_or_default(),
    );
}

fn content_folder(project_type: &str) -> Result<&'static str, String> {
    match project_type {
        "mod" => Ok("mods"),
        "resourcepack" => Ok("resourcepacks"),
        "shader" => Ok("shaderpacks"),
        _ => Err(format!("Bilinmeyen içerik türü: {project_type}")),
    }
}

async fn download_file(url: &str, destination: &Path, expected_sha1: Option<&str>) -> Result<(), String> {
    let bytes = http::download_bytes(url)
        .await
        .map_err(|err| format!("Dosya indirilemedi: {err} - {url}"))?;
    if let Some(expected) = expected_sha1 {
        let actual = http::sha1_hex(&bytes);
        if actual.to_lowercase() != expected.to_lowercase() {
            return Err(format!(
                "SHA1 doğrulaması başarısız: {}",
                destination
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_default()
            ));
        }
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("Klasör oluşturulamadı: {err}"))?;
    }
    fs::write(destination, bytes).map_err(|err| format!("Dosya yazılamadı: {err}"))
}

fn primary_file(version: &Value) -> Option<Value> {
    let files = version.get("files").and_then(Value::as_array)?;
    files
        .iter()
        .find(|file| file.get("primary").and_then(Value::as_bool).unwrap_or(false))
        .or_else(|| files.first())
        .cloned()
}

async fn resolve_version(
    instance: &Value,
    id_or_slug: &str,
    version_id: Option<&str>,
) -> Result<Value, String> {
    if let Some(version_id) = version_id.filter(|value| !value.is_empty()) {
        return api_get(&format!("/version/{version_id}")).await;
    }
    let loader = instance
        .get("loader")
        .and_then(Value::as_str)
        .filter(|loader| *loader != "vanilla");
    let game_version = instance.get("version").and_then(Value::as_str);
    let versions = get_project_versions(id_or_slug, loader, game_version).await?;
    versions
        .as_array()
        .and_then(|items| items.first())
        .cloned()
        .ok_or_else(|| {
            format!(
                "\"{id_or_slug}\" için {} / {} ile uyumlu sürüm bulunamadı",
                game_version.unwrap_or("?"),
                instance
                    .get("loader")
                    .and_then(Value::as_str)
                    .unwrap_or("vanilla")
            )
        })
}

pub async fn install_mod_to_instance<F>(
    paths: &AppPaths,
    store: &Store,
    project_id_or_slug: &str,
    instance_id: &str,
    version_id: Option<&str>,
    on_progress: F,
) -> Result<bool, String>
where
    F: Fn(Value),
{
    let instance = instance_manager::get_instance(store, instance_id)?;
    let mods_dir = paths.instance_dir(instance_id).join("mods");
    fs::create_dir_all(&mods_dir).map_err(|err| format!("Klasör oluşturulamadı: {err}"))?;
    let mut meta = load_meta(paths, instance_id, "mod");

    let mut queue: VecDeque<(String, Option<String>)> = VecDeque::new();
    queue.push_back((
        project_id_or_slug.to_string(),
        version_id.map(ToString::to_string),
    ));
    let mut installed_projects: HashSet<String> = HashSet::new();

    while let Some((id_or_slug, requested_version)) = queue.pop_front() {
        if installed_projects.contains(&id_or_slug) {
            continue;
        }
        installed_projects.insert(id_or_slug.clone());
        on_progress(json!({ "stage": "resolving", "name": id_or_slug }));
        let version = resolve_version(&instance, &id_or_slug, requested_version.as_deref()).await?;
        let project_id = version
            .get("project_id")
            .and_then(Value::as_str)
            .unwrap_or(&id_or_slug)
            .to_string();

        if let Some(already) = meta.get(&project_id) {
            if let Some(filename) = already.get("filename").and_then(Value::as_str) {
                if mods_dir.join(filename).exists() {
                    crate::logger::info(&format!("Zaten kurulu, atlanıyor: {filename}"));
                    continue;
                }
            }
        }

        let file = primary_file(&version)
            .ok_or_else(|| format!("İndirilecek dosya bulunamadı: {id_or_slug}"))?;
        let filename = file
            .get("filename")
            .and_then(Value::as_str)
            .ok_or_else(|| "Dosya adı bulunamadı".to_string())?
            .to_string();
        let url = file
            .get("url")
            .and_then(Value::as_str)
            .ok_or_else(|| "Dosya indirme adresi bulunamadı".to_string())?;
        let sha1 = file.pointer("/hashes/sha1").and_then(Value::as_str);
        on_progress(json!({ "stage": "downloading", "name": filename }));
        download_file(url, &mods_dir.join(&filename), sha1).await?;

        meta.insert(
            project_id.clone(),
            json!({
                "projectId": project_id,
                "title": project_title(&project_id).await.unwrap_or_else(|| id_or_slug.to_string()),
                "filename": filename,
                "versionId": version.get("id").cloned().unwrap_or(Value::Null),
                "versionNumber": version.get("version_number").cloned().unwrap_or(Value::Null),
                "installedAt": http::now_millis()
            }),
        );

        if let Some(dependencies) = version.get("dependencies").and_then(Value::as_array) {
            for dependency in dependencies {
                if dependency.get("dependency_type").and_then(Value::as_str) != Some("required") {
                    continue;
                }
                if let Some(dep_project) = dependency.get("project_id").and_then(Value::as_str) {
                    if !installed_projects.contains(dep_project) {
                        queue.push_back((
                            dep_project.to_string(),
                            dependency
                                .get("version_id")
                                .and_then(Value::as_str)
                                .map(ToString::to_string),
                        ));
                    }
                }
            }
        }
    }

    save_meta(paths, instance_id, "mod", &meta);
    on_progress(json!({ "stage": "done" }));
    Ok(true)
}

pub async fn install_content_to_instance<F>(
    paths: &AppPaths,
    store: &Store,
    project_id_or_slug: &str,
    instance_id: &str,
    version_id: Option<&str>,
    project_type: &str,
    on_progress: F,
) -> Result<bool, String>
where
    F: Fn(Value),
{
    let folder = content_folder(project_type)?;
    let instance = instance_manager::get_instance(store, instance_id)?;
    let target_dir = paths.instance_dir(instance_id).join(folder);
    fs::create_dir_all(&target_dir).map_err(|err| format!("Klasör oluşturulamadı: {err}"))?;
    let mut meta = load_meta(paths, instance_id, project_type);

    on_progress(json!({ "stage": "resolving", "name": project_id_or_slug }));
    let version = if let Some(version_id) = version_id.filter(|value| !value.is_empty()) {
        api_get(&format!("/version/{version_id}")).await?
    } else {
        let game_version = instance.get("version").and_then(Value::as_str);
        let versions = get_project_versions(project_id_or_slug, None, game_version).await?;
        versions
            .as_array()
            .and_then(|items| items.first())
            .cloned()
            .ok_or_else(|| {
                format!(
                    "\"{project_id_or_slug}\" için {} ile uyumlu sürüm bulunamadı",
                    game_version.unwrap_or("?")
                )
            })?
    };

    let project_id = version
        .get("project_id")
        .and_then(Value::as_str)
        .unwrap_or(project_id_or_slug)
        .to_string();
    if let Some(already) = meta.get(&project_id) {
        if let Some(filename) = already.get("filename").and_then(Value::as_str) {
            if target_dir.join(filename).exists() {
                crate::logger::info(&format!("Zaten kurulu, atlanıyor: {filename}"));
                on_progress(json!({ "stage": "done" }));
                return Ok(true);
            }
        }
    }

    let file = primary_file(&version)
        .ok_or_else(|| format!("İndirilecek dosya bulunamadı: {project_id_or_slug}"))?;
    let filename = file
        .get("filename")
        .and_then(Value::as_str)
        .ok_or_else(|| "Dosya adı bulunamadı".to_string())?
        .to_string();
    let url = file
        .get("url")
        .and_then(Value::as_str)
        .ok_or_else(|| "Dosya indirme adresi bulunamadı".to_string())?;
    let sha1 = file.pointer("/hashes/sha1").and_then(Value::as_str);
    on_progress(json!({ "stage": "downloading", "name": filename }));
    download_file(url, &target_dir.join(&filename), sha1).await?;

    meta.insert(
        project_id.clone(),
        json!({
            "projectId": project_id,
            "title": project_title(&project_id).await.unwrap_or_else(|| project_id_or_slug.to_string()),
            "filename": filename,
            "versionId": version.get("id").cloned().unwrap_or(Value::Null),
            "versionNumber": version.get("version_number").cloned().unwrap_or(Value::Null),
            "installedAt": http::now_millis()
        }),
    );
    save_meta(paths, instance_id, project_type, &meta);
    on_progress(json!({ "stage": "done" }));
    Ok(true)
}

pub fn list_installed_mods(paths: &AppPaths, instance_id: &str) -> Value {
    let mods_dir = paths.instance_dir(instance_id).join("mods");
    let meta = load_meta(paths, instance_id, "mod");
    let meta_by_filename: Map<String, Value> = meta
        .values()
        .filter_map(|entry| {
            entry
                .get("filename")
                .and_then(Value::as_str)
                .map(|filename| (filename.to_string(), entry.clone()))
        })
        .collect();
    let mut result = Vec::new();
    let Ok(entries) = fs::read_dir(&mods_dir) else {
        return Value::Array(result);
    };
    for entry in entries.flatten() {
        let filename = entry.file_name().to_string_lossy().to_string();
        if !filename.ends_with(".jar") && !filename.ends_with(".jar.disabled") {
            continue;
        }
        let base_filename = filename.trim_end_matches(".disabled").to_string();
        let matched = meta_by_filename
            .get(&base_filename)
            .or_else(|| meta_by_filename.get(&filename));
        // Gerçek ad önceliği: jar içi meta (fabric.mod.json / mods.toml /
        // mcmod.info) > kayıtlı Modrinth meta > dosya adı. Jar içi bilgi,
        // launcher dışından gelmiş ya da meta'da ID kayıtlı modları da düzeltir.
        let (jar_title, jar_version) = read_jar_metadata(&entry.path());
        let title = jar_title.or_else(|| {
            matched
                .and_then(|entry| entry.get("title"))
                .and_then(Value::as_str)
                .map(ToString::to_string)
        }).unwrap_or_else(|| base_filename.trim_end_matches(".jar").to_string());
        let version_number = matched
            .and_then(|entry| entry.get("versionNumber"))
            .and_then(Value::as_str)
            .map(ToString::to_string)
            .or(jar_version);
        result.push(json!({
            "filename": filename,
            "enabled": !filename.ends_with(".disabled"),
            "path": entry.path().to_string_lossy().to_string(),
            "projectId": matched.and_then(|entry| entry.get("projectId")).cloned().unwrap_or(Value::Null),
            "title": title,
            "versionNumber": version_number.map(Value::String).unwrap_or(Value::Null)
        }));
    }
    Value::Array(result)
}

/// Jar içinden modun gerçek adını/versiyonunu okur:
/// fabric.mod.json (Fabric/Quilt), neoforge.mods.toml / mods.toml (Forge/NeoForge), mcmod.info.
fn read_jar_metadata(path: &Path) -> (Option<String>, Option<String>) {
    let Ok(file) = File::open(path) else {
        return (None, None);
    };
    let Ok(mut archive) = ZipArchive::new(file) else {
        return (None, None);
    };

    if let Ok(mut entry) = archive.by_name("fabric.mod.json") {
        let mut raw = String::new();
        if entry.read_to_string(&mut raw).is_ok() {
            if let Ok(value) = serde_json::from_str::<Value>(&raw) {
                let name = value
                    .get("name")
                    .and_then(Value::as_str)
                    .or_else(|| value.get("id").and_then(Value::as_str))
                    .map(ToString::to_string)
                    .filter(|name| !name.trim().is_empty());
                let version = value
                    .get("version")
                    .and_then(Value::as_str)
                    .map(ToString::to_string)
                    .filter(|version| !version.trim().is_empty());
                if let Some(name) = name {
                    return (Some(name), version);
                }
            }
        }
    }

    for toml_path in ["META-INF/neoforge.mods.toml", "META-INF/mods.toml"] {
        if let Ok(mut entry) = archive.by_name(toml_path) {
            let mut raw = String::new();
            if entry.read_to_string(&mut raw).is_ok() {
                let name = toml_value_of(&raw, "displayName").or_else(|| toml_value_of(&raw, "modId"));
                let version = toml_value_of(&raw, "version");
                if let Some(name) = name {
                    return (Some(name), version);
                }
            }
        }
    }

    if let Ok(mut entry) = archive.by_name("mcmod.info") {
        let mut raw = String::new();
        if entry.read_to_string(&mut raw).is_ok() {
            if let Ok(value) = serde_json::from_str::<Value>(&raw) {
                let list = value.as_array().cloned().unwrap_or_default();
                if let Some(first) = list.first() {
                    let name = first
                        .get("name")
                        .and_then(Value::as_str)
                        .map(ToString::to_string)
                        .filter(|name| !name.trim().is_empty());
                    let version = first
                        .get("version")
                        .and_then(Value::as_str)
                        .map(ToString::to_string);
                    if let Some(name) = name {
                        return (Some(name), version);
                    }
                }
            }
        }
    }

    (None, None)
}

/// mods.toml tarzı dosyalarda `key = "değer"` satırından değeri çıkarır.
fn toml_value_of(raw: &str, key: &str) -> Option<String> {
    for line in raw.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(key) {
            if let Some(after_eq) = rest.trim_start().strip_prefix('=') {
                let val = after_eq.trim();
                if let Some(unquoted) = val.strip_prefix('"').and_then(|value| value.strip_suffix('"')) {
                    return Some(unquoted.to_string());
                }
                if let Some(unquoted) = val.strip_prefix('\'').and_then(|value| value.strip_suffix('\'')) {
                    return Some(unquoted.to_string());
                }
                return Some(val.to_string());
            }
        }
    }
    None
}

pub fn toggle_mod(
    paths: &AppPaths,
    instance_id: &str,
    filename: &str,
    enable: bool,
) -> Result<String, String> {
    let mods_dir = paths.instance_dir(instance_id).join("mods");
    let current = mods_dir.join(filename);
    let target_name = if enable {
        filename.trim_end_matches(".disabled").to_string()
    } else if filename.ends_with(".disabled") {
        filename.to_string()
    } else {
        format!("{filename}.disabled")
    };
    let target = mods_dir.join(&target_name);
    if current == target {
        return Ok(target.to_string_lossy().to_string());
    }
    fs::rename(&current, &target).map_err(|err| format!("Mod durumu değiştirilemedi: {err}"))?;
    Ok(target.to_string_lossy().to_string())
}

pub fn remove_mod(paths: &AppPaths, instance_id: &str, filename: &str) -> Result<(), String> {
    let mods_dir = paths.instance_dir(instance_id).join("mods");
    fs::remove_file(mods_dir.join(filename)).map_err(|err| format!("Mod silinemedi: {err}"))?;
    let base_filename = filename.trim_end_matches(".disabled");
    let mut meta = load_meta(paths, instance_id, "mod");
    let project_id = meta
        .iter()
        .find(|(_, entry)| entry.get("filename").and_then(Value::as_str) == Some(base_filename))
        .map(|(id, _)| id.clone());
    if let Some(project_id) = project_id {
        meta.remove(&project_id);
        save_meta(paths, instance_id, "mod", &meta);
    }
    Ok(())
}

pub async fn list_installed_content(
    paths: &AppPaths,
    instance_id: &str,
    project_type: &str,
) -> Result<Value, String> {
    let folder = content_folder(project_type)?;
    let target_dir = paths.instance_dir(instance_id).join(folder);
    let meta = load_meta(paths, instance_id, project_type);
    let meta_by_filename: Map<String, Value> = meta
        .values()
        .filter_map(|entry| {
            entry
                .get("filename")
                .and_then(Value::as_str)
                .map(|filename| (filename.to_string(), entry.clone()))
        })
        .collect();
    let Ok(entries) = fs::read_dir(&target_dir) else {
        return Ok(Value::Array(Vec::new()));
    };
    let mut result = Vec::new();
    for entry in entries.flatten() {
        let filename = entry.file_name().to_string_lossy().to_string();
        if !filename.ends_with(".zip") {
            continue;
        }
        let matched = meta_by_filename.get(&filename);
        result.push(json!({
            "filename": filename,
            "projectId": matched.and_then(|entry| entry.get("projectId")).cloned().unwrap_or(Value::Null),
            "title": matched.and_then(|entry| entry.get("title")).cloned().unwrap_or_else(|| json!(filename.trim_end_matches(".zip"))),
            "versionNumber": matched.and_then(|entry| entry.get("versionNumber")).cloned().unwrap_or(Value::Null)
        }));
    }
    Ok(Value::Array(result))
}

pub fn remove_content(
    paths: &AppPaths,
    instance_id: &str,
    project_type: &str,
    filename: &str,
) -> Result<(), String> {
    let folder = content_folder(project_type)?;
    let target_dir = paths.instance_dir(instance_id).join(folder);
    fs::remove_file(target_dir.join(filename))
        .map_err(|err| format!("İçerik silinemedi: {err}"))?;
    let mut meta = load_meta(paths, instance_id, project_type);
    let project_id = meta
        .iter()
        .find(|(_, entry)| entry.get("filename").and_then(Value::as_str) == Some(filename))
        .map(|(id, _)| id.clone());
    if let Some(project_id) = project_id {
        meta.remove(&project_id);
        save_meta(paths, instance_id, project_type, &meta);
    }
    Ok(())
}

pub async fn check_for_updates(
    paths: &AppPaths,
    store: &Store,
    instance_id: &str,
) -> Result<Value, String> {
    let instance = instance_manager::get_instance(store, instance_id)?;
    let meta = load_meta(paths, instance_id, "mod");
    let loader = instance
        .get("loader")
        .and_then(Value::as_str)
        .filter(|loader| *loader != "vanilla");
    let game_version = instance.get("version").and_then(Value::as_str);
    let mut updates = Vec::new();
    for entry in meta.values() {
        let project_id = entry
            .get("projectId")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if project_id.is_empty() {
            continue;
        }
        match get_project_versions(project_id, loader, game_version).await {
            Ok(versions) => {
                let Some(latest) = versions.as_array().and_then(|items| items.first()) else {
                    continue;
                };
                let latest_id = latest.get("id").and_then(Value::as_str);
                if latest_id != entry.get("versionId").and_then(Value::as_str) {
                    updates.push(json!({
                        "projectId": project_id,
                        "title": entry.get("title").cloned().unwrap_or(Value::Null),
                        "filename": entry.get("filename").cloned().unwrap_or(Value::Null),
                        "currentVersionNumber": entry.get("versionNumber").cloned().unwrap_or(Value::Null),
                        "latestVersionId": latest.get("id").cloned().unwrap_or(Value::Null),
                        "latestVersionNumber": latest.get("version_number").cloned().unwrap_or(Value::Null)
                    }));
                }
            }
            Err(err) => crate::logger::warn(&format!(
                "Güncelleme kontrolü başarısız ({project_id}): {err}"
            )),
        }
    }
    Ok(Value::Array(updates))
}

pub async fn update_mod<F>(
    paths: &AppPaths,
    store: &Store,
    instance_id: &str,
    project_id: &str,
    on_progress: F,
) -> Result<bool, String>
where
    F: Fn(Value),
{
    let meta = load_meta(paths, instance_id, "mod");
    if let Some(entry) = meta.get(project_id) {
        if let Some(filename) = entry.get("filename").and_then(Value::as_str) {
            let mods_dir = paths.instance_dir(instance_id).join("mods");
            let path = mods_dir.join(filename);
            let disabled = mods_dir.join(format!("{filename}.disabled"));
            if path.exists() {
                let _ = fs::remove_file(path);
            } else if disabled.exists() {
                let _ = fs::remove_file(disabled);
            }
        }
    }
    install_mod_to_instance(paths, store, project_id, instance_id, None, on_progress).await
}

fn extract_zip_to(archive_path: &Path, destination: &Path) -> Result<(), String> {
    let file = File::open(archive_path).map_err(|err| format!("Zip açılamadı: {err}"))?;
    let mut archive = ZipArchive::new(file).map_err(|err| format!("Geçersiz zip: {err}"))?;
    fs::create_dir_all(destination).map_err(|err| format!("Klasör oluşturulamadı: {err}"))?;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|err| format!("Zip girdisi okunamadı: {err}"))?;
        let Some(enclosed) = entry.enclosed_name() else {
            continue;
        };
        let target = destination.join(enclosed);
        if entry.is_dir() {
            fs::create_dir_all(&target).map_err(|err| format!("Klasör oluşturulamadı: {err}"))?;
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
                    .map_err(|err| format!("Zip okunamadı: {err}"))?;
                if read == 0 {
                    break;
                }
                std::io::Write::write_all(&mut output, &buffer[..read])
                    .map_err(|err| format!("Dosya yazılamadı: {err}"))?;
            }
        }
    }
    Ok(())
}

pub async fn install_modpack_from_project<F>(
    paths: &AppPaths,
    store: &Store,
    project_id_or_slug: &str,
    version_id: Option<&str>,
    on_progress: F,
) -> Result<Value, String>
where
    F: Fn(Value),
{
    on_progress(json!({ "stage": "manifest" }));
    let version = if let Some(version_id) = version_id.filter(|value| !value.is_empty()) {
        api_get(&format!("/version/{version_id}")).await?
    } else {
        let versions = get_project_versions(project_id_or_slug, None, None).await?;
        versions
            .as_array()
            .and_then(|items| items.first())
            .cloned()
            .ok_or_else(|| "Bu modpack için hiç sürüm bulunamadı".to_string())?
    };
    let file = version
        .get("files")
        .and_then(Value::as_array)
        .and_then(|files| {
            files.iter().find(|file| {
                file.get("filename")
                    .and_then(Value::as_str)
                    .map(|name| name.ends_with(".mrpack"))
                    .unwrap_or(false)
            })
        })
        .or_else(|| version.get("files").and_then(Value::as_array).and_then(|files| files.first()))
        .cloned()
        .ok_or_else(|| ".mrpack dosyası bulunamadı".to_string())?;
    let filename = file
        .get("filename")
        .and_then(Value::as_str)
        .unwrap_or("modpack.mrpack")
        .to_string();
    let url = file
        .get("url")
        .and_then(Value::as_str)
        .ok_or_else(|| "Modpack indirme adresi bulunamadı".to_string())?;
    let sha1 = file.pointer("/hashes/sha1").and_then(Value::as_str);

    let tmp_dir = std::env::temp_dir().join("buran-launcher-modpacks");
    fs::create_dir_all(&tmp_dir).map_err(|err| format!("Geçici klasör oluşturulamadı: {err}"))?;
    let tmp_path = tmp_dir.join(format!("{}-{filename}", http::now_millis()));
    on_progress(json!({ "stage": "downloadingMods", "current": 0, "total": 1, "name": filename }));
    download_file(url, &tmp_path, sha1).await?;
    let result = install_modpack_from_file(paths, store, &tmp_path, &on_progress).await;
    let _ = fs::remove_file(&tmp_path);
    result
}

pub async fn install_modpack_from_file<F>(
    paths: &AppPaths,
    store: &Store,
    mrpack_path: &Path,
    on_progress: &F,
) -> Result<Value, String>
where
    F: Fn(Value),
{
    on_progress(json!({ "stage": "manifest" }));
    let extract_dir = PathBuf::from(format!("{}_extracted", mrpack_path.to_string_lossy()));
    if extract_dir.exists() {
        let _ = fs::remove_dir_all(&extract_dir);
    }
    extract_zip_to(mrpack_path, &extract_dir)?;

    let index_path = extract_dir.join("modrinth.index.json");
    if !index_path.exists() {
        return Err("modrinth.index.json bulunamadı - geçersiz .mrpack dosyası".to_string());
    }
    let raw = fs::read_to_string(&index_path)
        .map_err(|err| format!("modrinth.index.json okunamadı: {err}"))?;
    let index: Value = serde_json::from_str(&raw)
        .map_err(|err| format!("modrinth.index.json ayrıştırılamadı: {err}"))?;

    let dependencies = index.get("dependencies").and_then(Value::as_object);
    let loader_entry = dependencies.and_then(|deps| {
        deps.iter().find(|(key, _)| key.as_str() != "minecraft")
    });
    let loader = loader_entry
        .map(|(key, _)| key.trim_end_matches("-loader").to_string())
        .unwrap_or_else(|| "vanilla".to_string());
    let loader_version = loader_entry
        .and_then(|(_, value)| value.as_str())
        .map(ToString::to_string);
    let mc_version = dependencies
        .and_then(|deps| deps.get("minecraft"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if mc_version.is_empty() {
        return Err("Modpack Minecraft sürümü belirtilmemiş".to_string());
    }

    let instance = instance_manager::create_instance(
        store,
        paths,
        &json!({
            "name": index.get("name").and_then(Value::as_str).unwrap_or("Modpack"),
            "icon": "package",
            "version": mc_version,
            "versionType": "release",
            "loader": loader,
            "loaderVersion": loader_version
        }),
    )?;
    let instance_id = instance["id"].as_str().unwrap_or_default().to_string();
    let instance_dir = paths.instance_dir(&instance_id);

    let files = index
        .get("files")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let total = files.len();
    for (index, file) in files.iter().enumerate() {
        let relative = file.get("path").and_then(Value::as_str).unwrap_or("");
        on_progress(json!({
            "stage": "downloadingMods",
            "current": index + 1,
            "total": total,
            "name": relative
        }));
        let Some(url) = file
            .get("downloads")
            .and_then(Value::as_array)
            .and_then(|downloads| downloads.first())
            .and_then(Value::as_str)
        else {
            continue;
        };
        let target = instance_dir.join(relative);
        if !instance_manager::is_safe_child(&instance_dir, &target) {
            crate::logger::warn(&format!("Modpack dosya yolu güvenli değil, atlanıyor: {relative}"));
            continue;
        }
        let sha1 = file.pointer("/hashes/sha1").and_then(Value::as_str);
        download_file(url, &target, sha1).await?;
    }

    let overrides = extract_dir.join("overrides");
    if overrides.exists() {
        instance_manager::copy_dir_recursive(&overrides, &instance_dir)?;
    }
    let _ = fs::remove_dir_all(&extract_dir);
    on_progress(json!({ "stage": "done" }));
    Ok(instance)
}