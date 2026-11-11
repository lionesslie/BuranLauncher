use crate::auth::{self, MclcAuth};
use crate::http;
use crate::instance_manager;
use crate::java_manager;
use crate::paths::AppPaths;
use crate::store::Store;
use crate::version_manager;
use futures_util::StreamExt;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Child;
use tokio::sync::Mutex;
use zip::ZipArchive;

type SharedChild = std::sync::Arc<Mutex<Child>>;

fn active_processes() -> &'static Mutex<HashMap<String, SharedChild>> {
    static PROCESSES: OnceLock<Mutex<HashMap<String, SharedChild>>> = OnceLock::new();
    PROCESSES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn emit_log(app: &AppHandle, instance_id: &str, line: &str) {
    let _ = app.emit(
        "launch:log",
        json!({ "instanceId": instance_id, "line": line }),
    );
}

fn emit_status(app: &AppHandle, instance_id: &str, status: Value) {
    let _ = app.emit(
        "launch:status",
        json!({ "instanceId": instance_id, "status": status }),
    );
}

fn emit_closed(app: &AppHandle, instance_id: &str, code: i32) {
    let _ = app.emit(
        "launch:closed",
        json!({ "instanceId": instance_id, "code": code }),
    );
}

fn is_allowed(rules: Option<&Value>) -> bool {
    let Some(rules) = rules.and_then(Value::as_array) else {
        return true;
    };
    if rules.is_empty() {
        return true;
    }
    let current_os = match std::env::consts::OS {
        "windows" => "windows",
        "macos" => "osx",
        "linux" => "linux",
        other => other,
    };
    let current_arch = std::env::consts::ARCH;
    let rule_arch_matches = |required: &str| match required {
        "x86" => current_arch == "x86",
        "x86_64" => current_arch == "x86_64",
        "arm64" | "aarch64" => current_arch == "aarch64",
        other => other == current_arch,
    };
    let features = json!({
        "is_demo_user": false,
        "has_custom_resolution": false,
        "has_quick_plays_support": false,
        "is_quick_play_singleplayer": false,
        "is_quick_play_multiplayer": false,
        "is_quick_play_realms": false
    });

    let mut allowed = false;
    for rule in rules {
        let action = rule.get("action").and_then(Value::as_str).unwrap_or("allow");
        let mut matches = true;
        if let Some(os) = rule.get("os") {
            if let Some(name) = os.get("name").and_then(Value::as_str) {
                if name != current_os {
                    matches = false;
                }
            }
            if let Some(arch) = os.get("arch").and_then(Value::as_str) {
                if !rule_arch_matches(arch) {
                    matches = false;
                }
            }
        }
        if matches {
            if let Some(rule_features) = rule.get("features").and_then(Value::as_object) {
                for (key, expected) in rule_features {
                    let actual = features.get(key).and_then(Value::as_bool).unwrap_or(false);
                    if actual != expected.as_bool().unwrap_or(false) {
                        matches = false;
                        break;
                    }
                }
            }
        }
        if matches {
            allowed = action == "allow";
        }
    }
    allowed
}

fn java_executable(java_path: &str) -> PathBuf {
    PathBuf::from(java_path)
}

fn maven_path_from_name(name: &str) -> Option<String> {
    let parts: Vec<&str> = name.split(':').collect();
    if parts.len() < 3 {
        return None;
    }
    let group = parts[0].replace('.', "/");
    let artifact = parts[1];
    let version = parts[2];
    let mut path = format!("{group}/{artifact}/{version}/{artifact}-{version}");
    if let Some(classifier) = parts.get(3) {
        path.push('-');
        path.push_str(classifier);
    }
    path.push_str(".jar");
    Some(path)
}

fn native_classifier_from_name(name: &str) -> Option<&str> {
    name.split(':')
        .nth(3)
        .filter(|classifier| classifier.starts_with("natives-"))
}

