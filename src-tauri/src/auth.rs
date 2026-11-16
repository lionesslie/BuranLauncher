use crate::http::{self, now_millis};
use crate::secrets::SecretBox;
use crate::store::Store;
use base64::Engine;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use uuid::Uuid;

const DEFAULT_CLIENT_ID: &str = "00000000402b5328";
const LEGACY_TOKEN_URL: &str = "https://login.live.com/oauth20_token.srf";
const LEGACY_SCOPE: &str = "service::user.auth.xboxlive.com::MBI_SSL";
const AZURE_TOKEN_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const AZURE_AUTHORIZE_URL: &str =
    "https://login.microsoftonline.com/consumers/oauth2/v2.0/authorize";
const AZURE_SCOPE: &str = "XboxLive.signin offline_access";
// Gömülü giriş penceresi için genel istemci (public client) yönlendirme
// adresi; giriş sonrası tarayıcı buraya döner, kodu navigasyondan yakalarız.
const NATIVE_REDIRECT: &str = "https://login.microsoftonline.com/common/oauth2/nativeclient";

#[derive(Clone)]
pub struct MclcAuth {
    pub access_token: String,
    pub client_token: String,
    pub uuid: String,
    pub username: String,
    pub user_type: String,
    pub user_properties: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AuthFlavor {
    Legacy,
    Azure,
}

impl AuthFlavor {
    fn as_str(self) -> &'static str {
        match self {
            Self::Legacy => "legacy",
            Self::Azure => "azure",
        }
    }

    fn from_str(value: &str) -> Option<Self> {
        match value {
            "legacy" => Some(Self::Legacy),
            "azure" => Some(Self::Azure),
            _ => None,
        }
    }

    fn token_url(self) -> &'static str {
        match self {
            Self::Legacy => LEGACY_TOKEN_URL,
            Self::Azure => AZURE_TOKEN_URL,
        }
    }

    fn scope(self) -> &'static str {
        match self {
            Self::Legacy => LEGACY_SCOPE,
            Self::Azure => AZURE_SCOPE,
        }
    }

    fn rps_ticket(self, access_token: &str) -> String {
        let prefix = match self {
            Self::Legacy => "t",
            Self::Azure => "d",
        };
        format!("{prefix}={access_token}")
    }
}

#[derive(Clone)]
struct AuthConfig {
    client_id: String,
    flavor: AuthFlavor,
}

impl AuthConfig {
    fn from_env() -> Self {
        let client_id = std::env::var("MS_CLIENT_ID")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty() && value != "BURAYA_AZURE_CLIENT_ID_YAZIN")
            .unwrap_or_else(|| DEFAULT_CLIENT_ID.to_string());
        let flavor = if is_legacy_client_id(&client_id) {
            AuthFlavor::Legacy
        } else {
            AuthFlavor::Azure
        };
        Self { client_id, flavor }
    }

    fn from_token_data(token_data: &Value) -> Self {
        let fallback = Self::from_env();
        let client_id = token_data
            .get("clientId")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(|value| value.trim().to_string())
            .unwrap_or(fallback.client_id);
        let flavor = token_data
            .get("authFlavor")
            .and_then(Value::as_str)
            .and_then(AuthFlavor::from_str)
            .unwrap_or(fallback.flavor);
        Self { client_id, flavor }
    }
}

fn is_legacy_client_id(client_id: &str) -> bool {
    client_id == DEFAULT_CLIENT_ID || client_id.starts_with("00000000")
}

fn notify(app: &AppHandle, level: &str, message: &str) {
    match level {
        "error" => crate::logger::error(message),
        "warn" => crate::logger::warn(message),
        _ => crate::logger::info(message),
    }
    let _ = app.emit(
        "discord:log",
        json!({ "level": level, "source": "Hesap", "message": message }),
    );
}

