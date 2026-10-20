// Instance (oyun profili) yönetimi - her instance tam izole klasör yapısına sahiptir
const fs = require('fs');
const path = require('path');
const { app } = require('electron');
const { v4: uuidv4 } = require('uuid');
const AdmZip = require('adm-zip');
const store = require('./store');
const log = require('electron-log');

function getInstancesRootDir() {
  const dir = path.join(app.getPath('userData'), 'instances');
  fs.mkdirSync(dir, { recursive: true });
  return dir;
}

function getInstanceDir(instanceId) {
  return path.join(getInstancesRootDir(), instanceId);
}

const SUBFOLDERS = ['mods', 'resourcepacks', 'shaderpacks', 'saves', 'config'];

function ensureInstanceFolders(instanceId) {
  const base = getInstanceDir(instanceId);
  for (const sub of SUBFOLDERS) {
    fs.mkdirSync(path.join(base, sub), { recursive: true });
  }
  return base;
}

function listInstances() {
  return store.get('instances');
}

function getInstance(id) {
  const instance = store.get('instances').find(i => i.id === id);
  if (!instance) throw new Error('Profil bulunamadı: ' + id);
  return instance;
}

function createInstance({ name, icon, version, versionType, loader, loaderVersion }) {
  if (!name || !name.trim()) throw new Error('Profil adı boş olamaz');
  if (!version) throw new Error('Minecraft sürümü seçilmeli');

  const instance = {
    id: uuidv4(),
    name: name.trim(),
    // Profiller için şu an tek sabit ikon kullanılıyor (Minecraft çim bloğu).
    // Alan ileride çoklu ikon desteği eklenirse diye korunuyor.
    icon: 'grass',
    version,
    versionType: versionType || 'release',
    loader: loader || 'vanilla', // vanilla | forge | fabric | quilt | neoforge
    loaderVersion: loaderVersion || null,
    createdAt: Date.now(),
    lastPlayed: null,
    totalPlaytimeMs: 0
  };

  ensureInstanceFolders(instance.id);

  const instances = store.get('instances');
  instances.push(instance);
  store.set('instances', instances);

  log.info(`Yeni instance oluşturuldu: ${instance.name} (${instance.id})`);
  return instance;
}

function updateInstance(id, patch) {
  const instances = store.get('instances');
  const idx = instances.findIndex(i => i.id === id);
  if (idx === -1) throw new Error('Profil bulunamadı: ' + id);
  instances[idx] = { ...instances[idx], ...patch, id: instances[idx].id };
  store.set('instances', instances);
  return instances[idx];
}

function duplicateInstance(id, newName) {
  const original = getInstance(id);
  const copy = createInstance({
    name: newName || original.name + ' (Kopya)',
    icon: original.icon,
    version: original.version,
    versionType: original.versionType,
    loader: original.loader,
    loaderVersion: original.loaderVersion
  });

  // Dosya içeriğini kopyala
  const srcDir = getInstanceDir(original.id);
  const destDir = getInstanceDir(copy.id);
  fs.cpSync(srcDir, destDir, { recursive: true });
  return copy;
}

function deleteInstance(id) {
  const instances = store.get('instances').filter(i => i.id !== id);
  store.set('instances', instances);
  const dir = getInstanceDir(id);
  if (fs.existsSync(dir)) {
    fs.rmSync(dir, { recursive: true, force: true });
  }
  log.info('Instance silindi: ' + id);
}

function recordPlaySession(id, durationMs) {
  const instances = store.get('instances');
  const idx = instances.findIndex(i => i.id === id);
  if (idx === -1) return;
  instances[idx].lastPlayed = Date.now();
  instances[idx].totalPlaytimeMs = (instances[idx].totalPlaytimeMs || 0) + durationMs;
  store.set('instances', instances);
}

function exportInstance(id, destZipPath) {
  const instance = getInstance(id);
  const dir = getInstanceDir(id);
  const zip = new AdmZip();
  zip.addLocalFolder(dir, 'data');
  zip.addFile('instance.json', Buffer.from(JSON.stringify(instance, null, 2), 'utf8'));
  zip.writeZip(destZipPath);
  return destZipPath;
}

function importInstance(zipPath) {
  const zip = new AdmZip(zipPath);
  const metaEntry = zip.getEntry('instance.json');
  if (!metaEntry) throw new Error('Geçersiz profil arşivi: instance.json bulunamadı');
  const meta = JSON.parse(zip.readAsText(metaEntry));

  const newInstance = createInstance({
    name: meta.name + ' (İçe Aktarıldı)',
    icon: meta.icon,
    version: meta.version,
    versionType: meta.versionType,
    loader: meta.loader,
    loaderVersion: meta.loaderVersion
  });

  const destDir = getInstanceDir(newInstance.id);
  zip.getEntries().forEach(entry => {
    if (entry.entryName.startsWith('data/') && !entry.isDirectory) {
      const relPath = entry.entryName.substring('data/'.length);
      const targetPath = path.join(destDir, relPath);
      fs.mkdirSync(path.dirname(targetPath), { recursive: true });
      fs.writeFileSync(targetPath, entry.getData());
    }
  });

  return newInstance;
}

module.exports = {
  getInstancesRootDir,
  getInstanceDir,
  ensureInstanceFolders,
  listInstances,
  getInstance,
  createInstance,
  updateInstance,
  duplicateInstance,
  deleteInstance,
  recordPlaySession,
  exportInstance,
  importInstance
};
