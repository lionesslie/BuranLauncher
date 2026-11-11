use crate::http;
use crate::java_manager;
use crate::loader_manager;
use crate::paths::AppPaths;
use crate::store::Store;
use crate::version_manager;
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Stdio;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin};
use tokio::sync::Mutex;
use uuid::Uuid;

struct ServerProcess {
    child: Arc<Mutex<Child>>,
    stdin: Arc<Mutex<Option<ChildStdin>>>,
}

fn active_servers() -> &'static Mutex<HashMap<String, Arc<ServerProcess>>> {
    static SERVERS: OnceLock<Mutex<HashMap<String, Arc<ServerProcess>>>> = OnceLock::new();
    SERVERS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn list_servers(store: &Store) -> Value {
    Value::Array(store.array("servers"))
}

pub fn get_server(store: &Store, server_id: &str) -> Result<Value, String> {
    store
        .array("servers")
        .into_iter()
        .find(|server| server.get("id").and_then(Value::as_str) == Some(server_id))
        .ok_or_else(|| format!("Sunucu bulunamadı: {server_id}"))
}

fn read_properties(file_path: &Path) -> Map<String, Value> {
    let Ok(raw) = fs::read_to_string(file_path) else {
        return Map::new();
    };
    let mut properties = Map::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(index) = trimmed.find('=') {
            properties.insert(
                trimmed[..index].to_string(),
                Value::String(trimmed[index + 1..].to_string()),
            );
        }
    }
    properties
}

fn property_value(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number.to_string(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn write_properties(file_path: &Path, patch: &Map<String, Value>) {
    let existing = fs::read_to_string(file_path).unwrap_or_default();
    let mut seen = std::collections::HashSet::new();
    let mut output_lines: Vec<String> = Vec::new();
    for line in existing.split('\n') {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            output_lines.push(line.to_string());
            continue;
        }
        let Some(index) = trimmed.find('=') else {
            output_lines.push(line.to_string());
            continue;
        };
        let key = &trimmed[..index];
        if let Some(value) = patch.get(key) {
            seen.insert(key.to_string());
            output_lines.push(format!("{key}={}", property_value(value)));
        } else {
            output_lines.push(line.to_string());
        }
    }
    for (key, value) in patch {
        if !seen.contains(key) {
            output_lines.push(format!("{key}={}", property_value(value)));
        }
    }
    let _ = fs::write(file_path, output_lines.join("\n"));
}

pub fn get_server_properties(
    store: &Store,
    paths: &AppPaths,
    server_id: &str,
) -> Result<Value, String> {
    get_server(store, server_id)?;
    let properties = read_properties(&paths.server_dir(server_id).join("server.properties"));
    Ok(Value::Object(properties))
}

pub fn set_server_properties(
    store: &Store,
    paths: &AppPaths,
    server_id: &str,
    patch: &Value,
) -> Result<Value, String> {
    get_server(store, server_id)?;
    let path = paths.server_dir(server_id).join("server.properties");
    let patch_map = patch
        .as_object()
        .ok_or_else(|| "Özellikler nesne olmalı".to_string())?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("Klasör oluşturulamadı: {err}"))?;
    }
    write_properties(&path, patch_map);
    Ok(Value::Object(read_properties(&path)))
}

