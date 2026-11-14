use crate::http;
use crate::paths::AppPaths;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use tauri::{AppHandle, Emitter};

const FABRIC_META: &str = "https://meta.fabricmc.net/v2";
const QUILT_META: &str = "https://meta.quiltmc.org/v3";
const PAPER_API: &str = "https://fill.papermc.io/v3";

fn emit_loader_progress(app: &AppHandle, payload: Value) {
    let _ = app.emit("loader:progress", payload);
}

fn emit_server_progress(app: &AppHandle, payload: Value) {
    let _ = app.emit("servers:createProgress", payload);
}

pub fn get_versions_root_dir(paths: &AppPaths) -> std::path::PathBuf {
    let dir = paths.versions_dir();
    let _ = fs::create_dir_all(&dir);
    dir
}

pub async fn list_fabric_loader_versions() -> Result<Value, String> {
    http::get_json(&format!("{FABRIC_META}/versions/loader"))
        .await
        .map_err(|_| "Fabric yükleyici sürümleri alınamadı".to_string())
}

pub async fn list_quilt_loader_versions() -> Result<Value, String> {
    http::get_json(&format!("{QUILT_META}/versions/loader"))
        .await
        .map_err(|_| "Quilt yükleyici sürümleri alınamadı".to_string())
}

/// "47.2.20" gibi sürüm dizisini sayısal parçalara böler (doğru sıralama için).
fn version_segments(version: &str) -> Vec<u64> {
    version
        .split(|character: char| character == '.' || character == '-')
        .map(|part| part.trim().parse::<u64>().unwrap_or(0))
        .collect()
}

/// Forge sürümlerini promotions listesinden çeker: [önerilen, son sürüm, ...].
pub async fn list_forge_versions(mc_version: &str) -> Result<Value, String> {
    let promotions = http::get_json(
        "https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json",
    )
    .await
    .map_err(|_| "Forge sürümleri alınamadı".to_string())?;
    let promos = promotions
        .get("promos")
        .and_then(Value::as_object)
        .ok_or_else(|| "Forge promos listesi boş döndü".to_string())?;

    let exact_prefix = format!("{mc_version}-");
    // Çok eski sürümler için "1.12-latest" biçiminde iki parçalı anahtar da vardır
    let two_parts: Vec<&str> = mc_version.split('.').take(2).collect();
    let short_prefix = if two_parts.len() == 2 {
        format!("{}-{}", two_parts[0], two_parts[1])
    } else {
        String::new()
    };

    let mut versions: Vec<(u8, String)> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (key, value) in promos {
        let Some(version) = value.as_str() else { continue };
        let Some(tag) = key
            .strip_prefix(&exact_prefix)
            .or_else(|| {
                if short_prefix.is_empty() {
                    None
                } else {
                    key.strip_prefix(&short_prefix)
                }
            })
        else {
            continue;
        };
        let tag = tag.to_lowercase();
        if tag != "latest" && tag != "recommended" {
            continue;
        }
        if seen.insert(version.to_string()) {
            let priority = if tag == "recommended" { 2 } else { 1 };
            versions.push((priority, version.to_string()));
        }
    }
    // Önerilen en önde, sonra son sürüm, kalanlar yeniden eskiye
    versions.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| version_segments(&b.1).cmp(&version_segments(&a.1)))
    });
    Ok(json!(versions
        .into_iter()
        .map(|(priority, version)| json!({ "version": version, "tag": if priority == 2 { "recommended" } else { "latest" } }))
        .collect::<Vec<Value>>()))
}

/// NeoForge sürümlerini maven metadata'sından çeker ve verilen MC sürümüyle
/// uyumlu olanları döndürür (ör. 1.21.1 → 21.1.x), yeniden eskiye sıralı.
pub async fn list_neoforge_versions(mc_version: &str) -> Result<Value, String> {
    let metadata = http::get_text(
        "https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml",
    )
    .await?;
    let mut versions: Vec<String> = Vec::new();
    for line in metadata.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix("<version>") else { continue };
        let Some(end) = rest.find("</version>") else { continue };
        versions.push(rest[..end].to_string());
    }
    let mut mc_parts = mc_version.split('.');
    let _major = mc_parts.next(); // her zaman "1"
    let minor = mc_parts.next().unwrap_or("");
    let patch = mc_parts.next().unwrap_or("");
    let prefix = if patch.is_empty() {
        format!("{minor}.")
    } else {
        format!("{minor}.{patch}.")
    };
    let mut filtered: Vec<String> = versions
        .into_iter()
        .filter(|version| version.starts_with(&prefix))
        .collect();
    filtered.sort_by(|a, b| version_segments(b).cmp(&version_segments(a)));
    Ok(json!(filtered
        .into_iter()
        .map(|version| json!({ "version": version }))
        .collect::<Vec<Value>>()))
}

