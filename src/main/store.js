// Merkezi yapılandırma deposu - electron-store kullanır
const Store = require('electron-store');

const store = new Store({
  name: 'buran-launcher-config',
  defaults: {
    language: null, // null ise OS diline göre otomatik seçilir
    settings: {
      ramMin: 1024,
      ramMax: 4096,
      javaPath: '',
      extraArgs: '',
      launchBehavior: 'minimize', // keepOpen | minimize | close
      startOnBoot: false,
      minimizeToTray: true
    },
    accounts: [], // {id, type: 'offline'|'microsoft', username, uuid, encryptedToken?}
    activeAccountId: null,
    instances: [] // {id, name, icon, version, versionType, loader, loaderVersion, createdAt, lastPlayed, totalPlaytimeMs}
  }
});

module.exports = store;