fn native_classifier_matches(classifier: &str) -> bool {
    let os = match std::env::consts::OS {
        "windows" => "windows",
        "macos" => "macos",
        "linux" => "linux",
        _ => return false,
    };
    let prefix = format!("natives-{os}");
    let Some(suffix) = classifier.strip_prefix(&prefix) else {
        return false;
    };
    match std::env::consts::ARCH {
        "x86_64" => suffix.is_empty() || suffix == "-x86_64",
        "x86" => suffix == "-x86",
        "aarch64" => suffix == "-arm64" || suffix == "-aarch64",
        _ => suffix.is_empty(),
    }
}

fn file_sha1(path: &Path) -> Option<String> {
    use sha1::Digest;
    let mut file = File::open(path).ok()?;
    let mut hasher = sha1::Sha1::new();
    let mut buffer = [0u8; 16384];
    loop {
        let read = file.read(&mut buffer).ok()?;
        if read == 0 {
            break;
        }
        use sha1::Digest;
        hasher.update(&buffer[..read]);
    }
    Some(http::hex_encode(&hasher.finalize()))
}

async fn ensure_download(
    url: &str,
    destination: &Path,
    expected_sha1: Option<&str>,
) -> Result<(), String> {
    if destination.exists() {
        if let Some(expected) = expected_sha1 {
            if file_sha1(destination).as_deref() == Some(expected) {
                return Ok(());
            }
        } else {
            return Ok(());
        }
    }
    let bytes = http::download_bytes(url)
        .await
        .map_err(|err| format!("Dosya indirilemedi ({}): {err}", destination.display()))?;
    if let Some(expected) = expected_sha1 {
        let actual = http::sha1_hex(&bytes);
        if actual != expected {
            return Err(format!(
                "SHA-1 doğrulaması başarısız: {} (beklenen {}, alınan {})",
                destination.display(),
                expected,
                actual
            ));
        }
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("Klasör oluşturulamadı: {err}"))?;
    }
    fs::write(destination, bytes).map_err(|err| format!("Dosya yazılamadı: {err}"))
}

async fn load_version_json(paths: &AppPaths, id: &str) -> Result<Value, String> {
    let path = paths.version_json_path(id);
    if let Ok(raw) = fs::read_to_string(&path) {
        if let Ok(value) = serde_json::from_str::<Value>(&raw) {
            return Ok(value);
        }
    }
    let detail = version_manager::get_version_detail(id)
        .await
        .map_err(|err| format!("Sürüm dosyası bulunamadı ({id}): {err}"))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("Klasör oluşturulamadı: {err}"))?;
    }
    fs::write(&path, serde_json::to_string_pretty(&detail).unwrap_or_default())
        .map_err(|err| format!("Sürüm dosyası yazılamadı: {err}"))?;
    Ok(detail)
}

async fn resolve_version(paths: &AppPaths, id: &str) -> Result<(Value, String), String> {
    let mut seen = Vec::new();
    let mut current_id = id.to_string();
    let mut chain: Vec<Value> = Vec::new();
    loop {
        if seen.contains(&current_id) {
            return Err("Sürüm tanımında döngüsel inheritsFrom tespit edildi".to_string());
        }
        seen.push(current_id.clone());
        let value = load_version_json(paths, &current_id).await?;
        chain.push(value.clone());
        if let Some(parent) = value.get("inheritsFrom").and_then(Value::as_str) {
            current_id = parent.to_string();
        } else {
            break;
        }
    }

    let base_id = current_id;
    let mut merged = chain.pop().unwrap_or_else(|| json!({}));
    while let Some(child) = chain.pop() {
        merged = merge_version(merged, child);
    }
    Ok((merged, base_id))
}

fn merge_version(mut parent: Value, child: Value) -> Value {
    if let (Some(parent_map), Some(child_map)) = (parent.as_object_mut(), child.as_object()) {
        for (key, value) in child_map {
            match key.as_str() {
                "libraries" => {
                    let mut libraries = parent_map
                        .get("libraries")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default();
                    if let Some(child_libs) = value.as_array() {
                        libraries.extend(child_libs.iter().cloned());
                    }
                    parent_map.insert(key.clone(), Value::Array(libraries));
                }
                "arguments" => {
                    let merged_args = merge_arguments(parent_map.get("arguments"), value);
                    parent_map.insert(key.clone(), merged_args);
                }
                _ => {
                    parent_map.insert(key.clone(), value.clone());
                }
            }
        }
    }
    parent
}