/// Kullanılabilir Paper (MC) sürümlerini döndürür, yeniden eskiye sıralı.
/// (PaperMC'nin eski v2 API'si kapandı; Fill v3 kullanılır.)
pub async fn list_paper_versions() -> Result<Value, String> {
    let response = http::get_json(&format!("{PAPER_API}/projects/paper")).await?;
    let families = response
        .get("versions")
        .and_then(Value::as_object)
        .ok_or_else(|| "Paper sürüm listesi beklenen biçimde değil".to_string())?;
    let mut versions: Vec<String> = families
        .values()
        .filter_map(Value::as_array)
        .flatten()
        .filter_map(Value::as_str)
        .filter(|version| !version.contains('-')) // pre/rc sürümlerini çıkar
        .map(ToString::to_string)
        .collect();
    versions.sort_by(|a, b| version_segments(b).cmp(&version_segments(a)));
    versions.dedup();
    Ok(json!(versions
        .into_iter()
        .map(|version| json!({ "version": version }))
        .collect::<Vec<Value>>()))
}

async fn install_meta_profile(
    paths: &AppPaths,
    app: &AppHandle,
    meta_base: &str,
    label: &str,
    mc_version: &str,
    loader_version: &str,
) -> Result<String, String> {
    emit_loader_progress(app, json!({ "stage": "fetchingProfile" }));
    let url = format!("{meta_base}/versions/loader/{mc_version}/{loader_version}/profile/json");
    let profile = http::get_json(&url)
        .await
        .map_err(|_| format!("{label} profili alınamadı (Minecraft/yükleyici sürüm uyumunu kontrol edin)"))?;
    let profile_id = profile
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{label} profil kimliği bulunamadı"))?
        .to_string();

    let versions_dir = get_versions_root_dir(paths);
    let profile_dir = versions_dir.join(&profile_id);
    fs::create_dir_all(&profile_dir).map_err(|err| format!("Klasör oluşturulamadı: {err}"))?;
    let json_path = profile_dir.join(format!("{profile_id}.json"));
    fs::write(
        json_path,
        serde_json::to_string_pretty(&profile).unwrap_or_default(),
    )
    .map_err(|err| format!("Profil dosyası yazılamadı: {err}"))?;

    emit_loader_progress(app, json!({ "stage": "done" }));
    Ok(profile_id)
}

pub async fn install_fabric(
    paths: &AppPaths,
    app: &AppHandle,
    mc_version: &str,
    loader_version: &str,
) -> Result<String, String> {
    install_meta_profile(
        paths,
        app,
        FABRIC_META,
        "Fabric",
        mc_version,
        loader_version,
    )
    .await
}

pub async fn install_quilt(
    paths: &AppPaths,
    app: &AppHandle,
    mc_version: &str,
    loader_version: &str,
) -> Result<String, String> {
    install_meta_profile(
        paths,
        app,
        QUILT_META,
        "Quilt",
        mc_version,
        loader_version,
    )
    .await
}

fn hide_console(command: &mut tokio::process::Command) {
    #[cfg(windows)]
    {
        command.creation_flags(0x0800_0000);
    }
}

async fn run_installer(
    java_bin: &str,
    installer: &Path,
    args: &[&str],
    working_dir: &Path,
    label: &str,
) -> Result<(), String> {
    let mut command = tokio::process::Command::new(java_bin);
    command.arg("-jar").arg(installer);
    for arg in args {
        command.arg(arg);
    }
    command.current_dir(working_dir);
    hide_console(&mut command);

    let output = command
        .output()
        .await
        .map_err(|err| format!("{label} yükleyicisi çalıştırılamadı: {err}"))?;
    if output.status.success() {
        Ok(())
    } else {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let details = if stderr.trim().is_empty() {
            stdout.into_owned()
        } else {
            format!("{stderr}\n{stdout}")
        };
        let tail: String = details
            .chars()
            .rev()
            .take(1200)
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        Err(format!(
            "{label} kurulumu başarısız oldu (kod {:?}): {tail}",
            output.status.code()
        ))
    }
}

fn ensure_launcher_profiles(root: &Path) -> Result<(), String> {
    let profile_path = root.join("launcher_profiles.json");
    if profile_path.exists() {
        return Ok(());
    }
    let contents = json!({
        "profiles": {},
        "selectedProfile": "",
        "clientToken": "",
        "authenticationDatabase": {},
        "launcherVersion": { "name": "Buran Launcher", "format": 21 }
    });
    fs::write(
        profile_path,
        serde_json::to_string_pretty(&contents).unwrap_or_default(),
    )
    .map_err(|err| format!("launcher_profiles.json oluşturulamadı: {err}"))
}

fn is_legacy_neoforge(mc_version: &str) -> bool {
    mc_version == "1.20.1"
}

