// Dünya (save) yönetimi: her instance'ın saves/ klasöründeki dünyaları
// listeler, yedekler (zip), siler ve zip'ten içe aktarır.
const fs = require('fs');
const path = require('path');
const AdmZip = require('adm-zip');
const { shell } = require('electron');
const instanceManager = require('./instanceManager');

function getSavesDir(instanceId) {
  return path.join(instanceManager.getInstanceDir(instanceId), 'saves');
}

function getFolderSize(dirPath) {
  let total = 0;
  function walk(p) {
    let entries;
    try { entries = fs.readdirSync(p, { withFileTypes: true }); } catch (err) { return; }
    for (const entry of entries) {
      const full = path.join(p, entry.name);
      if (entry.isDirectory()) walk(full);
      else {
        try { total += fs.statSync(full).size; } catch (err) { /* geçici dosya silinmiş olabilir, yoksay */ }
      }
    }
  }
  walk(dirPath);
  return total;
}

/**
 * Not: level.dat (NBT/gzip formatı) ayrıştırılmıyor; bu yüzden dünyanın
 * oyun içindeki "görünen adı" değil, klasör adı gösterilir. Çoğu durumda
 * ikisi aynıdır (Minecraft varsayılan olarak klasör adını dünya adından türetir).
 */
function listWorlds(instanceId) {
  const savesDir = getSavesDir(instanceId);
  if (!fs.existsSync(savesDir)) return [];

  return fs.readdirSync(savesDir, { withFileTypes: true })
    .filter(e => e.isDirectory())
    .map(e => {
      const worldDir = path.join(savesDir, e.name);
      const iconPath = path.join(worldDir, 'icon.png');
      let iconDataUrl = null;
      if (fs.existsSync(iconPath)) {
        try {
          iconDataUrl = 'data:image/png;base64,' + fs.readFileSync(iconPath).toString('base64');
        } catch (err) { /* simge okunamazsa sorun değil, varsayılan ikon kullanılır */ }
      }
      let lastModified = null;
      try { lastModified = fs.statSync(worldDir).mtimeMs; } catch (err) { /* yoksay */ }

      return {
        folderName: e.name,
        name: e.name,
        icon: iconDataUrl,
        sizeBytes: getFolderSize(worldDir),
        lastModified
      };
    })
    .sort((a, b) => (b.lastModified || 0) - (a.lastModified || 0));
}

function deleteWorld(instanceId, folderName) {
  const target = path.join(getSavesDir(instanceId), folderName);
  if (!fs.existsSync(target)) throw new Error('Dünya bulunamadı');
  fs.rmSync(target, { recursive: true, force: true });
}

function openWorldFolder(instanceId, folderName) {
  shell.openPath(path.join(getSavesDir(instanceId), folderName));
}

function backupWorld(instanceId, folderName, destZipPath) {
  const worldDir = path.join(getSavesDir(instanceId), folderName);
  if (!fs.existsSync(worldDir)) throw new Error('Dünya bulunamadı');
  const zip = new AdmZip();
  zip.addLocalFolder(worldDir, folderName);
  zip.writeZip(destZipPath);
  return destZipPath;
}

/**
 * Bir .zip arşivinden dünyayı saves/ klasörüne içe aktarır. Arşivin kökünde
 * tek bir klasör varsa (yaygın "dünya klasörünü zip'le" durumu) o klasör adı
 * kullanılır; isim çakışırsa "(1)", "(2)" eklenir.
 */
function importWorld(instanceId, zipPath) {
  const savesDir = getSavesDir(instanceId);
  fs.mkdirSync(savesDir, { recursive: true });
  const zip = new AdmZip(zipPath);
  const entries = zip.getEntries();

  const rootDirs = new Set();
  entries.forEach(entry => {
    const parts = entry.entryName.split('/');
    if (parts.length > 1 && parts[0]) rootDirs.add(parts[0]);
  });

  const baseName = rootDirs.size === 1 ? [...rootDirs][0] : path.basename(zipPath, path.extname(zipPath));
  let finalName = baseName;
  let counter = 1;
  while (fs.existsSync(path.join(savesDir, finalName))) {
    finalName = `${baseName} (${counter++})`;
  }

  const destDir = path.join(savesDir, finalName);
  fs.mkdirSync(destDir, { recursive: true });

  entries.forEach(entry => {
    if (entry.isDirectory) return;
    const parts = entry.entryName.split('/');
    const relPath = rootDirs.size === 1 ? parts.slice(1).join('/') : entry.entryName;
    if (!relPath) return;
    const targetPath = path.join(destDir, relPath);
    fs.mkdirSync(path.dirname(targetPath), { recursive: true });
    fs.writeFileSync(targetPath, entry.getData());
  });

  return finalName;
}

module.exports = { listWorlds, deleteWorld, openWorldFolder, backupWorld, importWorld };
