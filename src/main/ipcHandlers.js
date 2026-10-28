// Renderer <-> Main süreç arasındaki tüm IPC işleyicileri burada toplanır
const { ipcMain, dialog, app, shell } = require('electron');
const path = require('path');
const fs = require('fs');
const log = require('electron-log');

const store = require('./store');
const i18n = require('./i18n');
const authManager = require('./authManager');
const instanceManager = require('./instanceManager');
const versionManager = require('./versionManager');
const modrinthManager = require('./modrinthManager');
const loaderManager = require('./loaderManager');
const javaManager = require('./javaManager');
const launchManager = require('./launchManager');
const backgroundManager = require('./backgroundManager');
const worldManager = require('./worldManager');
const updater = require('./updater');

function wrap(handler) {
  return async (event, ...args) => {
    try {
      return { ok: true, data: await handler(event, ...args) };
    } catch (err) {
      log.error('IPC hatası:', err);
      return { ok: false, error: err.message || String(err) };
    }
  };
}

function registerIpcHandlers(mainWindow) {
  // --- i18n ---
  ipcMain.handle('i18n:get', wrap(() => i18n.getTranslations()));
  ipcMain.handle('i18n:setLang', wrap((e, lang) => i18n.setLang(lang)));

  // --- Hesaplar ---
  ipcMain.handle('accounts:list', wrap(() => authManager.listAccounts()));
  ipcMain.handle('accounts:getActive', wrap(() => authManager.getActiveAccount()));
  ipcMain.handle('accounts:addOffline', wrap((e, username) => authManager.addOfflineAccount(username)));
  ipcMain.handle('accounts:loginMicrosoft', wrap(() => authManager.loginWithMicrosoft(mainWindow)));
  ipcMain.handle('accounts:setActive', wrap((e, id) => authManager.setActiveAccount(id)));
  ipcMain.handle('accounts:remove', wrap((e, id) => authManager.removeAccount(id)));

  // --- Sürümler ---
  ipcMain.handle('versions:list', wrap((e, type) => versionManager.listVersions(type)));

  // --- Yükleyiciler ---
  ipcMain.handle('loader:fabricVersions', wrap(() => loaderManager.listFabricLoaderVersions()));
  ipcMain.handle('loader:quiltVersions', wrap(() => loaderManager.listQuiltLoaderVersions()));
  ipcMain.handle('loader:install', wrap(async (e, { type, mcVersion, loaderVersion }) => {
    const settings = store.get('settings');
    const javaPath = settings.javaPath || javaManager.detectSystemJava();
    const onProgress = status => mainWindow.webContents.send('loader:progress', status);
    if (type === 'fabric') return loaderManager.installFabric(mcVersion, loaderVersion, onProgress);
    if (type === 'quilt') return loaderManager.installQuilt(mcVersion, loaderVersion, onProgress);
    if (type === 'forge' || type === 'neoforge') {
      if (!javaPath) throw new Error('Yükleyici kurulumu için Java gerekli');
      return loaderManager.installForgeLike({ type, mcVersion, loaderVersion, javaBinPath: javaPath, onProgress });
    }
    throw new Error('Bilinmeyen yükleyici türü: ' + type);
  }));

  // --- Instance'lar (Profiller) ---
  ipcMain.handle('instances:list', wrap(() => instanceManager.listInstances()));
  ipcMain.handle('instances:create', wrap((e, data) => instanceManager.createInstance(data)));
  ipcMain.handle('instances:update', wrap((e, { id, patch }) => instanceManager.updateInstance(id, patch)));
  ipcMain.handle('instances:duplicate', wrap((e, { id, newName }) => instanceManager.duplicateInstance(id, newName)));
  ipcMain.handle('instances:delete', wrap((e, id) => instanceManager.deleteInstance(id)));
  ipcMain.handle('instances:export', wrap(async (e, id) => {
    const instance = instanceManager.getInstance(id);
    const { filePath, canceled } = await dialog.showSaveDialog(mainWindow, {
      defaultPath: `${instance.name}.zip`,
      filters: [{ name: 'Zip Arşivi', extensions: ['zip'] }]
    });
    if (canceled || !filePath) return null;
    return instanceManager.exportInstance(id, filePath);
  }));
  ipcMain.handle('instances:import', wrap(async () => {
    const { filePaths, canceled } = await dialog.showOpenDialog(mainWindow, {
      filters: [{ name: 'Zip Arşivi', extensions: ['zip'] }],
      properties: ['openFile']
    });
    if (canceled || !filePaths.length) return null;
    return instanceManager.importInstance(filePaths[0]);
  }));
  ipcMain.handle('instances:openFolder', wrap((e, id) => {
    shell.openPath(instanceManager.getInstanceDir(id));
  }));

  // --- Oyunu başlatma ---
  ipcMain.handle('launch:start', wrap(async (e, instanceId) => {
    const result = await launchManager.launchInstance(instanceId, {
      onLog: line => mainWindow.webContents.send('launch:log', { instanceId, line }),
      onStatus: status => mainWindow.webContents.send('launch:status', { instanceId, status }),
      onClose: code => {
        // Oyun kapandığında, launcher hangi davranışta olursa olsun kullanıcıya geri dönsün
        if (!mainWindow.isDestroyed()) {
          mainWindow.show();
          if (mainWindow.isMinimized()) mainWindow.restore();
        }
        mainWindow.webContents.send('launch:closed', { instanceId, code });
      }
    });

    // Oyun başarıyla başlatıldıktan sonra, ayarlardaki "launchBehavior" uygulanır
    const behavior = store.get('settings').launchBehavior;
    if (behavior === 'minimize') mainWindow.minimize();
    else if (behavior === 'close') mainWindow.hide(); // tamamen kapatmak yerine tepsiye küçült, süreç takibi devam eder

    return result;
  }));
  ipcMain.handle('launch:kill', wrap((e, instanceId) => launchManager.killInstance(instanceId)));
  ipcMain.handle('launch:isRunning', wrap((e, instanceId) => launchManager.isRunning(instanceId)));

  // --- Modrinth: Mod Mağazası ---
  ipcMain.handle('modrinth:search', wrap((e, params) => modrinthManager.search(params)));
  ipcMain.handle('modrinth:getProject', wrap((e, id) => modrinthManager.getProject(id)));
  ipcMain.handle('modrinth:getVersions', wrap((e, { id, opts }) => modrinthManager.getProjectVersions(id, opts)));
  ipcMain.handle('modrinth:installMod', wrap((e, { projectId, instanceId, versionId }) => {
    const onProgress = p => mainWindow.webContents.send('modrinth:progress', { projectId, instanceId, ...p });
    return modrinthManager.installModToInstance(projectId, instanceId, { versionId, onProgress });
  }));
  ipcMain.handle('modrinth:listInstalled', wrap((e, instanceId) => modrinthManager.listInstalledMods(instanceId)));
  ipcMain.handle('modrinth:toggleMod', wrap((e, { instanceId, filename, enable }) => modrinthManager.toggleMod(instanceId, filename, enable)));
  ipcMain.handle('modrinth:removeMod', wrap((e, { instanceId, filename }) => modrinthManager.removeMod(instanceId, filename)));
  ipcMain.handle('modrinth:checkUpdates', wrap((e, instanceId) => modrinthManager.checkForUpdates(instanceId)));
  ipcMain.handle('modrinth:updateMod', wrap((e, { instanceId, projectId }) => {
    const onProgress = p => mainWindow.webContents.send('modrinth:progress', { projectId, instanceId, ...p });
    return modrinthManager.updateMod(instanceId, projectId, { onProgress });
  }));

  // --- Modrinth: Kaynak Paketi / Shader (tek sayfadan, otomatik sürüm seçimi) ---
  ipcMain.handle('modrinth:searchContent', wrap((e, params) => modrinthManager.search(params)));
  ipcMain.handle('modrinth:installContent', wrap((e, { projectId, instanceId, projectType }) => {
    const onProgress = p => mainWindow.webContents.send('modrinth:progress', { projectId, instanceId, ...p });
    return modrinthManager.installContentToInstance(projectId, instanceId, { projectType, onProgress });
  }));
  ipcMain.handle('modrinth:listInstalledContent', wrap((e, { instanceId, projectType }) => modrinthManager.listInstalledContent(instanceId, projectType)));
  ipcMain.handle('modrinth:removeContent', wrap((e, { instanceId, projectType, filename }) => modrinthManager.removeContent(instanceId, projectType, filename)));

  // --- Modrinth: Modpack ---
  ipcMain.handle('modrinth:searchModpacks', wrap((e, params) => modrinthManager.search({ ...params, projectType: 'modpack' })));
  ipcMain.handle('modrinth:getModpackVersions', wrap((e, projectId) => modrinthManager.getProjectVersions(projectId)));
  ipcMain.handle('modrinth:installModpackFromProject', wrap((e, { projectId, versionId }) => {
    const onProgress = p => mainWindow.webContents.send('modpack:progress', p);
    return modrinthManager.installModpackFromProject(projectId, versionId, { onProgress });
  }));
  ipcMain.handle('modrinth:installModpackFile', wrap(async () => {
    const { filePaths, canceled } = await dialog.showOpenDialog(mainWindow, {
      filters: [{ name: 'Modrinth Modpack', extensions: ['mrpack'] }],
      properties: ['openFile']
    });
    if (canceled || !filePaths.length) return null;
    const onProgress = p => mainWindow.webContents.send('modpack:progress', p);
    return modrinthManager.installModpackFromFile(filePaths[0], { onProgress });
  }));

  // --- Java ---
  ipcMain.handle('java:detect', wrap(() => javaManager.detectSystemJava()));
  ipcMain.handle('java:browse', wrap(async () => {
    const { filePaths, canceled } = await dialog.showOpenDialog(mainWindow, { properties: ['openFile'] });
    if (canceled || !filePaths.length) return null;
    return filePaths[0];
  }));
  ipcMain.handle('java:download', wrap(async (e, majorVersion) => {
    const onProgress = p => mainWindow.webContents.send('java:progress', p);
    const javaBin = await javaManager.downloadAndInstallJava(majorVersion, onProgress);
    const settings = store.get('settings');
    settings.javaPath = javaBin;
    store.set('settings', settings);
    return javaBin;
  }));

  // --- Ayarlar ---
  ipcMain.handle('settings:get', wrap(() => store.get('settings')));
  ipcMain.handle('settings:set', wrap((e, patch) => {
    const current = store.get('settings');
    const updated = { ...current, ...patch };
    store.set('settings', updated);
    app.setLoginItemSettings({ openAtLogin: !!updated.startOnBoot });
    return updated;
  }));

  // --- Otomatik güncelleme ---
  ipcMain.handle('updater:check', wrap(() => updater.checkForUpdates()));
  ipcMain.handle('updater:download', wrap(() => updater.downloadUpdate()));
  ipcMain.handle('updater:install', wrap(() => updater.quitAndInstall()));
  ipcMain.handle('updater:getVersion', wrap(() => updater.getCurrentVersion()));

  // --- Dünyalar (saves) ---
  ipcMain.handle('worlds:list', wrap((e, instanceId) => worldManager.listWorlds(instanceId)));
  ipcMain.handle('worlds:delete', wrap((e, { instanceId, folderName }) => worldManager.deleteWorld(instanceId, folderName)));
  ipcMain.handle('worlds:openFolder', wrap((e, { instanceId, folderName }) => worldManager.openWorldFolder(instanceId, folderName)));
  ipcMain.handle('worlds:backup', wrap(async (e, { instanceId, folderName }) => {
    const { filePath, canceled } = await dialog.showSaveDialog(mainWindow, {
      defaultPath: `${folderName}.zip`,
      filters: [{ name: 'Zip Arşivi', extensions: ['zip'] }]
    });
    if (canceled || !filePath) return null;
    return worldManager.backupWorld(instanceId, folderName, filePath);
  }));
  ipcMain.handle('worlds:import', wrap(async (e, instanceId) => {
    const { filePaths, canceled } = await dialog.showOpenDialog(mainWindow, {
      filters: [{ name: 'Zip Arşivi', extensions: ['zip'] }],
      properties: ['openFile']
    });
    if (canceled || !filePaths.length) return null;
    return worldManager.importWorld(instanceId, filePaths[0]);
  }));

  // --- Arkaplan ---
  ipcMain.handle('background:get', wrap(() => backgroundManager.getBackgroundState()));
  ipcMain.handle('background:choose', wrap(async () => {
    const { filePaths, canceled } = await dialog.showOpenDialog(mainWindow, {
      filters: [{ name: 'Görseller', extensions: ['png', 'jpg', 'jpeg', 'webp', 'gif'] }],
      properties: ['openFile']
    });
    if (canceled || !filePaths.length) return null;
    return backgroundManager.setCustomBackground(filePaths[0]);
  }));
  ipcMain.handle('background:reset', wrap(() => backgroundManager.resetBackground()));

  // --- Pencere kontrolleri ---
  ipcMain.handle('window:minimize', wrap(() => mainWindow.minimize()));
  ipcMain.handle('window:maximize', wrap(() => {
    if (mainWindow.isMaximized()) mainWindow.unmaximize();
    else mainWindow.maximize();
  }));
  ipcMain.handle('window:close', wrap(() => {
    const settings = store.get('settings');
    if (settings.minimizeToTray) mainWindow.hide();
    else mainWindow.close();
  }));
}

module.exports = { registerIpcHandlers };