pub async fn create_server<F>(
    app: &AppHandle,
    store: &Store,
    paths: &AppPaths,
    data: &Value,
    on_progress: F,
) -> Result<Value, String>
where
    F: Fn(Value),
{
    let name = data
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if name.is_empty() {
        return Err("Sunucu adı boş olamaz".to_string());
    }
    let eula_accepted = data
        .get("eulaAccepted")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !eula_accepted {
        return Err("Mojang EULA kabul edilmeden sunucu oluşturulamaz.".to_string());
    }
    let version = data
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if version.is_empty() {
        return Err("Sunucu Minecraft sürümü seçilmeli".to_string());
    }
    let loader = data
        .get("loader")
        .and_then(Value::as_str)
        .unwrap_or("vanilla")
        .to_string();
    let loader_version = data
        .get("loaderVersion")
        .and_then(Value::as_str)
        .map(ToString::to_string);

    let server_id = Uuid::new_v4().to_string();
    let server_dir = paths.server_dir(&server_id);
    fs::create_dir_all(&server_dir).map_err(|err| format!("Sunucu klasörü oluşturulamadı: {err}"))?;

    let install_result: Result<Value, String> = async {
        match loader.as_str() {
            "vanilla" => {
                on_progress(json!({ "stage": "downloadingServer" }));
                let detail = version_manager::get_version_detail(&version).await?;
                let server_dl = detail
                    .pointer("/downloads/server")
                    .ok_or_else(|| {
                        "Bu sürüm için resmi sunucu dosyası yok (bazı eski sürümlerde sunucu yayınlanmamıştır)"
                            .to_string()
                    })?;
                let url = server_dl
                    .get("url")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "Sunucu indirme bağlantısı bulunamadı".to_string())?;
                let sha1 = server_dl.get("sha1").and_then(Value::as_str);
                let bytes = http::download_bytes(url)
                    .await
                    .map_err(|err| format!("Sunucu dosyası indirilemedi: {err}"))?;
                if let Some(expected) = sha1 {
                    let actual = http::sha1_hex(&bytes);
                    if actual != expected {
                        return Err("Sunucu dosyası SHA-1 doğrulaması başarısız".to_string());
                    }
                }
                fs::write(server_dir.join("server.jar"), bytes)
                    .map_err(|err| format!("Sunucu dosyası yazılamadı: {err}"))?;
                Ok(json!({ "type": "jar", "jarFile": "server.jar" }))
            }
            "fabric" => {
                let loader_version = loader_version
                    .as_deref()
                    .ok_or_else(|| "Fabric yükleyici sürümü belirtilmedi".to_string())?;
                loader_manager::install_fabric_server(
                    app,
                    &server_dir,
                    &version,
                    loader_version,
                )
                .await
            }
            "quilt" => {
                let loader_version = loader_version
                    .as_deref()
                    .ok_or_else(|| "Quilt yükleyici sürümü belirtilmedi".to_string())?;
                loader_manager::install_quilt_server(
                    app,
                    &server_dir,
                    &version,
                    loader_version,
                )
                .await
            }
            "forge" | "neoforge" => {
                let loader_version = loader_version
                    .as_deref()
                    .ok_or_else(|| "Forge/NeoForge yükleyici sürümü belirtilmedi".to_string())?;
                let settings = store.settings();
                let java_path = settings
                    .get("javaPath")
                    .and_then(Value::as_str)
                    .filter(|path| !path.trim().is_empty())
                    .map(ToString::to_string)
                    .or_else(java_manager::detect_system_java)
                    .or_else(|| {
                        java_manager::find_cached_java(
                            paths,
                            java_manager::required_java_version_for_mc(&version),
                        )
                        .map(|path| path.to_string_lossy().to_string())
                    })
                    .ok_or_else(|| {
                        "Forge/NeoForge sunucu kurulumu için Java gerekli (Ayarlar'dan kurun)"
                            .to_string()
                    })?;
                loader_manager::install_forge_like_server(
                    app,
                    &server_dir,
                    &loader,
                    &version,
                    loader_version,
                    &java_path,
                )
                .await
            }
            other => Err(format!("Bilinmeyen yükleyici: {other}")),
        }
    }
    .await;

    let launch_command = match install_result {
        Ok(command) => command,
        Err(err) => {
            let _ = fs::remove_dir_all(&server_dir);
            return Err(err);
        }
    };

    fs::write(
        server_dir.join("eula.txt"),
        "# https://www.minecraft.net/en-us/eula\neula=true\n",
    )
    .map_err(|err| format!("eula.txt yazılamadı: {err}"))?;

    let mut defaults = Map::new();
    for (key, value) in [
        ("server-port", "25565"),
        ("max-players", "20"),
        ("gamemode", "survival"),
        ("difficulty", "normal"),
        ("motd", name.as_str()),
        ("pvp", "true"),
        ("online-mode", "true"),
        ("white-list", "false"),
        ("level-name", "world"),
    ] {
        defaults.insert(key.to_string(), Value::String(value.to_string()));
    }
    write_properties(&server_dir.join("server.properties"), &defaults);

    let ram_min = data.get("ramMin").and_then(Value::as_u64).unwrap_or(1024);
    let ram_max = data.get("ramMax").and_then(Value::as_u64).unwrap_or(2048);
    let server = json!({
        "id": server_id,
        "name": name,
        "version": version,
        "loader": loader,
        "loaderVersion": loader_version,
        "ramMin": ram_min,
        "ramMax": ram_max,
        "launchCommand": launch_command,
        "createdAt": http::now_millis(),
        "lastStarted": Value::Null
    });

    store.push_array("servers", server.clone());
    on_progress(json!({ "stage": "done" }));
    Ok(server)
}