fn merge_arguments(parent: Option<&Value>, child: &Value) -> Value {
    let mut result = parent
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    if let Some(child_map) = child.as_object() {
        for (key, value) in child_map {
            let combined = result
                .get(key)
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .chain(value.as_array().cloned().unwrap_or_default())
                .collect::<Vec<_>>();
            result.insert(key.clone(), Value::Array(combined));
        }
    }
    Value::Object(result)
}

fn substitute(input: &str, variables: &HashMap<String, String>) -> String {
    let mut output = input.to_string();
    for (key, value) in variables {
        output = output.replace(&format!("${{{key}}}"), value);
    }
    output
}

fn version_argument_list(
    arguments: &Value,
    key: &str,
    variables: &HashMap<String, String>,
) -> Vec<String> {
    let mut out = Vec::new();
    let Some(list) = arguments.get(key).and_then(Value::as_array) else {
        return out;
    };
    for item in list {
        if let Some(text) = item.as_str() {
            out.push(substitute(text, variables));
            continue;
        }
        if !is_allowed(item.get("rules")) {
            continue;
        }
        if let Some(value) = item.get("value") {
            if let Some(text) = value.as_str() {
                out.push(substitute(text, variables));
            } else if let Some(values) = value.as_array() {
                for value in values {
                    if let Some(text) = value.as_str() {
                        out.push(substitute(text, variables));
                    }
                }
            }
        }
    }
    out
}

fn legacy_arguments(raw: &str, variables: &HashMap<String, String>) -> Vec<String> {
    raw.split_whitespace()
        .map(|part| substitute(part, variables))
        .collect()
}

fn extract_natives(jar_path: &Path, destination: &Path, excludes: &[String]) -> Result<(), String> {
    let file = File::open(jar_path).map_err(|err| format!("Natives jar açılamadı: {err}"))?;
    let mut archive = ZipArchive::new(file).map_err(|err| format!("Natives jar geçersiz: {err}"))?;
    fs::create_dir_all(destination).map_err(|err| format!("Natives klasörü oluşturulamadı: {err}"))?;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|err| format!("Natives girdisi okunamadı: {err}"))?;
        let Some(enclosed) = entry.enclosed_name() else {
            continue;
        };
        if entry.is_dir() {
            continue;
        }
        let name = enclosed.to_string_lossy().replace('\\', "/");
        if name.starts_with("META-INF/")
            || excludes
                .iter()
                .any(|exclude| name == *exclude || name.starts_with(&format!("{exclude}/")))
        {
            continue;
        }
        let target = destination.join(enclosed);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|err| format!("Klasör oluşturulamadı: {err}"))?;
        }
        let mut output = File::create(&target).map_err(|err| format!("Natives yazılamadı: {err}"))?;
        std::io::copy(&mut entry, &mut output)
            .map_err(|err| format!("Natives çıkarılamadı: {err}"))?;
    }
    Ok(())
}

struct LaunchContext {
    library_dir: PathBuf,
    assets_dir: PathBuf,
    natives_dir: PathBuf,
    classpath: Vec<PathBuf>,
    asset_index_name: String,
    log_config_path: Option<PathBuf>,
}

