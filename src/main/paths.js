// Tüm modüllerin AYNI .minecraft kök dizinini kullanmasını garanti eder.
// Önceden launchManager.js ve loaderManager.js farklı dizinler hesaplıyordu;
// bu da Forge/NeoForge kurulumunun launcher'ın aradığı yerden FARKLI bir yere
// yazılmasına ve "sürüm bulunamadı" hatasına yol açıyordu. Artık tek kaynak burası.
const path = require('path');
const { app } = require('electron');

function getMinecraftRootDir() {
  return path.join(app.getPath('userData'), '.minecraft');
}

function getVersionsDir() {
  return path.join(getMinecraftRootDir(), 'versions');
}

module.exports = { getMinecraftRootDir, getVersionsDir };