pub fn generate_offline_uuid(username: &str) -> String {
    let digest = md5::compute(format!("OfflinePlayer:{username}").as_bytes());
    let mut bytes = digest.0;
    bytes[6] = (bytes[6] & 0x0f) | 0x30;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex = http::hex_encode(&bytes);
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

fn safe_account(account: &Value) -> Value {
    let mut safe = account.clone();
    if let Some(map) = safe.as_object_mut() {
        map.remove("encryptedToken");
    }
    safe
}

pub fn list_accounts(store: &Store) -> Value {
    Value::Array(store.array("accounts").iter().map(safe_account).collect())
}

pub fn get_active_account(store: &Store) -> Value {
    let Some(id) = store.get_string("activeAccountId") else {
        return Value::Null;
    };
    store
        .array("accounts")
        .into_iter()
        .find(|account| account.get("id").and_then(Value::as_str) == Some(id.as_str()))
        .map(|account| safe_account(&account))
        .unwrap_or(Value::Null)
}

pub fn add_offline_account(store: &Store, username: &str) -> Result<Value, String> {
    let trimmed = username.trim();
    if trimmed.chars().count() < 3 {
        return Err("Kullanıcı adı en az 3 karakter olmalı".to_string());
    }
    let account = json!({
        "id": Uuid::new_v4().to_string(),
        "type": "offline",
        "username": trimmed,
        "uuid": generate_offline_uuid(trimmed),
        "createdAt": now_millis()
    });
    store.push_array("accounts", account.clone());
    if store.get_string("activeAccountId").is_none() {
        if let Some(id) = account.get("id").and_then(Value::as_str) {
            store.set_active_account(Some(id.to_string()));
        }
    }
    Ok(account)
}

pub fn set_active_account(store: &Store, id: &str) -> Result<String, String> {
    let exists = store
        .array("accounts")
        .iter()
        .any(|account| account.get("id").and_then(Value::as_str) == Some(id));
    if !exists {
        return Err("Hesap bulunamadı".to_string());
    }
    store.set_active_account(Some(id.to_string()));
    Ok(id.to_string())
}

pub fn remove_account(store: &Store, id: &str) {
    store.map_array("accounts", |accounts| {
        accounts.retain(|account| account.get("id").and_then(Value::as_str) != Some(id));
    });
    if store.get_string("activeAccountId").as_deref() == Some(id) {
        let next = store
            .array("accounts")
            .first()
            .and_then(|account| account.get("id"))
            .and_then(Value::as_str)
            .map(ToString::to_string);
        store.set_active_account(next);
    }
}

struct McAuthResult {
    access_token: String,
    expires_at: u64,
    username: String,
    uuid: String,
}

/// PKCE (S256) doğrulama çifti üretir: (code_verifier, code_challenge)
fn pkce_pair() -> (String, String) {
    use rand::Rng;
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill(&mut bytes);
    let verifier = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    let digest = Sha256::digest(verifier.as_bytes());
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest);
    (verifier, challenge)
}

/// Gömülü giriş penceresinden yakalanan yetkilendirme kodunu token çiftine
/// çevirir: (oauth_access_token, refresh_token)
async fn exchange_authorization_code(
    auth: &AuthConfig,
    code: &str,
    verifier: &str,
) -> Result<(String, String), String> {
    let response = http::client()
        .post(AZURE_TOKEN_URL)
        .form(&[
            ("grant_type", "authorization_code"),
            ("client_id", auth.client_id.as_str()),
            ("code", code),
            ("redirect_uri", NATIVE_REDIRECT),
            ("code_verifier", verifier),
            ("scope", AZURE_SCOPE),
        ])
        .send()
        .await
        .map_err(|err| format!("Microsoft token isteği başarısız: {err}"))?;
    let status = response.status();
    let body: Value = response
        .json()
        .await
        .map_err(|err| format!("Token yanıtı ayrıştırılamadı: {err}"))?;
    if !status.is_success() {
        let error = body
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("unknown_error");
        let description = body
            .get("error_description")
            .and_then(Value::as_str)
            .unwrap_or("");
        return Err(format!(
            "Microsoft kod değişimi başarısız ({error}): {description}"
        ));
    }
    let access_token = body
        .get("access_token")
        .and_then(Value::as_str)
        .ok_or_else(|| "Microsoft erişim token'ı bulunamadı".to_string())?
        .to_string();
    let refresh = body
        .get("refresh_token")
        .and_then(Value::as_str)
        .ok_or_else(|| "Microsoft yenileme token'ı bulunamadı".to_string())?
        .to_string();
    Ok((access_token, refresh))
}

