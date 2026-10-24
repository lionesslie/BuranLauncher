// SADECE Modrinth API kullanılır (docs.modrinth.com) - başka hiçbir mod kaynağı entegre edilmez
const fs = require('fs');
const path = require('path');
const crypto = require('crypto');
const fetch = require('node-fetch');
const extractZip = require('extract-zip');
const log = require('electron-log');
const instanceManager = require('./instanceManager');

const API_BASE = 'https://api.modrinth.com/v2';
const USER_AGENT = 'BuranLauncher/1.0.0 (iletisim: destek@buranlauncher.app)';

async function apiGet(endpoint) {
  const res = await fetch(API_BASE + endpoint, { headers: { 'User-Agent': USER_AGENT } });
  if (!res.ok) throw new Error(`Modrinth API hatası (${endpoint}): HTTP ${res.status}`);
  return res.json();
}

/**
 * projectType: 'mod' | 'modpack'
 */
async function search({ query = '', projectType = 'mod', loader = null, gameVersion = null, sort = 'relevance', offset = 0, limit = 20 }) {
  const facets = [[`project_type:${projectType}`]];
  if (loader) facets.push([`categories:${loader}`]);
  if (gameVersion) facets.push([`versions:${gameVersion}`]);

  const params = new URLSearchParams({
    query,
    facets: JSON.stringify(facets),
    index: sort, // relevance | downloads | newest | updated
    offset: String(offset),
    limit: String(limit)
  });

  return apiGet('/search?' + params.toString());
}

async function getProject(idOrSlug) {
  return apiGet(`/project/${idOrSlug}`);
}

async function getProjectVersions(idOrSlug, { loader = null, gameVersion = null } = {}) {
  const params = new URLSearchParams();
  if (loader) params.set('loaders', JSON.stringify([loader]));
  if (gameVersion) params.set('game_versions', JSON.stringify([gameVersion]));
  const qs = params.toString() ? '?' + params.toString() : '';
  return apiGet(`/project/${idOrSlug}/version${qs}`);
}

// --- Kurulu mod meta-verisi (güncelleme kontrolü için) ---
// mods/ klasöründeki .jar dosyaları tek başına hangi Modrinth projesinden/
// sürümünden geldiğini bilmez. Bu yüzden her instance'ın kökünde ayrı bir
// mods-meta.json dosyasında {projectId -> {projectId, title, filename, versionId, versionNumber}}
// eşlemesi tutulur.
function getMetaPath(instanceId, projectType = 'mod') {
  const filename = projectType === 'mod' ? 'mods-meta.json' : `${projectType}-meta.json`;
  return path.join(instanceManager.getInstanceDir(instanceId), filename);
}

function loadModsMeta(instanceId, projectType = 'mod') {
  const metaPath = getMetaPath(instanceId, projectType);
  if (!fs.existsSync(metaPath)) return {};
  try {
    return JSON.parse(fs.readFileSync(metaPath, 'utf8'));
  } catch (err) {
    log.warn(`${metaPath} okunamadı, sıfırdan başlatılıyor`, err);
    return {};
  }
}

function saveModsMeta(instanceId, meta, projectType = 'mod') {
  fs.writeFileSync(getMetaPath(instanceId, projectType), JSON.stringify(meta, null, 2));
}

// Modrinth projectType -> instance içindeki hedef klasör eşlemesi
const CONTENT_FOLDER_BY_TYPE = {
  mod: 'mods',
  resourcepack: 'resourcepacks',
  shader: 'shaderpacks'
};

function verifySha1(filePath, expectedSha1) {
  const buf = fs.readFileSync(filePath);
  const actual = crypto.createHash('sha1').update(buf).digest('hex');
  return actual.toLowerCase() === expectedSha1.toLowerCase();
}

async function downloadFile(url, destPath, expectedSha1) {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`Dosya indirilemedi: HTTP ${res.status} - ${url}`);
  fs.mkdirSync(path.dirname(destPath), { recursive: true });
  const buffer = await res.buffer();
  fs.writeFileSync(destPath, buffer);
  if (expectedSha1 && !verifySha1(destPath, expectedSha1)) {
    fs.unlinkSync(destPath);
    throw new Error('SHA1 doğrulaması başarısız: ' + path.basename(destPath));
  }
  return destPath;
}

