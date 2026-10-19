# Buran Launcher

Electron tabanlı, TR/RU dil destekli Minecraft başlatıcısı.

## Kurulum ve Çalıştırma

```bash
npm install
cp .env.example .env
# .env içine Azure App Registration'dan aldığınız MS_CLIENT_ID değerini girin
npm start
```

Geliştirme modunda (DevTools açık) çalıştırmak için:
```bash
npm run dev
```

## Paketleme (installer üretme)

```bash
npm run build:win     # Windows: NSIS kurulum + portable .exe
npm run build:linux   # Linux: AppImage + .deb
npm run build         # ikisi birden
```
Çıktılar `dist/` klasöründe oluşur.

## Eksik varlıklar (siz eklemelisiniz)

- `assets/fonts/Inter-Regular.woff2`, `Inter-Medium.woff2`, `Inter-SemiBold.woff2`
  → [rsms.me/inter](https://rsms.me/inter/) adresinden indirip bu klasöre koyun.
- `assets/icons/icon.ico` (Windows) ve `assets/icons/icon.png` (Linux, 512x512)
  → Buran Launcher logosu buraya eklenmeli.
- Üretimde ikonlar için `npm i lucide-static` kurup `node_modules/lucide-static/icons/*.svg`
  dosyalarını kullanmak, `src/renderer/js/icons.js` içindeki local setin yerini alabilir.

## Mimari Özet

- `src/main/` — Electron ana süreç: pencere, tray, IPC, auth, instance/mod/java yönetimi
- `src/preload/` — contextBridge ile güvenli, sınırlı API yüzeyi
- `src/renderer/` — Vanilla JS arayüz (framework yok), sayfa bazlı modüller
- `locales/` — tr.json / ru.json (tek kaynak, yeni dil eklemek için aynı şemada yeni dosya yeterli)