async fn oauth_refresh(
    auth: &AuthConfig,
    refresh_token: &str,
) -> Result<(String, String), String> {
    let response = http::client()
        .post(auth.flavor.token_url())
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", auth.client_id.as_str()),
            ("refresh_token", refresh_token),
            ("scope", auth.flavor.scope()),
        ])
        .send()
        .await
        .map_err(|err| format!("Microsoft token yenileme isteği başarısız: {err}"))?;
    let status = response.status();
    let body: Value = response
        .json()
        .await
        .map_err(|err| format!("Token yanıtı ayrıştırılamadı: {err}"))?;
    if !status.is_success() {
        return Err(format!(
            "Microsoft token yenilenemedi (HTTP {}): {}",
            status.as_u16(),
            trim_message(&body.to_string())
        ));
    }
    let access_token = body
        .get("access_token")
        .and_then(Value::as_str)
        .ok_or_else(|| "Yenilenen erişim token'ı bulunamadı".to_string())?
        .to_string();
    let refresh = body
        .get("refresh_token")
        .and_then(Value::as_str)
        .unwrap_or(refresh_token)
        .to_string();
    Ok((access_token, refresh))
}

fn trim_message(message: &str) -> String {
    let compact = message.replace(['\n', '\r'], " ");
    if compact.chars().count() > 400 {
        compact.chars().take(400).collect()
    } else {
        compact
    }
}

fn xsts_error_message(xerr: i64) -> String {
    match xerr {
        2148916233 => "Bu Microsoft hesabına bağlı bir Xbox hesabı yok. Önce Xbox profili oluşturun.".to_string(),
        2148916235 => "Xbox Live bu ülkede kullanılamıyor.".to_string(),
        2148916236 | 2148916237 => "Hesap doğrulaması gerekli (yetişkin onayı). Xbox hesabınızı doğrulayın.".to_string(),
        2148916238 => "Çocuk hesabı bir Microsoft ailesine eklenmeli.".to_string(),
        _ => format!("Xbox Live kimlik doğrulaması başarısız (XErr: {xerr})"),
    }
}