async fn prepare_client(
    app: &AppHandle,
    paths: &AppPaths,
    instance_id: &str,
    version: &Value,
    base_id: &str,
) -> Result<LaunchContext, String> {
    let root = paths.minecraft_root();
    let library_dir = root.join("libraries");
    let assets_dir = root.join("assets");
    let instance_dir = paths.instance_dir(instance_id);
    let natives_dir = instance_dir.join("natives");
    fs::create_dir_all(&library_dir).map_err(|err| format!("Klasör oluşturulamadı: {err}"))?;
    fs::create_dir_all(&assets_dir).map_err(|err| format!("Klasör oluşturulamadı: {err}"))?;
    fs::create_dir_all(&natives_dir).map_err(|err| format!("Klasör oluşturulamadı: {err}"))?;

    emit_status(app, instance_id, json!({ "task": "client", "current": 0, "total": 1 }));
    let client_jar = paths.versions_dir().join(base_id).join(format!("{base_id}.jar"));
    if let Some(client) = version.pointer("/downloads/client") {
        if let Some(url) = client.get("url").and_then(Value::as_str) {
            let sha1 = client.get("sha1").and_then(Value::as_str);
            ensure_download(url, &client_jar, sha1).await?;
        }
    }
    if !client_jar.exists() {
        return Err(format!(
            "İstemci jar dosyası bulunamadı: {}",
            client_jar.display()
        ));
    }
    emit_status(app, instance_id, json!({ "task": "client", "current": 1, "total": 1 }));

    let libraries = version
        .get("libraries")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    emit_status(
        app,
        instance_id,
        json!({ "task": "libraries", "current": 0, "total": libraries.len() }),
    );
    let current_os = match std::env::consts::OS {
        "windows" => "windows",
        "macos" => "osx",
        "linux" => "linux",
        other => other,
    };
    let mut classpath: Vec<PathBuf> = Vec::new();
    for (index, library) in libraries.iter().enumerate() {
        if !is_allowed(library.get("rules")) {
            continue;
        }
        let name = library.get("name").and_then(Value::as_str).unwrap_or("");
        let native_classifier = native_classifier_from_name(name);
        if let Some(classifier) = native_classifier {
            if !native_classifier_matches(classifier) {
                continue;
            }
        }
        let native_artifact = native_classifier.is_some();
        let mut artifact_path: Option<PathBuf> = None;

        if let Some(artifact) = library.pointer("/downloads/artifact") {
            if let (Some(url), Some(path)) = (
                artifact.get("url").and_then(Value::as_str),
                artifact.get("path").and_then(Value::as_str),
            ) {
                let destination = library_dir.join(path);
                let sha1 = artifact.get("sha1").and_then(Value::as_str);
                ensure_download(url, &destination, sha1).await?;
                artifact_path = Some(destination);
            }
        } else if !name.is_empty() {
            if let Some(path) = maven_path_from_name(name) {
                let base_url = library
                    .get("url")
                    .and_then(Value::as_str)
                    .unwrap_or("https://libraries.minecraft.net/");
                if !base_url.is_empty() {
                    let destination = library_dir.join(&path);
                    let url = format!("{}/{}", base_url.trim_end_matches('/'), path);
                    ensure_download(&url, &destination, None).await?;
                    artifact_path = Some(destination);
                }
            }
        }

        if let Some(downloads) = library.get("downloads") {
            if let Some(natives) = library.get("natives").and_then(Value::as_object) {
                if let Some(classifier) = natives.get(current_os).and_then(Value::as_str) {
                    let classifier = classifier.replace(
                        "${arch}",
                        if cfg!(target_pointer_width = "64") {
                            "64"
                        } else {
                            "32"
                        },
                    );
                    if let Some(native) = downloads
                        .get("classifiers")
                        .and_then(|classifiers| classifiers.get(&classifier))
                    {
                        let url = native.get("url").and_then(Value::as_str);
                        let path = native.get("path").and_then(Value::as_str);
                        if let (Some(url), Some(path)) = (url, path) {
                            let destination = library_dir.join(path);
                            let sha1 = native.get("sha1").and_then(Value::as_str);
                            ensure_download(url, &destination, sha1).await?;
                            let excludes = library
                                .pointer("/extract/exclude")
                                .and_then(Value::as_array)
                                .map(|items| {
                                    items
                                        .iter()
                                        .filter_map(Value::as_str)
                                        .map(ToString::to_string)
                                        .collect::<Vec<_>>()
                                })
                                .unwrap_or_default();
                            extract_natives(&destination, &natives_dir, &excludes)?;
                        }
                    }
                }
            }
        }

        if let Some(destination) = artifact_path {
            if native_artifact {
                let excludes = library
                    .pointer("/extract/exclude")
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(Value::as_str)
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                extract_natives(&destination, &natives_dir, &excludes)?;
            } else {
                classpath.push(destination);
            }
        }
        if index % 10 == 0 || index + 1 == libraries.len() {
            emit_status(
                app,
                instance_id,
                json!({ "task": "libraries", "current": index + 1, "total": libraries.len() }),
            );
        }
    }
    classpath.push(client_jar.clone());

    let asset_index = version.get("assetIndex").cloned().unwrap_or(Value::Null);
    let asset_index_name = asset_index
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("legacy")
        .to_string();
    let assets_root_override = version.get("assetIndex").is_some();
    if assets_root_override {
        if let Some(url) = asset_index.get("url").and_then(Value::as_str) {
            let index_path = assets_dir
                .join("indexes")
                .join(format!("{asset_index_name}.json"));
            let sha1 = asset_index.get("sha1").and_then(Value::as_str);
            ensure_download(url, &index_path, sha1).await?;
            download_asset_objects(app, instance_id, &assets_dir, &index_path, &asset_index).await?;
        }
    }

    let log_config_path = if let Some(logging) = version.pointer("/logging/client") {
        let file = &logging["file"];
        let id = file.get("id").and_then(Value::as_str).unwrap_or("client.xml");
        if let Some(url) = file.get("url").and_then(Value::as_str) {
            let destination = assets_dir.join("log_configs").join(id);
            let sha1 = file.get("sha1").and_then(Value::as_str);
            ensure_download(url, &destination, sha1).await?;
            Some(destination)
        } else {
            None
        }
    } else {
        None
    };

    Ok(LaunchContext {
        library_dir,
        assets_dir,
        natives_dir,
        classpath,
        asset_index_name,
        log_config_path,
    })
}

