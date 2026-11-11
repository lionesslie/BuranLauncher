use crate::http::{self, now_millis};
use crate::secrets::SecretBox;
use crate::store::Store;
use serde_json::{json, Value};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tauri_plugin_dialog::DialogExt;
use uuid::Uuid;

const DEFAULT_CLIENT_ID: &str = "00000000402b5328";
const DEVICE_CODE_URL: &str =
    "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
const TOKEN_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const SCOPE: &str = "XboxLive.signin offline_access";

#[derive(Clone)]
pub struct MclcAuth {
    pub access_token: String,
    pub client_token: String,
    pub uuid: String,
    pub username: String,
    pub user_type: String,
    pub user_properties: String,
}

fn client_id() -> String {
    std::env::var("MS_CLIENT_ID")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty() && value != "BURAYA_AZURE_CLIENT_ID_YAZIN")
        .unwrap_or_else(|| DEFAULT_CLIENT_ID.to_string())
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

async fn request_device_code(client_id: &str) -> Result<Value, String> {
    let response = http::client()
        .post(DEVICE_CODE_URL)
        .form(&[("client_id", client_id), ("scope", SCOPE)])
        .send()
        .await
        .map_err(|err| format!("Microsoft'a bağlanılamadı: {err}"))?;
    if !response.status().is_success() {
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "Microsoft cihaz kodu alınamadı (HTTP {status}): {}",
            trim_message(&body)
        ));
    }
    response
        .json()
        .await
        .map_err(|err| format!("Microsoft yanıtı ayrıştırılamadı: {err}"))
}

async fn poll_device_token(
    client_id: &str,
    device_code: &str,
    interval: u64,
    expires_in: u64,
) -> Result<Value, String> {
    let started = Instant::now();
    let mut wait = interval.max(1);
    loop {
        if started.elapsed() > Duration::from_secs(expires_in.max(30)) {
            return Err("Microsoft giriş süresi doldu. Lütfen tekrar deneyin.".to_string());
        }
        tokio::time::sleep(Duration::from_secs(wait)).await;
        let response = http::client()
            .post(TOKEN_URL)
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("client_id", client_id),
                ("device_code", device_code),
            ])
            .send()
            .await
            .map_err(|err| format!("Token isteği başarısız: {err}"))?;
        let status = response.status();
        let body: Value = response
            .json()
            .await
            .map_err(|err| format!("Token yanıtı ayrıştırılamadı: {err}"))?;
        if status.is_success() {
            return Ok(body);
        }
        let error = body
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("unknown_error");
        match error {
            "authorization_pending" => continue,
            "slow_down" => {
                wait += 5;
                continue;
            }
            "authorization_declined" => {
                return Err("Microsoft girişi iptal edildi.".to_string())
            }
            "expired_token" => {
                return Err("Microsoft giriş kodunun süresi doldu.".to_string())
            }
            "bad_verification_code" => {
                return Err("Microsoft doğrulama kodu geçersiz.".to_string())
            }
            other => {
                let description = body
                    .get("error_description")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                return Err(format!("Microsoft girişi başarısız ({other}): {description}"));
            }
        }
    }
}

async fn oauth_refresh(client_id: &str, refresh_token: &str) -> Result<(String, String), String> {
    let response = http::client()
        .post(TOKEN_URL)
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", client_id),
            ("refresh_token", refresh_token),
            ("scope", SCOPE),
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

async fn xbox_minecraft_auth(ms_access_token: &str) -> Result<McAuthResult, String> {
    let xbl_response = http::client()
        .post("https://user.auth.xboxlive.com/user/authenticate")
        .header("x-xbl-contract-version", "1")
        .json(&json!({
            "Properties": {
                "AuthMethod": "RPS",
                "SiteName": "user.auth.xboxlive.com",
                "RpsTicket": format!("d={ms_access_token}")
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
) {
    let token_data = json!({
        "oauthAccessToken": oauth_access,
        "refreshToken": refresh_token,
        "mcAccessToken": mc.access_token,
        "expiresAt": mc.expires_at,
        "username": mc.username,
        "uuid": mc.uuid
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
    let client_id = client_id();
    notify(app, "info", "Microsoft girişi başlatılıyor...");

    let device = request_device_code(&client_id).await?;
    let device_code = device
        .get("device_code")
        .and_then(Value::as_str)
        .ok_or_else(|| "Microsoft cihaz kodu alınamadı".to_string())?
        .to_string();
    let user_code = device
        .get("user_code")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let verification_uri = device
        .get("verification_uri")
        .and_then(Value::as_str)
        .unwrap_or("https://microsoft.com/devicelogin")
        .to_string();
    let message = device
        .get("message")
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .unwrap_or_else(|| {
            format!("{verification_uri} adresine gidin ve şu kodu girin: {user_code}")
        });
    let interval = device.get("interval").and_then(Value::as_u64).unwrap_or(5);
    let expires_in = device
        .get("expires_in")
        .and_then(Value::as_u64)
        .unwrap_or(900);

    notify(
        app,
        "info",
        &format!("Tarayıcıda {verification_uri} adresini açın ve şu kodu girin: {user_code}"),
    );
    let _ = open::that(&verification_uri);

    let dialog_app = app.clone();
    let dialog_message = format!(
        "Microsoft ile giriş yapmak için tarayıcıda şu adrese gidin:\n\n{verification_uri}\n\nKod: {user_code}\n\n{message}"
    );
    let _ = tokio::task::spawn_blocking(move || {
        dialog_app
            .dialog()
            .message(dialog_message)
            .title("Microsoft ile Giriş Yap")
            .blocking_show();
    })
    .await;

    notify(app, "info", "Microsoft giriş onayı bekleniyor...");
    let token = poll_device_token(&client_id, &device_code, interval, expires_in).await?;
    let oauth_access = token
        .get("access_token")
        .and_then(Value::as_str)
        .ok_or_else(|| "Microsoft erişim token'ı bulunamadı".to_string())?
        .to_string();
    let refresh_token = token
        .get("refresh_token")
        .and_then(Value::as_str)
        .ok_or_else(|| "Microsoft yenileme token'ı bulunamadı".to_string())?
        .to_string();

    notify(app, "info", "Xbox Live ve Minecraft oturumu doğrulanıyor...");
    let mc = xbox_minecraft_auth(&oauth_access).await?;

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
        let client_id = client_id();
        match oauth_refresh(&client_id, &refresh_token).await {
            Ok((oauth_access, new_refresh)) => match xbox_minecraft_auth(&oauth_access).await {
                Ok(mc) => {
                    store_token(
                        store,
                        secrets,
                        account_id,
                        &oauth_access,
                        &new_refresh,
                        &mc,
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
            },
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