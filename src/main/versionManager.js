// Mojang version manifest API entegrasyonu
const fetch = require('node-fetch');
const log = require('electron-log');

const MANIFEST_URL = 'https://launchermeta.mojang.com/mc/game/version_manifest_v2.json';

let cachedManifest = null;
let cachedAt = 0;
const CACHE_TTL_MS = 10 * 60 * 1000; // 10 dakika

async function fetchVersionManifest(force = false) {
  const now = Date.now();
  if (!force && cachedManifest && now - cachedAt < CACHE_TTL_MS) {
    return cachedManifest;
  }
  try {
    const res = await fetch(MANIFEST_URL);
    if (!res.ok) throw new Error(`Manifest alınamadı: HTTP ${res.status}`);
    const data = await res.json();
    cachedManifest = data;
    cachedAt = now;
    return data;
  } catch (err) {
    log.error('Sürüm manifesti indirilemedi', err);
    if (cachedManifest) return cachedManifest; // eski cache varsa onu döndür
    throw err;
  }
}

/**
 * type: 'release' | 'snapshot' | 'old_beta' | 'old_alpha' | 'all'
 */
async function listVersions(type = 'release') {
  const manifest = await fetchVersionManifest();
  if (type === 'all') return manifest.versions;
  return manifest.versions.filter(v => v.type === type);
}

async function getVersionDetail(versionId) {
  const manifest = await fetchVersionManifest();
  const entry = manifest.versions.find(v => v.id === versionId);
  if (!entry) throw new Error('Sürüm bulunamadı: ' + versionId);
  const res = await fetch(entry.url);
  if (!res.ok) throw new Error(`Sürüm detayı alınamadı: HTTP ${res.status}`);
  return res.json();
}

module.exports = { fetchVersionManifest, listVersions, getVersionDetail };
