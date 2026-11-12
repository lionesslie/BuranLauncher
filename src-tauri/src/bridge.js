(function () {
  'use strict';

  function tauriApi() {
    return window.__TAURI__ || {};
  }

  function normalizeError(error) {
    if (error instanceof Error) return error;
    var message = typeof error === 'string'
      ? error
      : (error && error.message) || String(error);
    return new Error(message);
  }

  function invoke(channel) {
    var args = Array.prototype.slice.call(arguments, 1);
    var api = tauriApi();
    if (!api.core || typeof api.core.invoke !== 'function') {
      return Promise.reject(new Error('Tauri IPC henüz hazır değil'));
    }
    return api.core.invoke('buran_invoke', { channel: channel, args: args })
      .catch(function (error) { throw normalizeError(error); });
  }

  function on(eventName, callback) {
    if (typeof callback !== 'function') return;
    var api = tauriApi();
    if (!api.event || typeof api.event.listen !== 'function') {
      console.error('[buran bridge] Tauri olay sistemi hazır değil: ' + eventName);
      return;
    }
    api.event.listen(eventName, function (event) {
      try {
        callback(event.payload);
      } catch (error) {
        console.error('[buran bridge] ' + eventName + ' dinleyicisi hata verdi', error);
      }
    }).catch(function (error) {
      console.error('[buran bridge] ' + eventName + ' dinlenemedi', normalizeError(error));
    });
  }

  window.buran = {
    i18n: {
      get: function () { return invoke('i18n:get'); },
      setLang: function (lang) { return invoke('i18n:setLang', lang); }
    },
    accounts: {
      list: function () { return invoke('accounts:list'); },
      getActive: function () { return invoke('accounts:getActive'); },
      addOffline: function (username) { return invoke('accounts:addOffline', username); },
      loginMicrosoft: function () { return invoke('accounts:loginMicrosoft'); },
      setActive: function (id) { return invoke('accounts:setActive', id); },
      remove: function (id) { return invoke('accounts:remove', id); }
    },
    versions: {
      list: function (type) { return invoke('versions:list', type); }
    },
    system: {
      memory: function () { return invoke('system:memory'); }
    },
    loader: {
      fabricVersions: function () { return invoke('loader:fabricVersions'); },
      quiltVersions: function () { return invoke('loader:quiltVersions'); },
      install: function (params) { return invoke('loader:install', params); },
      onProgress: function (callback) { on('loader:progress', callback); }
    },
    instances: {
      list: function () { return invoke('instances:list'); },
      create: function (data) { return invoke('instances:create', data); },
      update: function (id, patch) { return invoke('instances:update', { id: id, patch: patch }); },
      duplicate: function (id, newName) { return invoke('instances:duplicate', { id: id, newName: newName }); },
      delete: function (id) { return invoke('instances:delete', id); },
      export: function (id) { return invoke('instances:export', id); },
      import: function () { return invoke('instances:import'); },
      openFolder: function (id) { return invoke('instances:openFolder', id); }
    },
    launch: {
      start: function (instanceId) { return invoke('launch:start', instanceId); },
      kill: function (instanceId) { return invoke('launch:kill', instanceId); },
      isRunning: function (instanceId) { return invoke('launch:isRunning', instanceId); },
      onLog: function (callback) { on('launch:log', callback); },
      onStatus: function (callback) { on('launch:status', callback); },
      onClosed: function (callback) { on('launch:closed', callback); }
    },
    modrinth: {
      search: function (params) { return invoke('modrinth:search', params); },
      getProject: function (id) { return invoke('modrinth:getProject', id); },
      getVersions: function (id, opts) { return invoke('modrinth:getVersions', { id: id, opts: opts }); },
      installMod: function (projectId, instanceId, versionId) {
        return invoke('modrinth:installMod', { projectId: projectId, instanceId: instanceId, versionId: versionId });
      },
      listInstalled: function (instanceId) { return invoke('modrinth:listInstalled', instanceId); },
      toggleMod: function (instanceId, filename, enable) {
        return invoke('modrinth:toggleMod', { instanceId: instanceId, filename: filename, enable: enable });
      },
      removeMod: function (instanceId, filename) {
        return invoke('modrinth:removeMod', { instanceId: instanceId, filename: filename });
      },
      checkUpdates: function (instanceId) { return invoke('modrinth:checkUpdates', instanceId); },
      updateMod: function (instanceId, projectId) {
        return invoke('modrinth:updateMod', { instanceId: instanceId, projectId: projectId });
      },
      searchContent: function (params) { return invoke('modrinth:searchContent', params); },
      installContent: function (projectId, instanceId, projectType) {
        return invoke('modrinth:installContent', { projectId: projectId, instanceId: instanceId, projectType: projectType });
      },
      listInstalledContent: function (instanceId, projectType) {
        return invoke('modrinth:listInstalledContent', { instanceId: instanceId, projectType: projectType });
      },
      removeContent: function (instanceId, projectType, filename) {
        return invoke('modrinth:removeContent', { instanceId: instanceId, projectType: projectType, filename: filename });
      },
      searchModpacks: function (params) { return invoke('modrinth:searchModpacks', params); },
      getModpackVersions: function (projectId) { return invoke('modrinth:getModpackVersions', projectId); },
      installModpackFromProject: function (projectId, versionId) {
        return invoke('modrinth:installModpackFromProject', { projectId: projectId, versionId: versionId });
      },
      installModpackFile: function () { return invoke('modrinth:installModpackFile'); },
      onProgress: function (callback) { on('modrinth:progress', callback); },
      onModpackProgress: function (callback) { on('modpack:progress', callback); }
    },
    java: {
      detect: function () { return invoke('java:detect'); },
      version: function () { return invoke('java:version'); },
      browse: function () { return invoke('java:browse'); },
      download: function (majorVersion) { return invoke('java:download', majorVersion); },
      onProgress: function (callback) { on('java:progress', callback); }
    },
    settings: {
      get: function () { return invoke('settings:get'); },
      set: function (patch) { return invoke('settings:set', patch); }
    },
    discord: {
      onLog: function (callback) { on('discord:log', callback); }
    },
    servers: {
      list: function () { return invoke('servers:list'); },
      create: function (data) { return invoke('servers:create', data); },
      update: function (serverId, patch) { return invoke('servers:update', { serverId: serverId, patch: patch }); },
      delete: function (serverId) { return invoke('servers:delete', serverId); },
      openFolder: function (serverId) { return invoke('servers:openFolder', serverId); },
      isRunning: function (serverId) { return invoke('servers:isRunning', serverId); },
      getProperties: function (serverId) { return invoke('servers:getProperties', serverId); },
      setProperties: function (serverId, patch) {
        return invoke('servers:setProperties', { serverId: serverId, patch: patch });
      },
      start: function (serverId) { return invoke('servers:start', serverId); },
      stop: function (serverId) { return invoke('servers:stop', serverId); },
      sendCommand: function (serverId, command) {
        return invoke('servers:sendCommand', { serverId: serverId, command: command });
      },
      onCreateProgress: function (callback) { on('servers:createProgress', callback); },
      onLog: function (callback) { on('servers:log', callback); },
      onClosed: function (callback) { on('servers:closed', callback); }
    },
    updater: {
      check: function () { return invoke('updater:check'); },
      download: function () { return invoke('updater:download'); },
      install: function () { return invoke('updater:install'); },
      getVersion: function () { return invoke('updater:getVersion'); },
      onStatus: function (callback) { on('updater:status', callback); }
    },
    app: {
      status: function () { return invoke('app:status'); },
      openPath: function (path) { return invoke('app:openPath', path); },
      openUrl: function (url) { return invoke('app:openUrl', url); }
    },
    news: {
      list: function () { return invoke('news:list'); }
    },
    worlds: {
      list: function (instanceId) { return invoke('worlds:list', instanceId); },
      delete: function (instanceId, folderName) {
        return invoke('worlds:delete', { instanceId: instanceId, folderName: folderName });
      },
      openFolder: function (instanceId, folderName) {
        return invoke('worlds:openFolder', { instanceId: instanceId, folderName: folderName });
      },
      backup: function (instanceId, folderName) {
        return invoke('worlds:backup', { instanceId: instanceId, folderName: folderName });
      },
      import: function (instanceId) { return invoke('worlds:import', instanceId); }
    },
    background: {
      get: function () { return invoke('background:get'); },
      choose: function () { return invoke('background:choose'); },
      reset: function () { return invoke('background:reset'); }
    },
    window: {
      minimize: function () { return invoke('window:minimize'); },
      maximize: function () { return invoke('window:maximize'); },
      close: function () { return invoke('window:close'); }
    }
  };

  function installTitlebarDrag() {
    function currentWindow() {
      var api = tauriApi();
      try {
        return api.window && typeof api.window.getCurrentWindow === 'function'
          ? api.window.getCurrentWindow()
          : null;
      } catch (error) {
        return null;
      }
    }

    document.addEventListener('mousedown', function (event) {
      if (event.button !== 0) return;
      var target = event.target;
      if (!target || !target.closest) return;
      var titlebar = target.closest('.titlebar');
      if (!titlebar) return;
      if (target.closest('.titlebar-controls, .titlebar-account, button, input, select, textarea, a, [data-no-drag]')) {
        return;
      }
      var tauriWindow = currentWindow();
      if (tauriWindow && typeof tauriWindow.startDragging === 'function') {
        event.preventDefault();
        tauriWindow.startDragging().catch(function (error) {
          console.error('[buran bridge] Pencere sürüklenemedi', normalizeError(error));
        });
      }
    }, true);

    document.addEventListener('dblclick', function (event) {
      var target = event.target;
      if (!target || !target.closest) return;
      if (!target.closest('.titlebar')) return;
      if (target.closest('.titlebar-controls, .titlebar-account, button, input, select, textarea, a, [data-no-drag]')) {
        return;
      }
      invoke('window:maximize');
    }, true);
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', installTitlebarDrag, { once: true });
  } else {
    installTitlebarDrag();
  }
})();