async fn xbox_minecraft_auth(
    ms_access_token: &str,
    flavor: AuthFlavor,
) -> Result<McAuthResult, String> {
    let xbl_response = http::client()
        .post("https://user.auth.xboxlive.com/user/authenticate")
        .header("x-xbl-contract-version", "1")
        .json(&json!({
            "Properties": {
                "AuthMethod": "RPS",
                "SiteName": "user.auth.xboxlive.com",
                "RpsTicket": flavor.rps_ticket(ms_access_token)
            },
            "RelyingParty": "http://auth.xboxlive.com",
            "TokenType": "JWT"
        }))
        .send()
        .await
        .map_err(|err| format!("Xbox Live'a bağlanılamadı: {err}"))?;
    let xbl_status = xbl_response.status();
    let xbl_json: Value = xbl_response
        .json()
        .await
        .map_err(|err| format!("Xbox Live yanıtı ayrıştırılamadı: {err}"))?;
    if !xbl_status.is_success() {
        return Err(format!(
            "Xbox Live girişi başarısız (HTTP {}): {}",
            xbl_status.as_u16(),
            trim_message(&xbl_json.to_string())
        ));
    }
    let xbl_token = xbl_json
        .get("Token")
        .and_then(Value::as_str)
        .ok_or_else(|| "Xbox Live token'ı bulunamadı".to_string())?
        .to_string();
    let uhs = xbl_json
        .pointer("/DisplayClaims/xui/0/uhs")
        .and_then(Value::as_str)
        .ok_or_else(|| "Xbox kullanıcı bilgisi (uhs) bulunamadı".to_string())?
        .to_string();

    let xsts_response = http::client()
        .post("https://xsts.auth.xboxlive.com/xsts/authorize")
        .header("x-xbl-contract-version", "1")
        .json(&json!({
            "Properties": {
                "SandboxId": "RETAIL",
                "UserTokens": [xbl_token]
            },
            "RelyingParty": "rp://api.minecraftservices.com/",
            "TokenType": "JWT"
        }))
        .send()
        .await
        .map_err(|err| format!("XSTS isteği başarısız: {err}"))?;
    let xsts_status = xsts_response.status();
    let xsts_json: Value = xsts_response
        .json()
        .await
        .map_err(|err| format!("XSTS yanıtı ayrıştırılamadı: {err}"))?;
    if !xsts_status.is_success() {
        if let Some(xerr) = xsts_json.get("XErr").and_then(Value::as_i64) {
            return Err(xsts_error_message(xerr));
        }
        return Err(format!(
            "XSTS yetkilendirmesi başarısız (HTTP {}): {}",
            xsts_status.as_u16(),
            trim_message(&xsts_json.to_string())
        ));
    }
    let xsts_token = xsts_json
        .get("Token")
        .and_then(Value::as_str)
        .ok_or_else(|| "XSTS token'ı bulunamadı".to_string())?
        .to_string();

    let mc_response = http::client()
        .post("https://api.minecraftservices.com/authentication/login_with_xbox")
        .json(&json!({ "identityToken": format!("XBL3.0 x={uhs};{xsts_token}") }))
        .send()
        .await
        .map_err(|err| format!("Minecraft servisine bağlanılamadı: {err}"))?;
    let mc_status = mc_response.status();
    let mc_json: Value = mc_response
        .json()
        .await
        .map_err(|err| format!("Minecraft yanıtı ayrıştırılamadı: {err}"))?;
    if !mc_status.is_success() {
        return Err(format!(
            "Minecraft oturumu açılamadı (HTTP {}): {}",
            mc_status.as_u16(),
            trim_message(&mc_json.to_string())
        ));
    }
    let access_token = mc_json
        .get("access_token")
        .and_then(Value::as_str)
        .ok_or_else(|| "Minecraft erişim token'ı bulunamadı".to_string())?
        .to_string();
    let expires_in = mc_json
        .get("expires_in")
        .and_then(Value::as_u64)
        .unwrap_or(86_400);
    let expires_at = now_millis() / 1000 + expires_in;

    let profile_response = http::client()
        .get("https://api.minecraftservices.com/minecraft/profile")
        .header("Authorization", format!("Bearer {access_token}"))
        .send()
        .await
        .map_err(|err| format!("Minecraft profili alınamadı: {err}"))?;
    if profile_response.status().as_u16() == 404 {
        return Err(
            "Minecraft profili alınamadı. Hesabınızda Minecraft Java Edition lisansı olduğundan emin olun."
                .to_string(),
        );
    }
    let profile_status = profile_response.status();
    let profile: Value = profile_response
        .json()
        .await
        .map_err(|err| format!("Minecraft profili ayrıştırılamadı: {err}"))?;
    if !profile_status.is_success() {
        return Err(format!(
            "Minecraft profili alınamadı (HTTP {}): {}",
            profile_status.as_u16(),
            trim_message(&profile.to_string())
        ));
    }
    let username = profile
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| "Minecraft kullanıcı adı bulunamadı".to_string())?
        .to_string();
    let uuid = profile
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| "Minecraft UUID bulunamadı".to_string())?
        .to_string();

    Ok(McAuthResult {
        access_token,
        expires_at,
        username,
        uuid,
    })
}

fn store_token(
    store: &Store,
    secrets: &SecretBox,
    account_id: &str,
    oauth_access: &str,
    refresh_token: &str,
    mc: &McAuthResult,
    auth: &AuthConfig,
) {
    let token_data = json!({
        "oauthAccessToken": oauth_access,
        "refreshToken": refresh_token,
        "mcAccessToken": mc.access_token,
        "expiresAt": mc.expires_at,
        "username": mc.username,
        "uuid": mc.uuid,
        "clientId": auth.client_id,
        "authFlavor": auth.flavor.as_str()
    });
    let encrypted = secrets.encrypt(&token_data.to_string());
    store.map_array("accounts", |accounts| {
        if let Some(account) = accounts
            .iter_mut()
            .find(|account| account.get("id").and_then(Value::as_str) == Some(account_id))
        {
            account["encryptedToken"] = Value::String(encrypted.clone());
            account["username"] = Value::String(mc.username.clone());
            account["uuid"] = Value::String(mc.uuid.clone());
            account["updatedAt"] = json!(now_millis());
        }
    });
}

