use crate::auth;
use crate::background;
use crate::i18n;
use crate::instance_manager;
use crate::java_manager;
use crate::launch_manager;
use crate::loader_manager;
use crate::modrinth;
use crate::paths::AppPaths;
use crate::secrets::SecretBox;
use crate::server_manager;
use crate::store::Store;
use crate::updater;
use crate::version_manager;
use crate::world_manager;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_autostart::ManagerExt;

fn arg(args: &[Value], index: usize) -> Value {
    args.get(index).cloned().unwrap_or(Value::Null)
}

fn arg_string(args: &[Value], index: usize) -> Option<String> {
    args.get(index)
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn param_string<'a>(params: &'a Value, key: &str) -> Option<&'a str> {
    params
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
}

fn param_u64(params: &Value, key: &str, fallback: u64) -> u64 {
    params.get(key).and_then(Value::as_u64).unwrap_or(fallback)
}

/// Java sürümünü gerçekten çalıştırıp öğrenir: `<java> -version` çıktısındaki
/// `version "25.0.5"` kalıbından sürümü döndürür.
fn probe_java_version(exe: &std::path::Path) -> Option<String> {
    let mut command = std::process::Command::new(exe);
    command.arg("-version");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let output = command.output().ok()?;
    let text = String::from_utf8_lossy(&output.stderr);
    let marker = "version \"";
    let start = text.find(marker)? + marker.len();
    let rest = &text[start..];
    let end = rest.find('"')?;
    let version = rest[..end].trim().to_string();
    if version.is_empty() {
        None
    } else {
        Some(version)
    }
}

/// Ayarlardaki yol → kurulu (cache'lenmiş) JDK → sistem Java'sı sırasıyla
/// kullanılabilir bir java çalıştırılabilir dosyası bulur.
fn resolve_java_candidates(store: &Store, paths: &AppPaths) -> Vec<std::path::PathBuf> {
    let java_exe = if cfg!(windows) { "java.exe" } else { "java" };
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    let configured = store.get_setting_string("javaPath").unwrap_or_default();
    if !configured.is_empty() {
        let path = std::path::PathBuf::from(&configured);
        if path.is_file() {
            candidates.push(path);
        } else {
            candidates.push(path.join("bin").join(java_exe));
            candidates.push(path.join(java_exe));
        }
    }
    if let Ok(entries) = std::fs::read_dir(paths.java_root()) {
        let mut dirs: Vec<std::path::PathBuf> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect();
        dirs.sort();
        for dir in dirs {
            candidates.push(dir.join("bin").join(java_exe));
        }
    }
    if let Some(system_java) = java_manager::detect_system_java() {
        candidates.push(std::path::PathBuf::from(system_java));
    }
    candidates
}

async fn install_loader_after_import(
    app: &AppHandle,
    store: &Store,
    paths: &AppPaths,
    instance: &Value,
) -> Result<(), String> {
    let loader = instance
        .get("loader")
        .and_then(Value::as_str)
        .unwrap_or("vanilla");
    let loader_version = instance
        .get("loaderVersion")
        .and_then(Value::as_str)
        .unwrap_or("");
    let mc_version = instance
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or("");
    if loader == "vanilla" || loader_version.is_empty() || mc_version.is_empty() {
        return Ok(());
    }
    let profile_id = match loader {
        "fabric" => format!("fabric-loader-{loader_version}-{mc_version}"),
        "quilt" => format!("quilt-loader-{loader_version}-{mc_version}"),
        "forge" => format!("{mc_version}-forge-{loader_version}"),
        "neoforge" if mc_version == "1.20.1" => {
            format!("{mc_version}-forge-{loader_version}")
        }
        "neoforge" => format!("neoforge-{loader_version}"),
        _ => return Ok(()),
    };
    if paths.version_json_path(&profile_id).exists() {
        return Ok(());
    }
    match loader {
        "fabric" => {
            let _ = loader_manager::install_fabric(paths, app, mc_version, loader_version).await?;
        }
        "quilt" => {
            let _ = loader_manager::install_quilt(paths, app, mc_version, loader_version).await?;
        }
        "forge" | "neoforge" => {
            let java_path = store
                .get_setting_string("javaPath")
                .filter(|path| !path.trim().is_empty())
                .or_else(java_manager::detect_system_java)
                .or_else(|| {
                    java_manager::find_cached_java(
                        paths,
                        java_manager::required_java_version_for_mc(mc_version),
                    )
                    .map(|path| path.to_string_lossy().to_string())
                })
                .ok_or_else(|| "Yükleyici kurulumu için Java gerekli".to_string())?;
            let _ =
                loader_manager::install_forge_like(paths, app, loader, mc_version, loader_version, &java_path)
                    .await?;
        }
        _ => {}
    }
    Ok(())
}