/**
 * Bir modu, gerekli (required) bağımlılıklarıyla birlikte verilen instance'a kurar.
 * onProgress: ({stage, current, total, name}) => void
 */
async function installModToInstance(projectIdOrSlug, instanceId, { versionId = null, onProgress = () => {} } = {}) {
  const instance = instanceManager.getInstance(instanceId);
  const modsDir = path.join(instanceManager.getInstanceDir(instanceId), 'mods');
  fs.mkdirSync(modsDir, { recursive: true });

  const loaderParam = instance.loader !== 'vanilla' ? instance.loader : null;

  async function resolveVersion(idOrSlug, verId) {
    if (verId) return apiGet(`/version/${verId}`);
    const versions = await getProjectVersions(idOrSlug, { loader: loaderParam, gameVersion: instance.version });
    if (!versions.length) throw new Error(`"${idOrSlug}" için ${instance.version} / ${instance.loader} ile uyumlu sürüm bulunamadı`);
    return versions[0]; // en güncel uyumlu sürüm
  }

  const installedSlugs = new Set();
  const meta = loadModsMeta(instanceId);

  async function installOne(idOrSlug, verId) {
    if (installedSlugs.has(idOrSlug)) return;
    installedSlugs.add(idOrSlug);

    onProgress({ stage: 'resolving', name: idOrSlug });
    const version = await resolveVersion(idOrSlug, verId);

    // Aynı profilde bu mod zaten kuruluysa (meta-veride kayıtlıysa) tekrar indirip
    // kurmuyoruz - gereksiz ağ trafiği ve dosya üzerine yazma engellenir.
    const already = meta[version.project_id];
    if (already && fs.existsSync(path.join(modsDir, already.filename))) {
      log.info(`Zaten kurulu, atlanıyor: ${already.filename}`);
      return;
    }

    const primaryFile = version.files.find(f => f.primary) || version.files[0];
    if (!primaryFile) throw new Error('İndirilecek dosya bulunamadı: ' + idOrSlug);

    const destPath = path.join(modsDir, primaryFile.filename);
    onProgress({ stage: 'downloading', name: primaryFile.filename });
    await downloadFile(primaryFile.url, destPath, primaryFile.hashes && primaryFile.hashes.sha1);

    // Güncelleme kontrolü için kurulum bilgisini meta-veriye kaydet
    meta[version.project_id] = {
      projectId: version.project_id,
      title: idOrSlug,
      filename: primaryFile.filename,
      versionId: version.id,
      versionNumber: version.version_number,
      installedAt: Date.now()
    };

    // Gerekli (required) bağımlılıkları da otomatik kur
    for (const dep of version.dependencies || []) {
      if (dep.dependency_type === 'required' && dep.project_id) {
        await installOne(dep.project_id, dep.version_id || null);
      }
    }
  }

  await installOne(projectIdOrSlug, versionId);
  saveModsMeta(instanceId, meta);
  onProgress({ stage: 'done' });
  return true;
}

/**
 * Kaynak paketi (resourcepack) veya shader kurulumu için genel fonksiyon.
 * Mod kurulumundan farkı: loader filtrelemesi yapılmaz, bağımlılık zinciri
 * takip edilmez (Modrinth'te kaynak paketi/shader'ların moda benzer zorunlu
 * bağımlılıkları olmaz) ve dosyalar ilgili klasöre (resourcepacks/ ya da
 * shaderpacks/) yerleştirilir. Sürüm seçimi HER ZAMAN otomatiktir: Modrinth
 * API'si sürümleri en yeniden eskiye sıraladığı için ilk sonuç (en son
 * yayınlanan, instance'ın Minecraft sürümüyle uyumlu sürüm) seçilir.
 */
