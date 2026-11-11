use reqwest::Client;
use std::sync::OnceLock;

pub const USER_AGENT: &str = "BuranLauncher/1.0.0 (iletisim: destek@buranlauncher.app)";

pub fn client() -> &'static Client {
    static CLIENT: OnceLock<Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        Client::builder()
            .user_agent(USER_AGENT)
            .build()
            .expect("HTTP istemcisi oluşturulamadı")
    })
}

pub async fn get_json(url: &str) -> Result<serde_json::Value, String> {
    let response = client()
        .get(url)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .send()
        .await
        .map_err(|err| format!("Ağ hatası: {err}"))?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status().as_u16()));
    }
    response
        .json()
        .await
        .map_err(|err| format!("Yanıt ayrıştırılamadı: {err}"))
}

pub async fn download_bytes(url: &str) -> Result<Vec<u8>, String> {
    let response = client()
        .get(url)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .send()
        .await
        .map_err(|err| format!("İndirme hatası: {err}"))?;
    if !response.status().is_success() {
        return Err(format!("HTTP {} - {url}", response.status().as_u16()));
    }
    response
        .bytes()
        .await
        .map(|bytes| bytes.to_vec())
        .map_err(|err| format!("İndirme okunamadı: {err}"))
}

pub fn sha1_hex(bytes: &[u8]) -> String {
    use sha1::{Digest, Sha1};
    let mut hasher = Sha1::new();
    hasher.update(bytes);
    hex_encode(&hasher.finalize())
}

pub fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

pub fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}