fn emit_progress_with_ids(
    app: &AppHandle,
    event: &str,
    project_id: &str,
    instance_id: &str,
    payload: Value,
) {
    let mut payload = payload;
    payload["projectId"] = json!(project_id);
    payload["instanceId"] = json!(instance_id);
    let _ = app.emit(event, payload);
}

#[tauri::command]
pub async fn buran_invoke(
    app: AppHandle,
    channel: String,
    args: Vec<Value>,
) -> Result<Value, String> {
    let store = app.state::<Store>();
    let paths = app.state::<AppPaths>();
    let secrets = app.state::<SecretBox>();

    match channel.as_str() {
        "i18n:get" => Ok(i18n::get_translations(&store)),
        "i18n:setLang" => {
            let lang = arg_string(&args, 0).unwrap_or_default();
            i18n::set_lang(&store, &lang).map(Value::String)
        }

        "accounts:list" => Ok(auth::list_accounts(&store)),
        "accounts:getActive" => Ok(auth::get_active_account(&store)),
        "accounts:addOffline" => {
            let username = arg_string(&args, 0).unwrap_or_default();
            auth::add_offline_account(&store, &username)
        }
        "accounts:loginMicrosoft" => auth::login_with_microsoft(&app, &store, &secrets).await,
        "accounts:setActive" => {
            let id = arg_string(&args, 0).unwrap_or_default();
            auth::set_active_account(&store, &id).map(Value::String)
        }
        "accounts:remove" => {
            let id = arg_string(&args, 0).unwrap_or_default();
            auth::remove_account(&store, &id);
            Ok(Value::Null)
        }

        "versions:list" => {
            let version_type = arg_string(&args, 0).unwrap_or_else(|| "release".to_string());
            version_manager::list_versions(&version_type).await
        }

        "loader:fabricVersions" => loader_manager::list_fabric_loader_versions().await,
        "loader:quiltVersions" => loader_manager::list_quilt_loader_versions().await,
        "loader:install" => {
            let params = arg(&args, 0);
            let loader_type = param_string(&params, "type").unwrap_or("vanilla");
            let mc_version = param_string(&params, "mcVersion").unwrap_or("");
            let loader_version = param_string(&params, "loaderVersion").unwrap_or("");
            match loader_type {
                "fabric" => {
                    loader_manager::install_fabric(&paths, &app, mc_version, loader_version)
                        .await
                        .map(Value::String)
                }
                "quilt" => {
                    loader_manager::install_quilt(&paths, &app, mc_version, loader_version)
                        .await
                        .map(Value::String)
                }
                "forge" | "neoforge" => {
                    let java_path = store
                        .get_setting_string("javaPath")
                        .filter(|path| !path.trim().is_empty())
                        .or_else(java_manager::detect_system_java)
                        .or_else(|| {
                            java_manager::find_cached_java(
                                &paths,
                                java_manager::required_java_version_for_mc(mc_version),
                            )
                            .map(|path| path.to_string_lossy().to_string())
                        })
                        .ok_or_else(|| "Yükleyici kurulumu için Java gerekli".to_string())?;
                    loader_manager::install_forge_like(
                        &paths,
                        &app,
                        loader_type,
                        mc_version,
                        loader_version,
                        &java_path,
                    )
                    .await
                    .map(Value::String)
                }
                other => Err(format!("Bilinmeyen yükleyici türü: {other}")),
            }
        }

        "instances:list" => Ok(instance_manager::list_instances(&store)),
        "instances:create" => instance_manager::create_instance(&store, &paths, &arg(&args, 0)),
        "instances:update" => {
            let params = arg(&args, 0);
            let id = param_string(&params, "id").unwrap_or("");
            let patch = params.get("patch").cloned().unwrap_or_else(|| json!({}));
            instance_manager::update_instance(&store, id, &patch)
        }
        "instances:duplicate" => {
            let params = arg(&args, 0);
            let id = param_string(&params, "id").unwrap_or("");
            let new_name = param_string(&params, "newName").map(ToString::to_string);
            instance_manager::duplicate_instance(&store, &paths, id, new_name)
        }
        "instances:delete" => {
            let id = arg_string(&args, 0).unwrap_or_default();
            instance_manager::delete_instance(&store, &paths, &id).map(|_| Value::Null)
        }
        "instances:export" => {
            let id = arg_string(&args, 0).unwrap_or_default();
            let instance = instance_manager::get_instance(&store, &id)?;
            let default_name = format!(
                "{}.zip",
                instance
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("profil")
            );
            let Some(file) = app
                .dialog()
                .file()
                .set_file_name(default_name)
                .add_filter("Zip Arşivi", &["zip"])
                .blocking_save_file()
            else {
                return Ok(Value::Null);
            };
            let path = file.into_path().map_err(|err| err.to_string())?;
            instance_manager::export_instance(&store, &paths, &id, &path).map(Value::String)
        }
        "instances:import" => {
            let Some(file) = app
                .dialog()
                .file()
                .add_filter("Zip Arşivi", &["zip"])
                .blocking_pick_file()
            else {
                return Ok(Value::Null);
            };
            let path = file.into_path().map_err(|err| err.to_string())?;
            instance_manager::import_instance(&store, &paths, &path)
        }
        "instances:openFolder" => {
            let id = arg_string(&args, 0).unwrap_or_default();
            let dir = paths.instance_dir(&id);
            open::that(dir).map_err(|err| format!("Klasör açılamadı: {err}"))?;
            Ok(Value::Null)
        }

        "launch:start" => {
            let instance_id = arg_string(&args, 0).unwrap_or_default();
            let result =
                launch_manager::launch_instance(&app, &store, &paths, &instance_id).await?;
            let behavior = store
                .get_setting_string("launchBehavior")
                .unwrap_or_else(|| "minimize".to_string());
            if let Some(window) = app.get_webview_window("main") {
                match behavior.as_str() {
                    "minimize" => {
                        let _ = window.minimize();
                    }
                    "close" => {
                        let _ = window.hide();
                    }
                    _ => {}
                }
            }
            Ok(json!(result))
        }
        "launch:kill" => {
            let instance_id = arg_string(&args, 0).unwrap_or_default();
            Ok(json!(launch_manager::kill_instance(&instance_id).await))
        }
        "launch:isRunning" => {
            let instance_id = arg_string(&args, 0).unwrap_or_default();
            Ok(json!(launch_manager::is_running(&instance_id).await))
        }

        "modrinth:search" => {
            let params = arg(&args, 0);
            let query = param_string(&params, "query").unwrap_or("");
            let project_type = param_string(&params, "projectType").unwrap_or("mod");
            let loader = param_string(&params, "loader");
            let game_version = param_string(&params, "gameVersion");
            let sort = param_string(&params, "sort").unwrap_or("relevance");
            modrinth::search(
                query,
                project_type,
                loader,
                game_version,
                sort,
                param_u64(&params, "offset", 0),
                param_u64(&params, "limit", 20),
            )
            .await
        }
        "modrinth:getProject" => {
            let id = arg_string(&args, 0).unwrap_or_default();
            modrinth::get_project(&id).await
        }
        "modrinth:getVersions" => {
            let params = arg(&args, 0);
            let id = param_string(&params, "id").unwrap_or("");
            let opts = params.get("opts").cloned().unwrap_or(Value::Null);
            let loader = param_string(&opts, "loader");
            let game_version = param_string(&opts, "gameVersion");
            modrinth::list_project_versions(id, loader, game_version).await
        }
        "modrinth:installMod" => {
            let params = arg(&args, 0);
            let project_id = param_string(&params, "projectId").unwrap_or("").to_string();
            let instance_id = param_string(&params, "instanceId").unwrap_or("").to_string();
            let version_id = param_string(&params, "versionId").map(ToString::to_string);
            let project_for_progress = project_id.clone();
            let instance_for_progress = instance_id.clone();
            modrinth::install_mod_to_instance(
                &paths,
                &store,
                &project_id,
                &instance_id,
                version_id.as_deref(),
                |payload| {
                    emit_progress_with_ids(
                        &app,
                        "modrinth:progress",
                        &project_for_progress,
                        &instance_for_progress,
                        payload,
                    )
                },
            )
            .await
            .map(|_| json!(true))
            .map_err(|err| {
                let _ = app.emit(
                    "modrinth:progress",
                    json!({
                        "stage": "error",
                        "message": err,
                        "projectId": project_for_progress,
                        "instanceId": instance_for_progress
                    }),
                );
                err
            })
        }
        "modrinth:listInstalled" => {
            let instance_id = arg_string(&args, 0).unwrap_or_default();
            Ok(modrinth::list_installed_mods(&paths, &instance_id))
        }
        "modrinth:toggleMod" => {
            let params = arg(&args, 0);
            let instance_id = param_string(&params, "instanceId").unwrap_or("");
            let filename = param_string(&params, "filename").unwrap_or("");
            let enable = params
                .get("enable")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            modrinth::toggle_mod(&paths, instance_id, filename, enable).map(Value::String)
        }
        "modrinth:removeMod" => {
            let params = arg(&args, 0);
            let instance_id = param_string(&params, "instanceId").unwrap_or("");
            let filename = param_string(&params, "filename").unwrap_or("");
            modrinth::remove_mod(&paths, instance_id, filename).map(|_| Value::Null)
        }
        "modrinth:checkUpdates" => {
            let instance_id = arg_string(&args, 0).unwrap_or_default();
            modrinth::check_for_updates(&paths, &store, &instance_id).await
        }
        "modrinth:updateMod" => {
            let params = arg(&args, 0);
            let instance_id = param_string(&params, "instanceId").unwrap_or("").to_string();
            let project_id = param_string(&params, "projectId").unwrap_or("").to_string();
            let project_for_progress = project_id.clone();
            let instance_for_progress = instance_id.clone();
            modrinth::update_mod(
                &paths,
                &store,
                &instance_id,
                &project_id,
                |payload| {
                    emit_progress_with_ids(
                        &app,
                        "modrinth:progress",
                        &project_for_progress,
                        &instance_for_progress,
                        payload,
                    )
                },
            )
            .await
            .map(|_| json!(true))
            .map_err(|err| {
                let _ = app.emit(
                    "modrinth:progress",
                    json!({
                        "stage": "error",
                        "message": err,
                        "projectId": project_for_progress,
                        "instanceId": instance_for_progress
                    }),
                );
                err
            })
        }
        "modrinth:searchContent" => {
            let params = arg(&args, 0);
            let query = param_string(&params, "query").unwrap_or("");
            let project_type = param_string(&params, "projectType").unwrap_or("resourcepack");
            let loader = param_string(&params, "loader");
            let game_version = param_string(&params, "gameVersion");
            let sort = param_string(&params, "sort").unwrap_or("relevance");
            modrinth::search(
                query,
                project_type,
                loader,
                game_version,
                sort,
                param_u64(&params, "offset", 0),
                param_u64(&params, "limit", 20),
            )
            .await
        }
        "modrinth:installContent" => {
            let params = arg(&args, 0);
            let project_id = param_string(&params, "projectId").unwrap_or("").to_string();
            let instance_id = param_string(&params, "instanceId").unwrap_or("").to_string();
            let project_type = param_string(&params, "projectType").unwrap_or("resourcepack");
            let project_for_progress = project_id.clone();
            let instance_for_progress = instance_id.clone();
            modrinth::install_content_to_instance(
                &paths,
                &store,
                &project_id,
                &instance_id,
                None,
                project_type,
                |payload| {
                    emit_progress_with_ids(
                        &app,
                        "modrinth:progress",
                        &project_for_progress,
                        &instance_for_progress,
                        payload,
                    )
                },
            )
            .await
            .map(|_| json!(true))
            .map_err(|err| {
                let _ = app.emit(
                    "modrinth:progress",
                    json!({
                        "stage": "error",
                        "message": err,
                        "projectId": project_for_progress,
                        "instanceId": instance_for_progress
                    }),
                );
                err
            })
        }
        "modrinth:listInstalledContent" => {
            let params = arg(&args, 0);
            let instance_id = param_string(&params, "instanceId").unwrap_or("");
            let project_type = param_string(&params, "projectType").unwrap_or("resourcepack");
            modrinth::list_installed_content(&paths, instance_id, project_type).await
        }
        "modrinth:removeContent" => {
            let params = arg(&args, 0);
            let instance_id = param_string(&params, "instanceId").unwrap_or("");
            let project_type = param_string(&params, "projectType").unwrap_or("resourcepack");
            let filename = param_string(&params, "filename").unwrap_or("");
            modrinth::remove_content(&paths, instance_id, project_type, filename)
                .map(|_| Value::Null)
        }
        "modrinth:searchModpacks" => {
            let params = arg(&args, 0);
            let query = param_string(&params, "query").unwrap_or("");
            let game_version = param_string(&params, "gameVersion");
            let sort = param_string(&params, "sort").unwrap_or("relevance");
            modrinth::search(
                query,
                "modpack",
                None,
                game_version,
                sort,
                param_u64(&params, "offset", 0),
                param_u64(&params, "limit", 20),
            )
            .await
        }
        "modrinth:getModpackVersions" => {
            let project_id = arg_string(&args, 0).unwrap_or_default();
            modrinth::list_project_versions(&project_id, None, None).await
        }
        "modrinth:installModpackFromProject" => {
            let params = arg(&args, 0);
            let project_id = param_string(&params, "projectId").unwrap_or("");
            let version_id = param_string(&params, "versionId").map(ToString::to_string);
            let instance = modrinth::install_modpack_from_project(
                &paths,
                &store,
                project_id,
                version_id.as_deref(),
                |payload| {
                    let _ = app.emit("modpack:progress", payload);
                },
            )
            .await?;
            install_loader_after_import(&app, &store, &paths, &instance).await?;
            Ok(instance)
        }
        "modrinth:installModpackFile" => {
            let Some(file) = app
                .dialog()
                .file()
                .add_filter("Modrinth Modpack", &["mrpack"])
                .blocking_pick_file()
            else {
                return Ok(Value::Null);
            };
            let path = file.into_path().map_err(|err| err.to_string())?;
            let instance = modrinth::install_modpack_from_file(&paths, &store, &path, &|payload| {
                let _ = app.emit("modpack:progress", payload);
            })
            .await?;
            install_loader_after_import(&app, &store, &paths, &instance).await?;
            Ok(instance)
        }

        "java:detect" => Ok(match java_manager::detect_system_java() {
            Some(path) => Value::String(path),
            None => Value::Null,
        }),        "java:browse" => {
            let Some(file) = app.dialog().file().blocking_pick_file() else {
                return Ok(Value::Null);
            };
            let path = file.into_path().map_err(|err| err.to_string())?;
            Ok(Value::String(path.to_string_lossy().to_string()))
        }
        "java:download" => {
            let major = arg(&args, 0).as_u64().unwrap_or(21) as u32;
            let java_path = java_manager::download_and_install_java(&app, &paths, major).await?;
            store.update_settings(&json!({ "javaPath": java_path }));
            Ok(Value::String(java_path))
        }

        "app:status" => Ok(json!({
            "ramMin": store.get_setting("ramMin"),
            "ramMax": store.get_setting("ramMax"),
            "javaPath": store.get_setting_string("javaPath").unwrap_or_default(),
            "gameDir": paths.minecraft_root().to_string_lossy(),
            "version": updater::get_current_version()
        })),
        // Gerçek sistem RAM bilgisi (toplam / kullanılan, bayt)
        "system:memory" => {
            fn memory_snapshot() -> (u64, u64) {
                static SYSTEM: std::sync::OnceLock<
                    std::sync::Mutex<sysinfo::System>,
                > = std::sync::OnceLock::new();
                let mut guard = SYSTEM
                    .get_or_init(|| std::sync::Mutex::new(sysinfo::System::new()))
                    .lock()
                    .unwrap();
                guard.refresh_memory();
                (guard.total_memory(), guard.used_memory())
            }
            let (total, used) = memory_snapshot();
            Ok(json!({ "total": total, "used": used }))
        }
        // Etkin Java sürümü: ayarlanan yol veya kurulu JDK gerçekten
        // çalıştırılarak öğrenilir.
        "java:version" => {
            let mut found = Value::Null;
            for candidate in resolve_java_candidates(&store, &paths) {
                if candidate.is_file() {
                    if let Some(version) = probe_java_version(&candidate) {
                        found = json!({
                            "path": candidate.to_string_lossy(),
                            "version": version
                        });
                        break;
                    }
                }
            }
            Ok(found)
        }
        // Klasörü Windows Gezgininde açar (yalnızca mevcut dizinler).
        "app:openPath" => {
            let path = arg_string(&args, 0).unwrap_or_default();
            let dir = std::path::PathBuf::from(&path);
            if !dir.is_dir() {
                return Err("Geçersiz klasör".to_string());
            }
            open::that(&dir).map_err(|err| format!("Klasör açılamadı: {err}"))?;
            Ok(Value::Null)
        }
        "app:openUrl" => {
            let url = arg_string(&args, 0).unwrap_or_default();
            if !(url.starts_with("https://") || url.starts_with("http://")) {
                return Err("Geçersiz URL".to_string());
            }
            open::that(&url).map_err(|err| format!("Bağlantı açılamadı: {err}"))?;
            Ok(Value::Null)
        }
        "news:list" => updater::list_releases(5).await,

        "settings:get" => Ok(store.settings()),
        "settings:set" => {
            let patch = arg(&args, 0);
            let updated = store.update_settings(&patch);
            let start_on_boot = updated
                .get("startOnBoot")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let autostart = app.autolaunch();
            let result = if start_on_boot {
                autostart.enable()
            } else {
                autostart.disable()
            };
            if let Err(err) = result {
                crate::logger::warn(&format!("Otomatik başlatma ayarlanamadı: {err}"));
            }
            Ok(updated)
        }

        "servers:list" => Ok(server_manager::list_servers(&store)),
        "servers:create" => {
            let data = arg(&args, 0);
            server_manager::create_server(&app, &store, &paths, &data, |payload| {
                let _ = app.emit("servers:createProgress", payload);
            })
            .await
        }
        "servers:update" => {
            let params = arg(&args, 0);
            let server_id = param_string(&params, "serverId").unwrap_or("");
            let patch = params.get("patch").cloned().unwrap_or_else(|| json!({}));
            server_manager::update_server_settings(&store, server_id, &patch)
        }
        "servers:delete" => {
            let server_id = arg_string(&args, 0).unwrap_or_default();
            server_manager::delete_server(&store, &paths, &server_id)
                .await
                .map(|_| Value::Null)
        }
        "servers:openFolder" => {
            let server_id = arg_string(&args, 0).unwrap_or_default();
            server_manager::open_server_folder(&paths, &server_id).map(|_| Value::Null)
        }
        "servers:isRunning" => {
            let server_id = arg_string(&args, 0).unwrap_or_default();
            Ok(json!(server_manager::is_running(&server_id).await))
        }
        "servers:getProperties" => {
            let server_id = arg_string(&args, 0).unwrap_or_default();
            server_manager::get_server_properties(&store, &paths, &server_id)
        }
        "servers:setProperties" => {
            let params = arg(&args, 0);
            let server_id = param_string(&params, "serverId").unwrap_or("");
            let patch = params.get("patch").cloned().unwrap_or_else(|| json!({}));
            server_manager::set_server_properties(&store, &paths, server_id, &patch)
        }
        "servers:start" => {
            let server_id = arg_string(&args, 0).unwrap_or_default();
            server_manager::start_server(&app, &store, &paths, &server_id)
                .await
                .map(|_| json!(true))
        }
        "servers:stop" => {
            let server_id = arg_string(&args, 0).unwrap_or_default();
            Ok(json!(server_manager::stop_server(&server_id).await?))
        }
        "servers:sendCommand" => {
            let params = arg(&args, 0);
            let server_id = param_string(&params, "serverId").unwrap_or("");
            let command = param_string(&params, "command").unwrap_or("");
            server_manager::send_command(server_id, command)
                .await
                .map(|_| Value::Null)
        }

        "updater:check" => updater::check_for_updates(&app).await,
        "updater:download" => updater::download_update(&app).await,
        "updater:install" => updater::quit_and_install(&app).map(|_| Value::Null),
        "updater:getVersion" => Ok(Value::String(updater::get_current_version())),

        "worlds:list" => {
            let instance_id = arg_string(&args, 0).unwrap_or_default();
            Ok(world_manager::list_worlds(&paths, &instance_id))
        }
        "worlds:delete" => {
            let params = arg(&args, 0);
            let instance_id = param_string(&params, "instanceId").unwrap_or("");
            let folder_name = param_string(&params, "folderName").unwrap_or("");
            world_manager::delete_world(&paths, instance_id, folder_name).map(|_| Value::Null)
        }
        "worlds:openFolder" => {
            let params = arg(&args, 0);
            let instance_id = param_string(&params, "instanceId").unwrap_or("");
            let folder_name = param_string(&params, "folderName").unwrap_or("");
            world_manager::open_world_folder(&paths, instance_id, folder_name)
                .map(|_| Value::Null)
        }
        "worlds:backup" => {
            let params = arg(&args, 0);
            let instance_id = param_string(&params, "instanceId").unwrap_or("");
            let folder_name = param_string(&params, "folderName").unwrap_or("");
            let Some(file) = app
                .dialog()
                .file()
                .set_file_name(format!("{folder_name}.zip"))
                .add_filter("Zip Arşivi", &["zip"])
                .blocking_save_file()
            else {
                return Ok(Value::Null);
            };
            let path = file.into_path().map_err(|err| err.to_string())?;
            world_manager::backup_world(&paths, instance_id, folder_name, &path)
                .map(Value::String)
        }
        "worlds:import" => {
            let instance_id = arg_string(&args, 0).unwrap_or_default();
            let Some(file) = app
                .dialog()
                .file()
                .add_filter("Zip Arşivi", &["zip"])
                .blocking_pick_file()
            else {
                return Ok(Value::Null);
            };
            let path = file.into_path().map_err(|err| err.to_string())?;
            world_manager::import_world(&paths, &instance_id, &path).map(Value::String)
        }

        "background:get" => Ok(background::get_background_state(&store, &paths)),
        "background:choose" => {
            let Some(file) = app
                .dialog()
                .file()
                .add_filter("Görseller", &["png", "jpg", "jpeg", "webp", "gif"])
                .blocking_pick_file()
            else {
                return Ok(Value::Null);
            };
            let path = file.into_path().map_err(|err| err.to_string())?;
            background::set_custom_background(&store, &paths, &path)
        }
        "background:reset" => Ok(background::reset_background(&store, &paths)),

        "window:minimize" => {
            if let Some(window) = app.get_webview_window("main") {
                window.minimize().map_err(|err| err.to_string())?;
            }
            Ok(Value::Null)
        }
        "window:maximize" => {
            if let Some(window) = app.get_webview_window("main") {
                if window.is_maximized().unwrap_or(false) {
                    window.unmaximize().map_err(|err| err.to_string())?;
                } else {
                    window.maximize().map_err(|err| err.to_string())?;
                }
            }
            Ok(Value::Null)
        }
        "window:close" => {
            if let Some(window) = app.get_webview_window("main") {
                if store.get_setting_bool("minimizeToTray", true) {
                    let _ = window.hide();
                } else {
                    let _ = window.close();
                }
            }
            Ok(Value::Null)
        }

        other => Err(format!("Bilinmeyen kanal: {other}")),
    }
}

