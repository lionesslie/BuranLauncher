// Launcher arkaplanı: paket içi varsayılan görsel veya kullanıcının seçtiği
// özel görsel, base64 data URL'e çevrilip renderer'a IPC ile gönderilir.
// (CSP'de "self" dosya sisteminin tamamını kapsamadığından, en güvenli ve
// her ortamda çalışan yöntem data: URL kullanmaktır.)
const fs = require('fs');
const path = require('path');
const { app } = require('electron');
const store = require('./store');

const MIME_BY_EXT = {
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.jpeg': 'image/jpeg',
  '.webp': 'image/webp',
  '.gif': 'image/gif'
};

function getDefaultBackgroundPath() {
  return path.join(app.getAppPath(), 'assets', 'backgrounds', 'default.jpg');
}

function getCustomBackgroundsDir() {
  const dir = path.join(app.getPath('userData'), 'backgrounds');
  fs.mkdirSync(dir, { recursive: true });
  return dir;
}

function fileToDataUrl(filePath) {
  const ext = path.extname(filePath).toLowerCase();
  const mime = MIME_BY_EXT[ext] || 'image/png';
  const buf = fs.readFileSync(filePath);
  return `data:${mime};base64,${buf.toString('base64')}`;
}

function getBackgroundState() {
  const settings = store.get('settings');
  let targetPath = settings.backgroundPath && fs.existsSync(settings.backgroundPath)
    ? settings.backgroundPath
    : getDefaultBackgroundPath();

  let dataUrl = null;
  if (settings.backgroundEnabled) {
    try {
      dataUrl = fileToDataUrl(targetPath);
    } catch (err) {
      dataUrl = null; // görsel okunamazsa animasyonlu moda düşer (renderer tarafında ele alınır)
    }
  }

  return { enabled: settings.backgroundEnabled, dataUrl };
}

function setCustomBackground(sourceFilePath) {
  const ext = path.extname(sourceFilePath).toLowerCase() || '.png';
  if (!MIME_BY_EXT[ext]) throw new Error('Desteklenmeyen görsel formatı: ' + ext);

  const destPath = path.join(getCustomBackgroundsDir(), 'custom' + ext);
  fs.copyFileSync(sourceFilePath, destPath);

  const settings = store.get('settings');
  settings.backgroundPath = destPath;
  store.set('settings', settings);

  return getBackgroundState();
}

function resetBackground() {
  const settings = store.get('settings');
  settings.backgroundPath = null;
  store.set('settings', settings);
  return getBackgroundState();
}

module.exports = { getBackgroundState, setCustomBackground, resetBackground };
