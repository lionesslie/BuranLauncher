// Launcher'ın kendi otomatik güncellemesi (electron-updater üzerinden).
//
// ÖNEMLİ: Bu özelliğin çalışması için package.json'daki "build.publish"
// alanında gerçek bir GitHub kullanıcı adı/repo belirtilmiş ve o repo'da
// electron-builder ile üretilmiş GitHub Release'ler yayınlanmış olmalı
// (npm run build sonrası electron-builder --publish=always ile, ya da
// manuel release yükleyerek). Yapılandırılmamışsa veya hiç yayın yoksa,
// kontrol sessizce başarısız olur — launcher'ın normal çalışmasını etkilemez.
//
// Ayrıca: paketlenmemiş (geliştirme) modda electron-updater anlamsız
// olduğundan (app.isPackaged false) kontrol tamamen atlanır.
const { autoUpdater } = require('electron-updater');
const { app } = require('electron');
const log = require('electron-log');

let mainWindowRef = null;

autoUpdater.logger = log;
autoUpdater.autoDownload = false; // kullanıcı onayı olmadan otomatik indirme yapılmaz
autoUpdater.autoInstallOnAppQuit = true;

function send(data) {
  if (mainWindowRef && !mainWindowRef.isDestroyed()) {
    mainWindowRef.webContents.send('updater:status', data);
  }
}

function init(mainWindow) {
  mainWindowRef = mainWindow;

  if (!app.isPackaged) {
    log.info('[Güncelleme] Geliştirme modunda (paketlenmemiş), otomatik güncelleme kontrolü atlandı.');
    return;
  }

  autoUpdater.on('checking-for-update', () => send({ stage: 'checking' }));
  autoUpdater.on('update-available', (info) => send({ stage: 'available', version: info.version }));
  autoUpdater.on('update-not-available', () => send({ stage: 'none' }));
  autoUpdater.on('error', (err) => send({ stage: 'error', message: err.message }));
  autoUpdater.on('download-progress', (progress) => send({ stage: 'downloading', percent: Math.round(progress.percent) }));
  autoUpdater.on('update-downloaded', (info) => send({ stage: 'downloaded', version: info.version }));

  // Açılışta sessizce kontrol et (sadece haber verir, otomatik indirmez)
  autoUpdater.checkForUpdates().catch(err => {
    log.warn('[Güncelleme] Açılış kontrolü başarısız (muhtemelen publish yapılandırılmamış): ' + err.message);
  });
}

function checkForUpdates() {
  if (!app.isPackaged) return Promise.resolve({ dev: true });
  return autoUpdater.checkForUpdates();
}

function downloadUpdate() {
  if (!app.isPackaged) return Promise.resolve({ dev: true });
  return autoUpdater.downloadUpdate();
}

function quitAndInstall() {
  autoUpdater.quitAndInstall();
}

function getCurrentVersion() {
  return app.getVersion();
}

module.exports = { init, checkForUpdates, downloadUpdate, quitAndInstall, getCurrentVersion };
