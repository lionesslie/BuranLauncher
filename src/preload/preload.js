// Güvenli IPC köprüsü: contextIsolation açık, nodeIntegration kapalı.
// Renderer, Node.js API'lerine doğrudan erişemez; yalnızca burada tanımlanan
// sınırlı ve güvenli fonksiyonları kullanabilir.
const { contextBridge, ipcRenderer } = require('electron');

function invoke(channel, ...args) {
  return ipcRenderer.invoke(channel, ...args).then(res => {
    if (!res || res.ok === undefined) return res; // ham sonuç (ör. window kontrolleri)
    if (!res.ok) throw new Error(res.error);
    return res.data;
  });
}

contextBridge.exposeInMainWorld('buran', {
  i18n: {
    get: () => invoke('i18n:get'),
    setLang: (lang) => invoke('i18n:setLang', lang)
  },
  accounts: {
    list: () => invoke('accounts:list'),
    getActive: () => invoke('accounts:getActive'),
    addOffline: (username) => invoke('accounts:addOffline', username),
    loginMicrosoft: () => invoke('accounts:loginMicrosoft'),
    setActive: (id) => invoke('accounts:setActive', id),
    remove: (id) => invoke('accounts:remove', id)
  },
  versions: {
    list: (type) => invoke('versions:list', type)
  },
  loader: {
    fabricVersions: () => invoke('loader:fabricVersions'),
    quiltVersions: () => invoke('loader:quiltVersions'),
    install: (params) => invoke('loader:install', params),
    onProgress: (cb) => ipcRenderer.on('loader:progress', (e, data) => cb(data))
  },
  instances: {
    list: () => invoke('instances:list'),
    create: (data) => invoke('instances:create', data),
    update: (id, patch) => invoke('instances:update', { id, patch }),
    duplicate: (id, newName) => invoke('instances:duplicate', { id, newName }),
    delete: (id) => invoke('instances:delete', id),
    export: (id) => invoke('instances:export', id),
    import: () => invoke('instances:import'),
    openFolder: (id) => invoke('instances:openFolder', id)
  },
  launch: {
    start: (instanceId) => invoke('launch:start', instanceId),
    kill: (instanceId) => invoke('launch:kill', instanceId),
    isRunning: (instanceId) => invoke('launch:isRunning', instanceId),
    onLog: (cb) => ipcRenderer.on('launch:log', (e, data) => cb(data)),
    onStatus: (cb) => ipcRenderer.on('launch:status', (e, data) => cb(data)),
    onClosed: (cb) => ipcRenderer.on('launch:closed', (e, data) => cb(data))
  },
  modrinth: {
    search: (params) => invoke('modrinth:search', params),
    getProject: (id) => invoke('modrinth:getProject', id),
    getVersions: (id, opts) => invoke('modrinth:getVersions', { id, opts }),
    installMod: (projectId, instanceId, versionId) => invoke('modrinth:installMod', { projectId, instanceId, versionId }),
    listInstalled: (instanceId) => invoke('modrinth:listInstalled', instanceId),
    toggleMod: (instanceId, filename, enable) => invoke('modrinth:toggleMod', { instanceId, filename, enable }),
    removeMod: (instanceId, filename) => invoke('modrinth:removeMod', { instanceId, filename }),
    checkUpdates: (instanceId) => invoke('modrinth:checkUpdates', instanceId),
    updateMod: (instanceId, projectId) => invoke('modrinth:updateMod', { instanceId, projectId }),
    searchContent: (params) => invoke('modrinth:searchContent', params),
    installContent: (projectId, instanceId, projectType) => invoke('modrinth:installContent', { projectId, instanceId, projectType }),
    listInstalledContent: (instanceId, projectType) => invoke('modrinth:listInstalledContent', { instanceId, projectType }),
    removeContent: (instanceId, projectType, filename) => invoke('modrinth:removeContent', { instanceId, projectType, filename }),
    searchModpacks: (params) => invoke('modrinth:searchModpacks', params),
    getModpackVersions: (projectId) => invoke('modrinth:getModpackVersions', projectId),
    installModpackFromProject: (projectId, versionId) => invoke('modrinth:installModpackFromProject', { projectId, versionId }),
    installModpackFile: () => invoke('modrinth:installModpackFile'),
    onProgress: (cb) => ipcRenderer.on('modrinth:progress', (e, data) => cb(data)),
    onModpackProgress: (cb) => ipcRenderer.on('modpack:progress', (e, data) => cb(data))
  },
  java: {
    detect: () => invoke('java:detect'),
    browse: () => invoke('java:browse'),
    download: (majorVersion) => invoke('java:download', majorVersion),
    onProgress: (cb) => ipcRenderer.on('java:progress', (e, data) => cb(data))
  },
  settings: {
    get: () => invoke('settings:get'),
    set: (patch) => invoke('settings:set', patch)
  },
  discord: {
    onLog: (cb) => ipcRenderer.on('discord:log', (e, data) => cb(data))
  },
  servers: {
    list: () => invoke('servers:list'),
    create: (data) => invoke('servers:create', data),
    update: (serverId, patch) => invoke('servers:update', { serverId, patch }),
    delete: (serverId) => invoke('servers:delete', serverId),
    openFolder: (serverId) => invoke('servers:openFolder', serverId),
    isRunning: (serverId) => invoke('servers:isRunning', serverId),
    getProperties: (serverId) => invoke('servers:getProperties', serverId),
    setProperties: (serverId, patch) => invoke('servers:setProperties', { serverId, patch }),
    start: (serverId) => invoke('servers:start', serverId),
    stop: (serverId) => invoke('servers:stop', serverId),
    sendCommand: (serverId, command) => invoke('servers:sendCommand', { serverId, command }),
    onCreateProgress: (cb) => ipcRenderer.on('servers:createProgress', (e, data) => cb(data)),
    onLog: (cb) => ipcRenderer.on('servers:log', (e, data) => cb(data)),
    onClosed: (cb) => ipcRenderer.on('servers:closed', (e, data) => cb(data))
  },
  updater: {
    check: () => invoke('updater:check'),
    download: () => invoke('updater:download'),
    install: () => invoke('updater:install'),
    getVersion: () => invoke('updater:getVersion'),
    onStatus: (cb) => ipcRenderer.on('updater:status', (e, data) => cb(data))
  },
  worlds: {
    list: (instanceId) => invoke('worlds:list', instanceId),
    delete: (instanceId, folderName) => invoke('worlds:delete', { instanceId, folderName }),
    openFolder: (instanceId, folderName) => invoke('worlds:openFolder', { instanceId, folderName }),
    backup: (instanceId, folderName) => invoke('worlds:backup', { instanceId, folderName }),
    import: (instanceId) => invoke('worlds:import', instanceId)
  },
  background: {
    get: () => invoke('background:get'),
    choose: () => invoke('background:choose'),
    reset: () => invoke('background:reset')
  },
  window: {
    minimize: () => invoke('window:minimize'),
    maximize: () => invoke('window:maximize'),
    close: () => invoke('window:close')
  }
});