async function installContentToInstance(projectIdOrSlug, instanceId, { versionId = null, projectType, onProgress = () => {} } = {}) {
  const folderName = CONTENT_FOLDER_BY_TYPE[projectType];
  if (!folderName) throw new Error('Bilinmeyen içerik türü: ' + projectType);

  const instance = instanceManager.getInstance(instanceId);
  const targetDir = path.join(instanceManager.getInstanceDir(instanceId), folderName);
  fs.mkdirSync(targetDir, { recursive: true });

  const meta = loadModsMeta(instanceId, projectType);

  onProgress({ stage: 'resolving', name: projectIdOrSlug });
  let version;
  if (versionId) {
    version = await apiGet(`/version/${versionId}`);
  } else {
    // Otomatik sürüm seçimi: sadece Minecraft sürümüne göre filtrelenir,
    // sonuçların İLKİ (en son yayınlanan uyumlu sürüm) otomatik seçilir.
    const versions = await getProjectVersions(projectIdOrSlug, { gameVersion: instance.version });
    if (!versions.length) throw new Error(`"${projectIdOrSlug}" için ${instance.version} ile uyumlu sürüm bulunamadı`);
    version = versions[0];
  }

  const already = meta[version.project_id];
  if (already && fs.existsSync(path.join(targetDir, already.filename))) {
    log.info(`Zaten kurulu, atlanıyor: ${already.filename}`);
    onProgress({ stage: 'done' });
    return true;
  }

  const primaryFile = version.files.find(f => f.primary) || version.files[0];
  if (!primaryFile) throw new Error('İndirilecek dosya bulunamadı: ' + projectIdOrSlug);

  const destPath = path.join(targetDir, primaryFile.filename);
  onProgress({ stage: 'downloading', name: primaryFile.filename });
  await downloadFile(primaryFile.url, destPath, primaryFile.hashes && primaryFile.hashes.sha1);

  meta[version.project_id] = {
    projectId: version.project_id,
    title: projectIdOrSlug,
    filename: primaryFile.filename,
    versionId: version.id,
    versionNumber: version.version_number,
    installedAt: Date.now()
  };
  saveModsMeta(instanceId, meta, projectType);

  onProgress({ stage: 'done' });
  return true;
}

function listInstalledContent(instanceId, projectType) {
  const folderName = CONTENT_FOLDER_BY_TYPE[projectType];
  if (!folderName) throw new Error('Bilinmeyen içerik türü: ' + projectType);

  const targetDir = path.join(instanceManager.getInstanceDir(instanceId), folderName);
  const meta = loadModsMeta(instanceId, projectType);
  const metaByFilename = {};
  for (const entry of Object.values(meta)) metaByFilename[entry.filename] = entry;

  if (!fs.existsSync(targetDir)) return [];
  const exts = projectType === 'resourcepack' ? ['.zip'] : ['.zip'];
  return fs.readdirSync(targetDir)
    .filter(f => exts.some(ext => f.endsWith(ext)))
    .map(f => {
      const matched = metaByFilename[f];
      return {
        filename: f,
        projectId: matched ? matched.projectId : null,
        title: matched ? matched.title : f.replace(/\.zip$/, ''),
        versionNumber: matched ? matched.versionNumber : null
      };
    });
}

function removeContent(instanceId, projectType, filename) {
  const folderName = CONTENT_FOLDER_BY_TYPE[projectType];
  if (!folderName) throw new Error('Bilinmeyen içerik türü: ' + projectType);

  const targetDir = path.join(instanceManager.getInstanceDir(instanceId), folderName);
  fs.unlinkSync(path.join(targetDir, filename));

  const meta = loadModsMeta(instanceId, projectType);
  const projectId = Object.keys(meta).find(pid => meta[pid].filename === filename);
  if (projectId) {
    delete meta[projectId];
    saveModsMeta(instanceId, meta, projectType);
  }
}

/**
 * Kurulu modların Modrinth üzerinde daha yeni bir sürümü var mı kontrol eder.
 * Döndürülen her öğe: {projectId, title, filename, currentVersionNumber, latestVersionId, latestVersionNumber}
 */