async fn download_asset_objects(
    app: &AppHandle,
    instance_id: &str,
    assets_dir: &Path,
    index_path: &Path,
    asset_index: &Value,
) -> Result<(), String> {
    let raw = fs::read_to_string(index_path)
        .map_err(|err| format!("Varlık dizini okunamadı: {err}"))?;
    let index: Value =
        serde_json::from_str(&raw).map_err(|err| format!("Varlık dizini ayrıştırılamadı: {err}"))?;
    let Some(objects) = index.get("objects").and_then(Value::as_object) else {
        return Ok(());
    };
    let total = objects.len();
    emit_status(
        app,
        instance_id,
        json!({ "task": "assets", "current": 0, "total": total }),
    );

    let mut pending: Vec<(String, PathBuf)> = Vec::new();
    let mut virtual_objects: Vec<(String, PathBuf)> = Vec::new();
    for (name, object) in objects {
        let Some(hash) = object.get("hash").and_then(Value::as_str) else {
            continue;
        };
        if hash.len() < 2 {
            continue;
        }
        let destination = assets_dir
            .join("objects")
            .join(&hash[..2])
            .join(hash);
        if !destination.exists() {
            let url = format!("https://resources.download.minecraft.net/{}/{}", &hash[..2], hash);
            pending.push((url, destination.clone()));
        }
        if index
            .get("virtual")
            .and_then(Value::as_bool)
            .unwrap_or(false)
            || index
                .get("map_to_resources")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        {
            virtual_objects.push((name.clone(), destination));
        }
    }

    let mut completed = total.saturating_sub(pending.len());
    emit_status(
        app,
        instance_id,
        json!({ "task": "assets", "current": completed, "total": total }),
    );

    let downloads = pending.into_iter().map(|(url, destination)| async move {
        let bytes = http::download_bytes(&url).await?;
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|err| format!("Klasör oluşturulamadı: {err}"))?;
        }
        fs::write(&destination, bytes).map_err(|err| format!("Varlık yazılamadı: {err}"))?;
        Ok::<(), String>(())
    });
    let results: Vec<Result<(), String>> = futures_util::stream::iter(downloads)
        .buffer_unordered(16)
        .collect()
        .await;
    let mut failures = 0;
    for result in results {
        match result {
            Ok(()) => completed += 1,
            Err(err) => {
                failures += 1;
                crate::logger::warn(&format!("Varlık indirilemedi: {err}"));
            }
        }
    }
    emit_status(
        app,
        instance_id,
        json!({ "task": "assets", "current": completed, "total": total }),
    );
    if failures > 0 {
        crate::logger::warn(&format!("{failures} varlık indirilemedi, oyun eksik varlıklarla başlatılacak"));
    }

    if !virtual_objects.is_empty() {
        let virtual_root = if index
            .get("map_to_resources")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            assets_dir.parent().unwrap_or(assets_dir).join("resources")
        } else {
            assets_dir
                .join("virtual")
                .join(
                    asset_index
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or("legacy"),
                )
        };
        for (name, source) in virtual_objects {
            if !source.exists() {
                continue;
            }
            let target = virtual_root.join(&name);
            if let Some(parent) = target.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if !target.exists() {
                let _ = fs::copy(&source, &target);
            }
        }
    }

    Ok(())
}

