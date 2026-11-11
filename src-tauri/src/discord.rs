use crate::store::Store;
use discord_rich_presence::activity::{Activity, Assets, Timestamps};
use discord_rich_presence::{DiscordIpc, DiscordIpcClient};
use serde_json::json;
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Emitter, Manager};

static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();
static CLIENT: OnceLock<Mutex<Option<DiscordIpcClient>>> = OnceLock::new();

fn client_slot() -> &'static Mutex<Option<DiscordIpcClient>> {
    CLIENT.get_or_init(|| Mutex::new(None))
}

fn notify(level: &str, message: &str) {
    match level {
        "error" => crate::logger::error(&format!("[Discord] {message}")),
        "warn" => crate::logger::warn(&format!("[Discord] {message}")),
        "success" | "info" => crate::logger::info(&format!("[Discord] {message}")),
        _ => crate::logger::info(&format!("[Discord] {message}")),
    }
    if let Some(app) = APP_HANDLE.get() {
        let _ = app.emit(
            "discord:log",
            json!({ "level": level, "source": "Discord", "message": message }),
        );
    }
}

fn discord_client_id() -> Option<String> {
    std::env::var("DISCORD_CLIENT_ID")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty() && value != "BURAYA_DISCORD_CLIENT_ID_YAZIN")
}

fn is_enabled(app: &AppHandle) -> bool {
    let store = app.state::<Store>();
    store.get_setting_bool("discordPresenceEnabled", true) && discord_client_id().is_some()
}

pub fn init(app: AppHandle) {
    let _ = APP_HANDLE.set(app);
    let Some(app) = APP_HANDLE.get() else {
        return;
    };
    if !is_enabled(app) {
        let store = app.state::<Store>();
        if !store.get_setting_bool("discordPresenceEnabled", true) {
            notify("info", "Devre dışı (Ayarlar'dan kapalı).");
        } else {
            notify(
                "info",
                "DISCORD_CLIENT_ID tanımlı değil, bağlantı denenmiyor (.env dosyasını kontrol edin).",
            );
        }
        return;
    }
    connect();
}

fn connect() {
    let Some(client_id) = discord_client_id() else {
        return;
    };
    let mut slot = client_slot().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if slot.is_some() {
        return;
    }
    notify("info", "DISCORD_CLIENT_ID bulundu, Discord'a bağlanılıyor...");
    match DiscordIpcClient::new(&client_id) {
        Ok(mut client) => match client.connect() {
            Ok(()) => {
                *slot = Some(client);
                notify("success", "Bağlandı ve hazır.");
            }
            Err(err) => {
                notify(
                    "warn",
                    &format!("Bağlanılamadı — Discord masaüstü uygulaması açık mı? ({err})"),
                );
            }
        },
        Err(err) => {
            notify("error", &format!("Başlatılamadı: {err}"));
        }
    }
}

fn loader_label(loader: &str) -> Option<&'static str> {
    match loader {
        "fabric" => Some("Fabric"),
        "quilt" => Some("Quilt"),
        "forge" => Some("Forge"),
        "neoforge" => Some("NeoForge"),
        _ => None,
    }
}

pub fn set_playing(instance_name: &str, mc_version: &str, loader: &str) {
    let Some(app) = APP_HANDLE.get() else {
        return;
    };
    if !is_enabled(app) {
        return;
    }
    if client_slot()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .is_none()
    {
        connect();
    }
    let loader_text = loader_label(loader);
    let details = format!("{instance_name} oynuyor");
    let mut activity = Activity::new()
        .details(&details)
        .assets(Assets::new().large_image("buran_logo").large_text("Buran Launcher"))
        .timestamps(Timestamps::new().start(chrono::Utc::now().timestamp()));
    let state = if loader_text.is_some() {
        format!("Minecraft {mc_version} · {}", loader_text.unwrap_or_default())
    } else {
        format!("Minecraft {mc_version}")
    };
    activity = activity.state(&state);
    if let Some(loader_text) = loader_text {
        activity = activity.assets(
            Assets::new()
                .large_image("buran_logo")
                .large_text("Buran Launcher")
                .small_image(loader)
                .small_text(loader_text),
        );
    }

    let mut slot = client_slot().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(client) = slot.as_mut() {
        match client.set_activity(activity) {
            Ok(()) => notify("info", &format!("Durum güncellendi: \"{instance_name} oynuyor\"")),
            Err(err) => {
                notify("error", &format!("Durum güncellenemedi: {err}"));
                let _ = client.close();
                *slot = None;
            }
        }
    } else {
        notify("warn", "Durum gönderilemedi: Discord'a bağlı değil.");
    }
}

pub fn clear_activity() {
    let mut slot = client_slot().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(client) = slot.as_mut() else {
        return;
    };
    match client.clear_activity() {
        Ok(()) => notify("info", "Durum temizlendi."),
        Err(err) => notify("warn", &format!("Durum temizlenemedi: {err}")),
    }
}

pub fn destroy() {
    let mut slot = client_slot().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(client) = slot.as_mut() {
        let _ = client.close();
    }
    *slot = None;
}