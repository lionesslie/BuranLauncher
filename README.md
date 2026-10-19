# Buran Launcher

Electron tabanlı, Minecraft Launcher.

## Kurulum ve Çalıştırma
```bash
npm install
npm start
```

## Paketleme (installer üretme)

```bash
npm run build:win     # Windows: NSIS kurulum + portable .exe
npm run build:linux   # Linux: AppImage + .deb
npm run build         # ikisi birden
```
Çıktılar `dist/` klasöründe oluşur.

```bash