async function checkForUpdates(instanceId) {
  const instance = instanceManager.getInstance(instanceId);
  const meta = loadModsMeta(instanceId);
  const loaderParam = instance.loader !== 'vanilla' ? instance.loader : null;
  const updates = [];

  for (const entry of Object.values(meta)) {
    try {
      const versions = await getProjectVersions(entry.projectId, { loader: loaderParam, gameVersion: instance.version });
      if (!versions.length) continue;
      const latest = versions[0];
      if (latest.id !== entry.versionId) {
        updates.push({
          projectId: entry.projectId,
          title: entry.title,
          filename: entry.filename,
          currentVersionNumber: entry.versionNumber,
          latestVersionId: latest.id,
          latestVersionNumber: latest.version_number
        });
      }
    } catch (err) {
      log.warn(`Güncelleme kontrolü başarısız (${entry.projectId})`, err.message);
    }
  }
  return updates;
}

/**
 * Bir modu en güncel uyumlu sürüme günceller: eski dosyayı kaldırır, yenisini kurar.
 */
async function updateMod(instanceId, projectId, { onProgress = () => {} } = {}) {
  const meta = loadModsMeta(instanceId);
  const entry = meta[projectId];
  if (entry) {
    const modsDir = path.join(instanceManager.getInstanceDir(instanceId), 'mods');
    const oldPath = path.join(modsDir, entry.filename);
    const oldPathDisabled = oldPath + '.disabled';
    if (fs.existsSync(oldPath)) fs.unlinkSync(oldPath);
    else if (fs.existsSync(oldPathDisabled)) fs.unlinkSync(oldPathDisabled);
  }
  return installModToInstance(projectId, instanceId, { onProgress });
}

function listInstalledMods(instanceId) {
  const modsDir = path.join(instanceManager.getInstanceDir(instanceId), 'mods');
  const meta = loadModsMeta(instanceId);
  const metaByFilename = {};
  for (const entry of Object.values(meta)) {
    metaByFilename[entry.filename] = entry;
  }

  if (!fs.existsSync(modsDir)) return [];
  return fs.readdirSync(modsDir)
    .filter(f => f.endsWith('.jar') || f.endsWith('.jar.disabled'))
    .map(f => {
      const baseFilename = f.replace(/\.disabled$/, '');
      const matched = metaByFilename[baseFilename] || metaByFilename[f];
      return {
        filename: f,
        enabled: !f.endsWith('.disabled'),
        path: path.join(modsDir, f),
        projectId: matched ? matched.projectId : null,
        title: matched ? matched.title : baseFilename.replace(/\.jar$/, ''),
        versionNumber: matched ? matched.versionNumber : null
      };
    });
}

function toggleMod(instanceId, filename, enable) {
  const modsDir = path.join(instanceManager.getInstanceDir(instanceId), 'mods');
  const current = path.join(modsDir, filename);
  const target = enable
    ? path.join(modsDir, filename.replace(/\.disabled$/, ''))
    : path.join(modsDir, filename.endsWith('.disabled') ? filename : filename + '.disabled');
  fs.renameSync(current, target);
  return target;
}

function removeMod(instanceId, filename) {
  const modsDir = path.join(instanceManager.getInstanceDir(instanceId), 'mods');
  fs.unlinkSync(path.join(modsDir, filename));

  // Meta-veriden de temizle (varsa)
  const baseFilename = filename.replace(/\.disabled$/, '');
  const meta = loadModsMeta(instanceId);
  const projectId = Object.keys(meta).find(pid => meta[pid].filename === baseFilename);
  if (projectId) {
    delete meta[projectId];
    saveModsMeta(instanceId, meta);
  }
}

/**
 * .mrpack (Modrinth modpack) DOSYASINDAN otomatik instance oluşturur
 * (elle seçilen yerel dosya için).
 */
async function installModpackFromFile(mrpackPath, { onProgress = () => {} } = {}) {
  return installModpackFromMrpackPath(mrpackPath, { onProgress });
}