pub async fn login_with_microsoft(
    app: &AppHandle,
    store: &Store,
    secrets: &SecretBox,
) -> Result<Value, String> {
    let auth = AuthConfig::from_env();
    notify(app, "info", "Microsoft girişi başlatılıyor...");

    // Önceki giriş penceresi varsa kapat (çift tıklama vb.)
    if let Some(existing) = app.get_webview_window("ms-auth") {
        let _ = existing.close();
        tokio::time::sleep(Duration::from_millis(400)).await;
    }

    let (verifier, challenge) = pkce_pair();
    let state = Uuid::new_v4().to_string();
    let authorize_url = format!(
        "{AZURE_AUTHORIZE_URL}?client_id={client_id}&response_type=code&redirect_uri={redirect}&scope={scope}&code_challenge={challenge}&code_challenge_method=S256&state={state}&prompt=select_account",
        client_id = auth.client_id,
        redirect = NATIVE_REDIRECT,
        scope = "XboxLive.signin%20offline_access",
    );
    let parsed_url: tauri::Url = authorize_url
        .parse()
        .map_err(|err| format!("Giriş adresi oluşturulamadı: {err}"))?;

    let (tx, rx) = mpsc::channel::<Result<String, String>>();
    let success = Arc::new(AtomicBool::new(false));

    let nav_tx = tx.clone();
    let nav_state = state.clone();
    let window = WebviewWindowBuilder::new(app, "ms-auth", WebviewUrl::External(parsed_url))
        .title("Microsoft ile Giriş")
        .inner_size(430.0, 620.0)
        .min_inner_size(360.0, 480.0)
        .resizable(true)
        .center()
        .on_navigation(move |nav_url| {
            if !nav_url.as_str().starts_with(NATIVE_REDIRECT) {
                return true;
            }
            let pairs: Vec<(String, String)> = nav_url
                .query_pairs()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect();
            let state_ok = pairs
                .iter()
                .any(|(key, value)| key == "state" && *value == nav_state);
            if let Some((_, code)) = pairs.iter().find(|(key, _)| key == "code") {
                if state_ok {
                    let _ = nav_tx.send(Ok(code.clone()));
                }
                return false;
            }
            if let Some((_, error)) = pairs.iter().find(|(key, _)| key == "error") {
                let _ = nav_tx.send(Err(format!(
                    "Microsoft girişi tamamlanamadı ({error})"
                )));
                return false;
            }
            true
        })
        .build()
        .map_err(|err| format!("Giriş penceresi açılamadı: {err}"))?;

    let close_tx = tx.clone();
    let close_flag = success.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::Destroyed = event {
            if !close_flag.load(Ordering::Relaxed) {
                let _ = close_tx.send(Err("Giriş penceresi kapatıldı.".to_string()));
            }
        }
    });

    notify(app, "info", "Microsoft giriş penceresi açıldı...");
    let received = tokio::task::spawn_blocking(move || {
        rx.recv_timeout(Duration::from_secs(300))
    })
    .await
    .map_err(|err| format!("Giriş akışı kesildi: {err}"))?;

    let code = match received {
        Ok(Ok(code)) => code,
        Ok(Err(err)) => {
            let _ = window.close();
            return Err(err);
        }
        Err(_) => {
            let _ = window.close();
            return Err(
                "Microsoft giriş penceresi zaman aşımına uğradı (5 dakika). Lütfen tekrar deneyin."
                    .to_string(),
            );
        }
    };
    success.store(true, Ordering::Relaxed);
    let _ = window.close();

    notify(app, "info", "Xbox Live ve Minecraft oturumu doğrulanıyor...");
    let (oauth_access, refresh_token) = exchange_authorization_code(&auth, &code, &verifier).await?;
    let mc = xbox_minecraft_auth(&oauth_access, auth.flavor).await?;

    let mut accounts = store.array("accounts");
    let existing_index = accounts.iter().position(|account| {
        account.get("type").and_then(Value::as_str) == Some("microsoft")
            && account.get("uuid").and_then(Value::as_str) == Some(mc.uuid.as_str())
    });
    let account_id = existing_index
        .and_then(|index| accounts[index].get("id").and_then(Value::as_str))
        .map(ToString::to_string)
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let created_at = existing_index
        .and_then(|index| accounts[index].get("createdAt").cloned())
        .unwrap_or_else(|| json!(now_millis()));

    let account = json!({
        "id": account_id,
        "type": "microsoft",
        "username": mc.username,
        "uuid": mc.uuid,
        "encryptedToken": "",
        "createdAt": created_at,
        "updatedAt": now_millis()
    });

    if let Some(index) = existing_index {
        accounts[index] = account.clone();
    } else {
        accounts.push(account.clone());
    }
    store.set("accounts", Value::Array(accounts));
    store_token(
        store,
        secrets,
        account_id.as_str(),
        &oauth_access,
        &refresh_token,
        &mc,
        &auth,
    );

    if store.get_string("activeAccountId").is_none() {
        store.set_active_account(Some(account_id.clone()));
    }

    let saved = store
        .array("accounts")
        .into_iter()
        .find(|item| item.get("id").and_then(Value::as_str) == Some(account_id.as_str()))
        .unwrap_or(account);
    notify(app, "success", &format!("Microsoft hesabı eklendi: {}", mc.username));
    Ok(safe_account(&saved))
}