fn neoforge_installer_url(mc_version: &str, loader_version: &str) -> String {
    if is_legacy_neoforge(mc_version) {
        format!(
            "https://maven.neoforged.net/releases/net/neoforged/forge/{mc_version}-{loader_version}/forge-{mc_version}-{loader_version}-installer.jar"
        )
    } else {
        format!(
            "https://maven.neoforged.net/releases/net/neoforged/neoforge/{loader_version}/neoforge-{loader_version}-installer.jar"
        )
    }
}

fn neoforge_profile_id(mc_version: &str, loader_version: &str) -> String {
    if is_legacy_neoforge(mc_version) {
        format!("{mc_version}-forge-{loader_version}")
    } else {
        format!("neoforge-{loader_version}")
    }
}

pub async fn install_forge_like(
    paths: &AppPaths,
    app: &AppHandle,
    loader_type: &str,
    mc_version: &str,
    loader_version: &str,
    java_bin: &str,
) -> Result<String, String> {
    emit_loader_progress(app, json!({ "stage": "resolvingInstaller" }));

    let installer_url = if loader_type == "neoforge" {
        neoforge_installer_url(mc_version, loader_version)
    } else {
        format!(
            "https://maven.minecraftforge.net/net/minecraftforge/forge/{mc_version}-{loader_version}/forge-{mc_version}-{loader_version}-installer.jar"
        )
    };

    let versions_dir = get_versions_root_dir(paths);
    let installer_path = versions_dir.join(format!("{loader_type}-installer-{loader_version}.jar"));

    emit_loader_progress(app, json!({ "stage": "downloading" }));
    let bytes = http::download_bytes(&installer_url).await.map_err(|_| {
        format!(
            "{} yükleyicisi indirilemedi (sürüm numarasını kontrol edin)",
            if loader_type == "neoforge" { "NeoForge" } else { "Forge" }
        )
    })?;
    fs::write(&installer_path, bytes).map_err(|err| format!("Yükleyici yazılamadı: {err}"))?;

    emit_loader_progress(app, json!({ "stage": "installing" }));
    let root = paths.minecraft_root();
    fs::create_dir_all(&root).map_err(|err| format!("Klasör oluşturulamadı: {err}"))?;
    ensure_launcher_profiles(&root)?;
    let result = run_installer(
        java_bin,
        &installer_path,
        &["--installClient", &root.to_string_lossy()],
        &versions_dir,
        if loader_type == "neoforge" { "NeoForge" } else { "Forge" },
    )
    .await;
    let _ = fs::remove_file(&installer_path);
    result?;

    emit_loader_progress(app, json!({ "stage": "done" }));
    if loader_type == "neoforge" {
        Ok(neoforge_profile_id(mc_version, loader_version))
    } else {
        Ok(format!("{mc_version}-forge-{loader_version}"))
    }
}

