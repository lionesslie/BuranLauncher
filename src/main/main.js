// Buran Launcher - Ana süreç (main process) giriş noktası
require('dotenv').config({ path: require('path').join(__dirname, '..', '..', '.env') });

const { app, BrowserWindow, Tray, Menu, nativeImage } = require('electron');
const path = require('path');
const log = require('electron-log');

const store = require('./store');
const i18n = require('./i18n');
const { registerIpcHandlers } = require('./ipcHandlers');

log.initialize && log.initialize();
log.transports.file.resolvePathFn = () => path.join(app.getPath('userData'), 'logs', 'main.log');

let mainWindow = null;
let tray = null;
let isQuitting = false;

function createWindow() {
  mainWindow = new BrowserWindow({
    width: 1200,
    height: 760,
    minWidth: 960,
    minHeight: 600,
    frame: false, // custom title bar kullanılacak
    backgroundColor: '#111318',
    icon: path.join(__dirname, '..', '..', 'assets', 'icons', process.platform === 'win32' ? 'icon.ico' : 'icon.png'),
    webPreferences: {
      preload: path.join(__dirname, '..', 'preload', 'preload.js'),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: false
    }
  });

  mainWindow.loadFile(path.join(__dirname, '..', 'renderer', 'index.html'));

  mainWindow.on('close', (e) => {
    const settings = store.get('settings');
    if (!isQuitting && settings.minimizeToTray) {
      e.preventDefault();
      mainWindow.hide();
    }
  });

  if (process.argv.includes('--dev')) {
    mainWindow.webContents.openDevTools();
  }
}

function createTray() {
  const iconPath = path.join(__dirname, '..', '..', 'assets', 'icons', process.platform === 'win32' ? 'icon.ico' : 'icon.png');
  const image = nativeImage.createFromPath(iconPath);
  tray = new Tray(image.isEmpty() ? image : image.resize({ width: 16, height: 16 }));
  tray.setToolTip('Buran Launcher');

  const { translations } = i18n.getTranslations();
  const menu = Menu.buildFromTemplate([
    {
      label: translations.app.name,
      enabled: false
    },
    { type: 'separator' },
    {
      label: translations.common.loading === 'Yükleniyor...' ? 'Aç' : 'Open',
      click: () => mainWindow.show()
    },
    {
      label: 'Çıkış / Выход',
      click: () => {
        isQuitting = true;
        app.quit();
      }
    }
  ]);
  tray.setContextMenu(menu);

  tray.on('click', () => {
    mainWindow.isVisible() ? mainWindow.hide() : mainWindow.show();
  });
}

app.whenReady().then(() => {
  const settings = store.get('settings');
  app.setLoginItemSettings({ openAtLogin: !!settings.startOnBoot });

  createWindow();
  createTray();
  registerIpcHandlers(mainWindow);

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow();
  });
});

app.on('before-quit', () => { isQuitting = true; });

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') app.quit();
});

process.on('uncaughtException', (err) => {
  log.error('Yakalanmamış hata:', err);
});