pub fn update_server_settings(
    store: &Store,
    server_id: &str,
    patch: &Value,
) -> Result<Value, String> {
    let mut servers = store.array("servers");
    let index = servers
        .iter()
        .position(|server| server.get("id").and_then(Value::as_str) == Some(server_id))
        .ok_or_else(|| "Sunucu bulunamadı".to_string())?;
    let mut updated = servers[index].clone();
    if let (Some(target), Some(patch_map)) = (updated.as_object_mut(), patch.as_object()) {
        for (key, value) in patch_map {
            target.insert(key.clone(), value.clone());
        }
    }
    updated["id"] = Value::String(server_id.to_string());
    servers[index] = updated.clone();
    store.set("servers", Value::Array(servers));
    Ok(updated)
}

pub async fn delete_server(
    store: &Store,
    paths: &AppPaths,
    server_id: &str,
) -> Result<(), String> {
    if is_running(server_id).await {
        return Err("Çalışan bir sunucu silinemez, önce durdurun.".to_string());
    }
    store.map_array("servers", |servers| {
        servers.retain(|server| server.get("id").and_then(Value::as_str) != Some(server_id));
    });
    let dir = paths.server_dir(server_id);
    if dir.exists() {
        fs::remove_dir_all(dir).map_err(|err| format!("Sunucu klasörü silinemedi: {err}"))?;
    }
    Ok(())
}

pub fn open_server_folder(paths: &AppPaths, server_id: &str) -> Result<(), String> {
    let dir = paths.server_dir(server_id);
    if !dir.exists() {
        return Err("Sunucu klasörü bulunamadı".to_string());
    }
    open::that(dir).map_err(|err| format!("Klasör açılamadı: {err}"))
}

pub async fn is_running(server_id: &str) -> bool {
    active_servers().lock().await.contains_key(server_id)
}

fn emit_server_log(app: &AppHandle, server_id: &str, line: &str) {
    let _ = app.emit(
        "servers:log",
        json!({ "serverId": server_id, "line": line }),
    );
}

fn emit_server_closed(app: &AppHandle, server_id: &str, code: i32) {
    let _ = app.emit(
        "servers:closed",
        json!({ "serverId": server_id, "code": code }),
    );
}

fn apply_java_env(command: &mut tokio::process::Command, java_path: &str) {
    let Some(bin_dir) = Path::new(java_path).parent() else {
        return;
    };
    let mut entries = vec![bin_dir.to_path_buf()];
    if let Some(existing) = std::env::var_os("PATH") {
        entries.extend(std::env::split_paths(&existing));
    }
    if let Ok(joined) = std::env::join_paths(entries) {
        command.env("PATH", joined);
    }
    if let Some(java_home) = bin_dir.parent() {
        command.env("JAVA_HOME", java_home);
    }
}