/**
 * Modrinth'teki bir modpack PROJESİNDEN, belirli bir sürümü (verilmezse en
 * güncel sürümü) indirip otomatik instance oluşturur. "Modpack'ler" sayfasında
 * arama sonucundaki "Kur" butonu bunu kullanır.
 */
async function installModpackFromProject(projectIdOrSlug, versionId = null, { onProgress = () => {} } = {}) {
  onProgress({ stage: 'manifest' });

  let version;
  if (versionId) {
    version = await apiGet(`/version/${versionId}`);
  } else {
    const versions = await getProjectVersions(projectIdOrSlug);
    if (!versions.length) throw new Error('Bu modpack için hiç sürüm bulunamadı');
    version = versions[0]; // en güncel sürüm
  }

  const mrpackFile = version.files.find(f => f.filename.endsWith('.mrpack')) || version.files[0];
  if (!mrpackFile) throw new Error('.mrpack dosyası bulunamadı');

  const tmpDir = path.join(require('electron').app.getPath('temp'), 'buran-launcher-modpacks');
  fs.mkdirSync(tmpDir, { recursive: true });
  const tmpPath = path.join(tmpDir, `${Date.now()}-${mrpackFile.filename}`);

  onProgress({ stage: 'downloadingMods', current: 0, total: 1, name: mrpackFile.filename });
  await downloadFile(mrpackFile.url, tmpPath, mrpackFile.hashes && mrpackFile.hashes.sha1);

  try {
    return await installModpackFromMrpackPath(tmpPath, { onProgress });
  } finally {
    try { fs.unlinkSync(tmpPath); } catch (err) { /* geçici dosya, sorun değil */ }
  }
}

async function installModpackFromMrpackPath(mrpackPath, { onProgress = () => {} } = {}) {
  onProgress({ stage: 'manifest' });
  const tmpExtractDir = mrpackPath + '_extracted';
  await extractZip(mrpackPath, { dir: tmpExtractDir });

  const indexPath = path.join(tmpExtractDir, 'modrinth.index.json');
  if (!fs.existsSync(indexPath)) throw new Error('modrinth.index.json bulunamadı - geçersiz .mrpack dosyası');
  const index = JSON.parse(fs.readFileSync(indexPath, 'utf8'));

  const loaderEntry = Object.entries(index.dependencies || {}).find(([k]) => k !== 'minecraft');
  const loader = loaderEntry ? loaderEntry[0].replace('-loader', '') : 'vanilla';
  const loaderVersion = loaderEntry ? loaderEntry[1] : null;
  const mcVersion = index.dependencies && index.dependencies.minecraft;

  const instance = instanceManager.createInstance({
    name: index.name || 'Modpack',
    icon: 'package',
    version: mcVersion,
    versionType: 'release',
    loader,
    loaderVersion
  });

  const instanceDir = instanceManager.getInstanceDir(instance.id);

  const files = index.files || [];
  let current = 0;
  for (const file of files) {
    current++;
    onProgress({ stage: 'downloadingMods', current, total: files.length, name: file.path });
    const destPath = path.join(instanceDir, file.path);
    const url = file.downloads && file.downloads[0];
    if (!url) continue;
    const sha1 = file.hashes && file.hashes.sha1;
    await downloadFile(url, destPath, sha1);
  }

  // overrides/ klasörünü instance köküne kopyala (config, resourcepack vb.)
  const overridesDir = path.join(tmpExtractDir, 'overrides');
  if (fs.existsSync(overridesDir)) {
    fs.cpSync(overridesDir, instanceDir, { recursive: true });
  }

  fs.rmSync(tmpExtractDir, { recursive: true, force: true });
  onProgress({ stage: 'done' });
  return instance;
}

module.exports = {
  search,
  getProject,
  getProjectVersions,
  installModToInstance,
  listInstalledMods,
  toggleMod,
  removeMod,
  checkForUpdates,
  updateMod,
  installModpackFromFile,
  installModpackFromProject,
  installContentToInstance,
  listInstalledContent,
  removeContent
};