fn build_java_command(
    instance: &Value,
    version: &Value,
    auth: &MclcAuth,
    java_bin: &str,
    instance_dir: &Path,
    settings: &Value,
    context: &LaunchContext,
) -> Result<(Vec<String>, String), String> {
    let main_class = version
        .get("mainClass")
        .and_then(Value::as_str)
        .ok_or_else(|| "Sürüm tanımında mainClass bulunamadı".to_string())?;
    let version_name = instance
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or("");
    let version_type = instance
        .get("versionType")
        .and_then(Value::as_str)
        .unwrap_or("release");
    let mut uuid = auth.uuid.replace('-', "");
    if uuid.is_empty() {
        uuid = auth.uuid.clone();
    }
    let classpath = context
        .classpath
        .iter()
        .map(|path| path.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join(if cfg!(windows) { ";" } else { ":" });
    let library_directory = context.library_dir.to_string_lossy().to_string();
    let assets_root = context.assets_dir.to_string_lossy().to_string();
    let natives_directory = context.natives_dir.to_string_lossy().to_string();
    let log_config = context
        .log_config_path
        .as_ref()
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_default();
    let game_assets = if context.asset_index_name != "legacy" {
        context
            .assets_dir
            .join("virtual")
            .join(&context.asset_index_name)
            .to_string_lossy()
            .to_string()
    } else {
        context
            .assets_dir
            .parent()
            .unwrap_or(&context.assets_dir)
            .join("resources")
            .to_string_lossy()
            .to_string()
    };

    let mut variables = HashMap::new();
    variables.insert("auth_player_name".to_string(), auth.username.clone());
    variables.insert("version_name".to_string(), version_name.to_string());
    variables.insert(
        "game_directory".to_string(),
        instance_dir.to_string_lossy().to_string(),
    );
    variables.insert("assets_root".to_string(), assets_root.clone());
    variables.insert("assets_index_name".to_string(), context.asset_index_name.clone());
    variables.insert("auth_uuid".to_string(), uuid);
    variables.insert("auth_access_token".to_string(), auth.access_token.clone());
    variables.insert("auth_session".to_string(), auth.access_token.clone());
    variables.insert("clientid".to_string(), auth.client_token.clone());
    variables.insert("auth_xuid".to_string(), String::new());
    variables.insert("user_type".to_string(), auth.user_type.clone());
    variables.insert("version_type".to_string(), version_type.to_string());
    variables.insert("natives_directory".to_string(), natives_directory.clone());
    variables.insert("launcher_name".to_string(), "BuranLauncher".to_string());
    variables.insert("launcher_version".to_string(), "1.0.0".to_string());
    variables.insert("classpath".to_string(), classpath.clone());
    variables.insert(
        "classpath_separator".to_string(),
        if cfg!(windows) { ";" } else { ":" }.to_string(),
    );
    variables.insert("library_directory".to_string(), library_directory);
    variables.insert("resolution_width".to_string(), "854".to_string());
    variables.insert("resolution_height".to_string(), "480".to_string());
    variables.insert("game_assets".to_string(), game_assets);
    variables.insert("user_properties".to_string(), auth.user_properties.clone());
    variables.insert("path".to_string(), log_config);
    variables.insert("game_assets_index".to_string(), context.asset_index_name.clone());

    let mut jvm_args: Vec<String> = Vec::new();
    let ram_min = settings
        .get("ramMin")
        .and_then(Value::as_u64)
        .unwrap_or(1024);
    let ram_max = settings
        .get("ramMax")
        .and_then(Value::as_u64)
        .unwrap_or(4096);
    jvm_args.push(format!("-Xms{ram_min}M"));
    jvm_args.push(format!("-Xmx{ram_max}M"));

    let mut game_args: Vec<String> = Vec::new();
    if let Some(arguments) = version.get("arguments") {
        jvm_args.extend(version_argument_list(arguments, "jvm", &variables));
        game_args.extend(version_argument_list(arguments, "game", &variables));
    } else if let Some(legacy) = version.get("minecraftArguments").and_then(Value::as_str) {
        jvm_args.push(format!("-Djava.library.path={natives_directory}"));
        jvm_args.push("-cp".to_string());
        jvm_args.push(classpath.clone());
        game_args.extend(legacy_arguments(legacy, &variables));
    }

    if !jvm_args
        .iter()
        .any(|arg| arg.starts_with("-Djava.library.path="))
    {
        jvm_args.push(format!("-Djava.library.path={natives_directory}"));
    }
    let has_classpath_flag = jvm_args
        .iter()
        .any(|arg| arg == "-cp" || arg == "-classpath");
    if !has_classpath_flag {
        jvm_args.push("-cp".to_string());
        jvm_args.push(classpath);
    }

    if let Some(extra) = settings.get("extraArgs").and_then(Value::as_str) {
        if !extra.trim().is_empty() {
            game_args.extend(extra.split_whitespace().map(ToString::to_string));
        }
    }

    let mut command_args = jvm_args;
    command_args.push(main_class.to_string());
    command_args.extend(game_args);
    let _ = java_executable(java_bin);
    Ok((command_args, java_bin.to_string()))
}

pub async fn launch_instance(
    app: &AppHandle,
    store: &Store,
    paths: &AppPaths,
    instance_id: &str,
) -> Result<bool, String> {
    let instance = instance_manager::get_instance(store, instance_id)?;
    let active_account_id = store
        .get_string("activeAccountId")
        .ok_or_else(|| "Önce bir hesap seçmelisiniz".to_string())?;
    let account = store
        .array("accounts")
        .into_iter()
        .find(|account| account.get("id").and_then(Value::as_str) == Some(active_account_id.as_str()))
        .ok_or_else(|| "Seçili hesap bulunamadı".to_string())?;
    let account_id = account
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if is_running(instance_id).await {
        return Err("Bu profil zaten çalışıyor".to_string());
    }

    let settings = store.settings();
    let mut java_path = settings
        .get("javaPath")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if java_path.trim().is_empty() {
        java_path = java_manager::detect_system_java().unwrap_or_default();
    }
    if java_path.is_empty() {
        let instance_version = instance
            .get("version")
            .and_then(Value::as_str)
            .unwrap_or("");
        emit_status(
            app,
            instance_id,
            json!({ "task": "java-auto-download", "instanceVersion": instance_version, "stage": "autoDownloadStart" }),
        );
        java_path = java_manager::ensure_java_for_version(paths, instance_version, |payload| {
            let mut status = payload;
            status["task"] = json!("java-auto-download");
            status["instanceVersion"] = json!(instance_version);
            emit_status(app, instance_id, status);
        })
        .await
        .map(|path| path.to_string_lossy().to_string())
        .map_err(|err| {
            format!(
                "Java otomatik indirilemedi: {err} — Ayarlar üzerinden manuel bir Java yolu belirtebilirsiniz."
            )
        })?;
    }

    let auth = auth::get_mclc_auth(store, &app.state::<crate::secrets::SecretBox>(), &account_id)
        .await?;

    let instance_version = instance
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let loader = instance
        .get("loader")
        .and_then(Value::as_str)
        .unwrap_or("vanilla");
    let loader_version = instance
        .get("loaderVersion")
        .and_then(Value::as_str)
        .unwrap_or("");
    let effective_version = match loader {
        "fabric" if !loader_version.is_empty() => {
            format!("fabric-loader-{loader_version}-{instance_version}")
        }
        "quilt" if !loader_version.is_empty() => {
            format!("quilt-loader-{loader_version}-{instance_version}")
        }
        "forge" if !loader_version.is_empty() => {
            format!("{instance_version}-forge-{loader_version}")
        }
        "neoforge" if !loader_version.is_empty() && instance_version == "1.20.1" => {
            format!("{instance_version}-forge-{loader_version}")
        }
        "neoforge" if !loader_version.is_empty() => format!("neoforge-{loader_version}"),
        _ => instance_version.clone(),
    };

    emit_status(app, instance_id, json!({ "task": "preparing" }));
    let (version, base_id) = resolve_version(paths, &effective_version).await?;
    let context = prepare_client(app, paths, instance_id, &version, &base_id).await?;
    let instance_dir = paths.instance_dir(instance_id);
    fs::create_dir_all(&instance_dir).map_err(|err| format!("Profil klasörü oluşturulamadı: {err}"))?;

    let (command_args, java_bin) = build_java_command(
        &instance,
        &version,
        &auth,
        &java_path,
        &instance_dir,
        &settings,
        &context,
    )?;

    emit_status(app, instance_id, json!({ "task": "launching" }));
    let mut command = tokio::process::Command::new(&java_bin);
    command.args(&command_args);
    command.current_dir(&instance_dir);
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    command.stdin(Stdio::null());
    #[cfg(windows)]
    {
        command.creation_flags(0x0800_0000);
    }

    let mut child = command
        .spawn()
        .map_err(|err| format!("Oyun başlatılamadı (Java yolu: {java_bin}): {err}"))?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let child = std::sync::Arc::new(Mutex::new(child));
    active_processes()
        .lock()
        .await
        .insert(instance_id.to_string(), child.clone());

    let started_at = Instant::now();
    let app_for_output = app.clone();
    let instance_for_output = instance_id.to_string();
    if let Some(stdout) = stdout {
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                emit_log(&app_for_output, &instance_for_output, &line);
            }
        });
    }
    let app_for_error = app.clone();
    let instance_for_error = instance_id.to_string();
    if let Some(stderr) = stderr {
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                emit_log(&app_for_error, &instance_for_error, &line);
            }
        });
    }

    crate::discord::set_playing(
        instance
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("Minecraft"),
        &instance_version,
        loader,
    );

    let app_for_close = app.clone();
    let instance_for_close = instance_id.to_string();
    tokio::spawn(async move {
        let code = loop {
            tokio::time::sleep(Duration::from_millis(1000)).await;
            let mut guard = child.lock().await;
            match guard.try_wait() {
                Ok(Some(status)) => break status.code().unwrap_or(-1),
                Ok(None) => continue,
                Err(err) => {
                    crate::logger::error(&format!("Oyun durumu okunamadı: {err}"));
                    break -1;
                }
            }
        };
        active_processes().lock().await.remove(&instance_for_close);
        let duration_ms = started_at.elapsed().as_millis() as u64;
        {
            let state = app_for_close.state::<Store>();
            instance_manager::record_play_session(&state, &instance_for_close, duration_ms);
        }
        crate::discord::clear_activity();
        if let Some(window) = app_for_close.get_webview_window("main") {
            let _ = window.show();
            let _ = window.unminimize();
        }
        emit_closed(&app_for_close, &instance_for_close, code);
    });

    Ok(true)
}

pub async fn is_running(instance_id: &str) -> bool {
    active_processes().lock().await.contains_key(instance_id)
}

pub async fn kill_instance(instance_id: &str) -> bool {
    let child = active_processes().lock().await.remove(instance_id);
    if let Some(child) = child {
        let mut guard = child.lock().await;
        let _ = guard.start_kill();
        crate::discord::clear_activity();
        true
    } else {
        false
    }
}