# Buran Launcher

Backend'i tamamen Rust/Tauri v2 olan Minecraft launcher. Electron tabanlı eski backend (`src/main`, `src/preload`) kaldırılmıştır.

## Gereksinimler

- [Node.js](https://nodejs.org/) ve npm
- [Rust](https://www.rust-lang.org/tools/install)
- Windows'ta WebView2 (Windows 10/11'de genellikle hazır)
- Linux'ta Tauri sistem bağımlılıkları (`webkit2gtk-4.1`, `libappindicator3`, `librsvg2` vb.)

## Kurulum ve Çalıştırma

```bash
npm install
npm run dev
```

Uygulama verileri `%APPDATA%\com.buranlauncher.app` altında tutulur.

## Proje Yapısı

- `src-tauri/`: Rust/Tauri backend ve uygulama yapılandırması
- `src/renderer/`: Tauri arayüzü (HTML/CSS/JS)
- `assets/`, `locales/`: Statik kaynaklar

## Paketleme

```bash
npm run build:win     # Windows: NSIS kurulum
npm run build:linux   # Linux: AppImage + .deb
npm run build         # Platformun varsayılan paketleri
```

Çıktılar `src-tauri/target/release/bundle/` altında oluşur.

## Ortam Değişkenleri

Varsayılan değerler kodda gömülüdür; `.env` yalnızca bunları geçersiz kılmak için kullanılır:

- `DISCORD_CLIENT_ID`: Discord Rich Presence için uygulama kimliği (varsayılan: `1563197891261235302`).
- `BURAN_GITHUB_REPO`: Güncelleme kontrolü için GitHub deposu; `kullanici/repo` veya tam GitHub URL'si kabul edilir (varsayılan: `lionesslie/BuranLauncher`).
- `MS_CLIENT_ID`: İsteğe bağlı özel Azure client ID'si. Tanımlanmazsa Microsoft'un genel Minecraft launcher client ID'siyle `login.live.com` cihaz kodu akışı kullanılır; özel Azure uygulaması verilirse `login.microsoftonline.com` akışına geçilir.