pub async fn start_server(
    app: &AppHandle,
    store: &Store,
    paths: &AppPaths,
    server_id: &str,
) -> Result<bool, String> {
    if is_running(server_id).await {
        return Err("Sunucu zaten çalışıyor".to_string());
    }
    let server = get_server(store, server_id)?;
    let server_dir = paths.server_dir(server_id);
    fs::create_dir_all(&server_dir).map_err(|err| format!("Sunucu klasörü oluşturulamadı: {err}"))?;

    let launch_command = server
        .get("launchCommand")
        .ok_or_else(|| "Sunucu başlatma bilgisi eksik".to_string())?;
    let launch_type = launch_command
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("jar");

    let settings = store.settings();
    let server_version = server.get("version").and_then(Value::as_str).unwrap_or("");
    let resolved_java = settings
        .get("javaPath")
        .and_then(Value::as_str)
        .filter(|path| !path.trim().is_empty())
        .map(ToString::to_string)
        .or_else(java_manager::detect_system_java)
        .or_else(|| {
            java_manager::find_cached_java(
                paths,
                java_manager::required_java_version_for_mc(server_version),
            )
            .map(|path| path.to_string_lossy().to_string())
        });

    let mut command = if launch_type == "script" {
        let script_file = launch_command
            .get("scriptFile")
            .and_then(Value::as_str)
            .ok_or_else(|| "Sunucu betiği bulunamadı".to_string())?;
        let script_path = server_dir.join(script_file);
        if !script_path.exists() {
            return Err(format!("Sunucu betiği bulunamadı: {}", script_path.display()));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755));
        }
        let mut command = if cfg!(windows) {
            let mut cmd = tokio::process::Command::new("cmd");
            cmd.arg("/C").arg(script_file).arg("nogui");
            cmd
        } else {
            let mut cmd = tokio::process::Command::new("/bin/sh");
            cmd.arg(&script_path).arg("nogui");
            cmd
        };
        command.current_dir(&server_dir);
        command
    } else {
        let jar_file = launch_command
            .get("jarFile")
            .and_then(Value::as_str)
            .ok_or_else(|| "Sunucu jar dosyası bulunamadı".to_string())?;
        let java_path = resolved_java.clone().ok_or_else(|| {
            "Java bulunamadı. Ayarlar'dan Java yolunu belirtin veya otomatik indirin.".to_string()
        })?;
        let ram_min = server
            .get("ramMin")
            .and_then(Value::as_u64)
            .unwrap_or(1024);
        let ram_max = server
            .get("ramMax")
            .and_then(Value::as_u64)
            .unwrap_or(2048);
        let mut command = tokio::process::Command::new(&java_path);
        command
            .arg(format!("-Xms{ram_min}M"))
            .arg(format!("-Xmx{ram_max}M"))
            .arg("-jar")
            .arg(jar_file)
            .arg("nogui");
        command.current_dir(&server_dir);
        command
    };

    if let Some(java_path) = resolved_java.as_deref() {
        apply_java_env(&mut command, java_path);
    }

    command.stdout(Stdio::piped()).stderr(Stdio::piped()).stdin(Stdio::piped());
    #[cfg(windows)]
    {
        command.creation_flags(0x0800_0000);
    }

    let mut child = command
        .spawn()
        .map_err(|err| format!("Sunucu başlatılamadı: {err}"))?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdin = child.stdin.take();
    let child = Arc::new(Mutex::new(child));
    let process = Arc::new(ServerProcess {
        child: child.clone(),
        stdin: Arc::new(Mutex::new(stdin)),
    });
    active_servers()
        .lock()
        .await
        .insert(server_id.to_string(), process);

    let _ = update_server_settings(store, server_id, &json!({ "lastStarted": http::now_millis() }));

    let app_for_output = app.clone();
    let server_for_output = server_id.to_string();
    if let Some(stdout) = stdout {
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                emit_server_log(&app_for_output, &server_for_output, &line);
            }
        });
    }
    let app_for_error = app.clone();
    let server_for_error = server_id.to_string();
    if let Some(stderr) = stderr {
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                emit_server_log(&app_for_error, &server_for_error, &line);
            }
        });
    }

    let app_for_close = app.clone();
    let server_for_close = server_id.to_string();
    tokio::spawn(async move {
        let code = loop {
            tokio::time::sleep(Duration::from_millis(1000)).await;
            let mut guard = child.lock().await;
            match guard.try_wait() {
                Ok(Some(status)) => break status.code().unwrap_or(-1),
                Ok(None) => continue,
                Err(err) => {
                    crate::logger::error(&format!("Sunucu durumu okunamadı: {err}"));
                    break -1;
                }
            }
        };
        active_servers().lock().await.remove(&server_for_close);
        emit_server_closed(&app_for_close, &server_for_close, code);
    });

    Ok(true)
}

pub async fn stop_server(server_id: &str) -> Result<bool, String> {
    let process = active_servers().lock().await.get(server_id).cloned();
    let Some(process) = process else {
        return Ok(false);
    };
    let closed_stdin = {
        let mut stdin = process.stdin.lock().await;
        if let Some(stdin) = stdin.as_mut() {
            if stdin.write_all(b"stop\n").await.is_err() {
                let mut child = process.child.lock().await;
                let _ = child.start_kill();
                return Ok(true);
            }
            let _ = stdin.flush().await;
        } else {
            let mut child = process.child.lock().await;
            let _ = child.start_kill();
            return Ok(true);
        }
        stdin.take()
    };
    drop(closed_stdin);

    let process_for_timer = process.clone();
    let server_for_timer = server_id.to_string();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(15)).await;
        let current = active_servers()
            .lock()
            .await
            .get(&server_for_timer)
            .cloned();
        if let Some(current) = current {
            if Arc::ptr_eq(&current, &process_for_timer) {
                crate::logger::warn(
                    "Sunucu \"stop\" komutuna 15 saniyede yanıt vermedi, zorla kapatılıyor.",
                );
                let mut child = process_for_timer.child.lock().await;
                let _ = child.start_kill();
            }
        }
    });
    Ok(true)
}

pub async fn send_command(server_id: &str, command: &str) -> Result<(), String> {
    let process = active_servers().lock().await.get(server_id).cloned();
    let Some(process) = process else {
        return Err("Sunucu çalışmıyor".to_string());
    };
    let mut stdin = process.stdin.lock().await;
    let Some(stdin) = stdin.as_mut() else {
        return Err("Sunucu konsolu kullanılamıyor".to_string());
    };
    stdin
        .write_all(format!("{command}\n").as_bytes())
        .await
        .map_err(|err| format!("Komut gönderilemedi: {err}"))?;
    stdin
        .flush()
        .await
        .map_err(|err| format!("Komut gönderilemedi: {err}"))
}