async fn latest_installer_version(meta_base: &str, label: &str) -> Result<String, String> {
    let list = http::get_json(&format!("{meta_base}/versions/installer"))
        .await
        .map_err(|_| format!("{label} installer sürümleri alınamadı"))?;
    list.as_array()
        .and_then(|items| items.first())
        .and_then(|item| item.get("version"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .ok_or_else(|| format!("{label} installer sürümü bulunamadı"))
}

async fn install_meta_server(
    app: &AppHandle,
    meta_base: &str,
    label: &str,
    server_dir: &Path,
    mc_version: &str,
    loader_version: &str,
) -> Result<Value, String> {
    emit_server_progress(app, json!({ "stage": "resolvingInstaller" }));
    let installer_version = latest_installer_version(meta_base, label).await?;
    emit_server_progress(app, json!({ "stage": "downloading" }));

    let url = format!(
        "{meta_base}/versions/loader/{mc_version}/{loader_version}/{installer_version}/server/jar"
    );
    let bytes = http::download_bytes(&url).await.map_err(|_| {
        format!("{label} sunucu dosyası indirilemedi (sürüm uyumunu kontrol edin)")
    })?;
    fs::write(server_dir.join("server.jar"), bytes)
        .map_err(|err| format!("Sunucu dosyası yazılamadı: {err}"))?;
    emit_server_progress(app, json!({ "stage": "done" }));
    Ok(json!({ "type": "jar", "jarFile": "server.jar" }))
}

pub async fn install_fabric_server(
    app: &AppHandle,
    server_dir: &Path,
    mc_version: &str,
    loader_version: &str,
) -> Result<Value, String> {
    install_meta_server(
        app,
        FABRIC_META,
        "Fabric",
        server_dir,
        mc_version,
        loader_version,
    )
    .await
}

pub async fn install_quilt_server(
    app: &AppHandle,
    server_dir: &Path,
    mc_version: &str,
    loader_version: &str,
) -> Result<Value, String> {
    install_meta_server(
        app,
        QUILT_META,
        "Quilt",
        server_dir,
        mc_version,
        loader_version,
    )
    .await
}

pub async fn install_forge_like_server(
    app: &AppHandle,
    server_dir: &Path,
    loader_type: &str,
    mc_version: &str,
    loader_version: &str,
    java_bin: &str,
) -> Result<Value, String> {
    emit_server_progress(app, json!({ "stage": "resolvingInstaller" }));
    let installer_url = if loader_type == "neoforge" {
        neoforge_installer_url(mc_version, loader_version)
    } else {
        format!(
            "https://maven.minecraftforge.net/net/minecraftforge/forge/{mc_version}-{loader_version}/forge-{mc_version}-{loader_version}-installer.jar"
        )
    };

    let installer_path = server_dir.join(format!("{loader_type}-installer.jar"));
    emit_server_progress(app, json!({ "stage": "downloading" }));
    let bytes = http::download_bytes(&installer_url).await.map_err(|_| {
        format!(
            "{} yükleyicisi indirilemedi (sürüm numarasını kontrol edin)",
            if loader_type == "neoforge" { "NeoForge" } else { "Forge" }
        )
    })?;
    fs::write(&installer_path, bytes).map_err(|err| format!("Yükleyici yazılamadı: {err}"))?;

    emit_server_progress(app, json!({ "stage": "installing" }));
    let result = run_installer(
        java_bin,
        &installer_path,
        &["--installServer"],
        server_dir,
        if loader_type == "neoforge" { "NeoForge" } else { "Forge" },
    )
    .await;
    let _ = fs::remove_file(&installer_path);
    result?;

    let run_script = if cfg!(windows) { "run.bat" } else { "run.sh" };
    if server_dir.join(run_script).exists() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(server_dir.join(run_script), fs::Permissions::from_mode(0o755));
        }
        emit_server_progress(app, json!({ "stage": "done" }));
        return Ok(json!({ "type": "script", "scriptFile": run_script }));
    }

    let universal = fs::read_dir(server_dir)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .find(|name| {
            let lower = name.to_lowercase();
            lower.ends_with("universal.jar")
                || (lower.starts_with("forge-") && lower.ends_with(".jar") && !lower.contains("installer"))
        });

    if let Some(jar) = universal {
        emit_server_progress(app, json!({ "stage": "done" }));
        return Ok(json!({ "type": "jar", "jarFile": jar }));
    }

    Err(format!(
        "{loader_type} sunucu kurulumu tamamlandı ama çalıştırılabilir dosya bulunamadı (run.sh/run.bat veya universal.jar). Sunucu klasörünü elle kontrol edin: {}",
        server_dir.display()
    ))
}

/// Paper sunucusunu indirip kurar: tek dosya (server.jar) yeterlidir.
/// `build` verilmezse son derleme kullanılır (Fill v3: en yenisi ilk eleman).
pub async fn install_paper_server(
    app: &AppHandle,
    server_dir: &Path,
    mc_version: &str,
    build: Option<&str>,
) -> Result<Value, String> {
    emit_server_progress(app, json!({ "stage": "downloadingServer" }));
    let builds = http::get_json(&format!(
        "{PAPER_API}/projects/paper/versions/{mc_version}/builds"
    ))
    .await?;
    let build_list = builds
        .as_array()
        .ok_or_else(|| format!("Paper {mc_version} için derleme bulunamadı"))?;
    if build_list.is_empty() {
        return Err(format!("Paper {mc_version} için derleme bulunamadı"));
    }
    let build_entry = if let Some(requested) = build {
        build_list
            .iter()
            .find(|entry| {
                entry
                    .get("id")
                    .map(|value| value.to_string())
                    .as_deref()
                    == Some(requested)
            })
            .ok_or_else(|| format!("Paper derlemesi bulunamadı: {requested}"))?
    } else {
        &build_list[0]
    };
    let download = build_entry
        .get("downloads")
        .and_then(|downloads| {
            downloads
                .get("server:default")
                .or_else(|| downloads.as_object().and_then(|map| map.values().next()))
        })
        .ok_or_else(|| "Paper indirme bilgisi bulunamadı".to_string())?;
    let url = download
        .get("url")
        .and_then(Value::as_str)
        .ok_or_else(|| "Paper indirme bağlantısı bulunamadı".to_string())?;
    let bytes = http::download_bytes(url)
        .await
        .map_err(|err| format!("Paper sunucu dosyası indirilemedi: {err}"))?;
    fs::write(server_dir.join("server.jar"), bytes)
        .map_err(|err| format!("Sunucu dosyası yazılamadı: {err}"))?;
    emit_server_progress(app, json!({ "stage": "done" }));
    Ok(json!({ "type": "jar", "jarFile": "server.jar" }))
}