pub async fn get_mclc_auth(
    store: &Store,
    secrets: &SecretBox,
    account_id: &str,
) -> Result<MclcAuth, String> {
    let full_account = store
        .array("accounts")
        .into_iter()
        .find(|account| account.get("id").and_then(Value::as_str) == Some(account_id))
        .ok_or_else(|| "Hesap bulunamadı".to_string())?;
    let account_type = full_account
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("offline");

    if account_type == "offline" {
        return Ok(MclcAuth {
            access_token: "0".to_string(),
            client_token: Uuid::new_v4().to_string(),
            uuid: full_account
                .get("uuid")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            username: full_account
                .get("username")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            user_type: "legacy".to_string(),
            user_properties: "{}".to_string(),
        });
    }

    let encrypted = full_account
        .get("encryptedToken")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Microsoft hesabı için kayıtlı oturum bulunamadı, lütfen yeniden giriş yapın.".to_string())?;
    let token_data: Value = secrets
        .decrypt(encrypted)
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .ok_or_else(|| "Oturum bilgisi okunamadı, lütfen yeniden giriş yapın.".to_string())?;

    let refresh_token = token_data
        .get("refreshToken")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if !refresh_token.is_empty() {
        let auth = AuthConfig::from_token_data(&token_data);
        match oauth_refresh(&auth, &refresh_token).await {
            Ok((oauth_access, new_refresh)) => {
                match xbox_minecraft_auth(&oauth_access, auth.flavor).await {
                    Ok(mc) => {
                        store_token(
                            store,
                            secrets,
                            account_id,
                            &oauth_access,
                            &new_refresh,
                            &mc,
                            &auth,
                        );
                        return Ok(MclcAuth {
                            access_token: mc.access_token,
                            client_token: Uuid::new_v4().to_string(),
                            uuid: mc.uuid,
                            username: mc.username,
                            user_type: "msa".to_string(),
                            user_properties: "{}".to_string(),
                        });
                    }
                    Err(err) => {
                        crate::logger::warn(&format!(
                            "Microsoft token sessiz yenileme başarısız, mevcut token ile devam ediliyor: {err}"
                        ));
                    }
                }
            }
            Err(err) => {
                crate::logger::warn(&format!(
                    "Microsoft token sessiz yenileme başarısız, mevcut token ile devam ediliyor: {err}"
                ));
            }
        }
    }

    let access_token = token_data
        .get("mcAccessToken")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Minecraft erişim token'ı bulunamadı, lütfen yeniden giriş yapın.".to_string())?
        .to_string();

    Ok(MclcAuth {
        access_token,
        client_token: Uuid::new_v4().to_string(),
        uuid: token_data
            .get("uuid")
            .and_then(Value::as_str)
            .or_else(|| full_account.get("uuid").and_then(Value::as_str))
            .unwrap_or_default()
            .to_string(),
        username: token_data
            .get("username")
            .and_then(Value::as_str)
            .or_else(|| full_account.get("username").and_then(Value::as_str))
            .unwrap_or_default()
            .to_string(),
        user_type: "msa".to_string(),
        user_properties: "{}".to_string(),
    })
}