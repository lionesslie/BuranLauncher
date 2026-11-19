use crate::store::Store;
use discord_rich_presence::activity::{Activity, Assets, Timestamps};
use discord_rich_presence::{DiscordIpc, DiscordIpcClient};
use serde_json::json;
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Emitter, Manager};

static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();
static CLIENT: OnceLock<Mutex<Option<DiscordIpcClient>>> = OnceLock::new();
static PRESENCE: OnceLock<Mutex<Option<PresenceBase>>> = OnceLock::new();

fn client_slot() -> &'static Mutex<Option<DiscordIpcClient>> {
    CLIENT.get_or_init(|| Mutex::new(None))
}

fn presence_slot() -> &'static Mutex<Option<PresenceBase>> {
    PRESENCE.get_or_init(|| Mutex::new(None))
}

#[derive(Clone)]
struct PresenceBase {
    instance_name: String,
    mc_version: String,
    loader: String,
    connection: Connection,
}

#[derive(Clone, PartialEq)]
enum Connection {
    Menu,
    Singleplayer,
    Server(String),
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

const DEFAULT_CLIENT_ID: &str = "1563197891261235302";

fn discord_client_id() -> Option<String> {
    let client_id = std::env::var("DISCORD_CLIENT_ID")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty() && value != "BURAYA_DISCORD_CLIENT_ID_YAZIN")
        .unwrap_or_else(|| DEFAULT_CLIENT_ID.to_string());
    Some(client_id)
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

fn connection_label(conn: &Connection, lang: &str) -> String {
    let (menu, single, server_prefix) = match lang {
        "ru" => ("В меню", "Одиночная игра", "Сервер: "),
        _ => ("Menüde", "Tek oyunculu", "Sunucu: "),
    };
    match conn {
        Connection::Menu => menu.to_string(),
        Connection::Singleplayer => single.to_string(),
        Connection::Server(host) => format!("{server_prefix}{host}"),
    }
}

fn push_activity(app: &AppHandle) {
    let base = {
        let guard = presence_slot().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.clone()
    };
    let Some(base) = base else {
        return;
    };
    if client_slot()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .is_none()
    {
        connect();
    }
    let store = app.state::<Store>();
    let lang = crate::i18n::current_lang(&store);
    let loader_text = loader_label(&base.loader);
    let details = match loader_text {
        Some(loader) => format!("{loader} • Minecraft {}", base.mc_version),
        None => format!("Minecraft {}", base.mc_version),
    };
    let large_text = format!("{} — Buran Launcher", base.instance_name);
    let state = connection_label(&base.connection, &lang);
    let mut assets = Assets::new()
        .large_image("buran_logo")
        .large_text(&large_text);
    if let Some(loader) = loader_text {
        assets = assets.small_image(&base.loader).small_text(loader);
    }
    let activity = Activity::new()
        .details(&details)
        .state(&state)
        .assets(assets)
        .timestamps(Timestamps::new().start(chrono::Utc::now().timestamp()));
    let summary = format!("{} ({}, {})", base.instance_name, base.mc_version, state);
    let mut slot = client_slot().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(client) = slot.as_mut() {
        match client.set_activity(activity) {
            Ok(()) => notify("info", &format!("Durum güncellendi: {summary}")),
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

pub fn set_playing(instance_name: &str, mc_version: &str, loader: &str) {
    let Some(app) = APP_HANDLE.get() else {
        return;
    };
    if !is_enabled(app) {
        return;
    }
    {
        let mut guard = presence_slot().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        *guard = Some(PresenceBase {
            instance_name: instance_name.to_string(),
            mc_version: mc_version.to_string(),
            loader: loader.to_string(),
            connection: Connection::Menu,
        });
    }
    push_activity(app);
}

/// Oyun çıktısındaki bağlantı satırlarını izleyip Discord durumunu günceller.
/// Sunucuya bağlanınca sunucu adı, tek oyunculu dünyaya girince "Tek oyunculu",
/// menüye dönünce "Menüde" gösterilir.
pub fn handle_game_log(line: &str) {
    let Some(marker) = line.find("]: ") else {
        return;
    };
    let message = &line[marker + 3..];
    let next = if let Some(rest) = message.strip_prefix("Connecting to ") {
        let host = rest.split(',').next().unwrap_or(rest).trim();
        if host.is_empty() {
            return;
        }
        Connection::Server(host.chars().take(64).collect())
    } else if message.contains("Starting integrated minecraft server version") {
        Connection::Singleplayer
    } else if message.contains("Disconnected from server")
        || message.contains("Connection lost")
        || message.contains("Stopping integrated server")
    {
        Connection::Menu
    } else {
        return;
    };

    let changed = {
        let mut guard = presence_slot().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        match guard.as_mut() {
            Some(base) if base.connection != next => {
                base.connection = next;
                true
            }
            _ => false,
        }
    };
    if changed {
        if let Some(app) = APP_HANDLE.get() {
            push_activity(app);
        }
    }
}

pub fn clear_activity() {
    let had_state = {
        let mut guard = presence_slot().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.take().is_some()
    };
    if !had_state {
        return;
    }
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
