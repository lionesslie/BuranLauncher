// Mod yükleyicileri (Forge, Fabric, Quilt, NeoForge) için sürüm listesi ve kurulum
const fs = require('fs');
const path = require('path');
const fetch = require('node-fetch');
const { spawn } = require('child_process');
const log = require('electron-log');
const { getMinecraftRootDir, getVersionsDir } = require('./paths');

const FABRIC_META = 'https://meta.fabricmc.net/v2';
const QUILT_META = 'https://meta.quiltmc.org/v3';

function getVersionsRootDir() {
  // ÖNEMLİ: launchManager.js'nin kullandığı .minecraft/versions ile AYNI dizin
  // olmalı, aksi halde kurulan yükleyici launcher tarafından bulunamaz.
  const dir = getVersionsDir();
  fs.mkdirSync(dir, { recursive: true });
  return dir;
}

async function listFabricLoaderVersions() {
  const res = await fetch(`${FABRIC_META}/versions/loader`);
  if (!res.ok) throw new Error('Fabric yükleyici sürümleri alınamadı');
  return res.json();
}

async function listQuiltLoaderVersions() {
  const res = await fetch(`${QUILT_META}/versions/loader`);
  if (!res.ok) throw new Error('Quilt yükleyici sürümleri alınamadı');
  return res.json();
}

/**
 * Fabric için minecraft-launcher-core'un kullanabileceği "custom" sürüm json'ını
 * ve ilgili kütüphaneleri indirir, mclc'nin version.custom alanında kullanılacak
 * profil adını döndürür.
 */
async function installFabric(mcVersion, loaderVersion, onProgress = () => {}) {
  onProgress({ stage: 'fetchingProfile' });
  const res = await fetch(`${FABRIC_META}/versions/loader/${mcVersion}/${loaderVersion}/profile/json`);
  if (!res.ok) throw new Error('Fabric profili alınamadı (Minecraft/yükleyici sürüm uyumunu kontrol edin)');
  const profile = await res.json();

  const versionsDir = getVersionsRootDir();
  const profileId = profile.id; // örn: fabric-loader-0.15.x-1.20.x
  const profileDir = path.join(versionsDir, profileId);
  fs.mkdirSync(profileDir, { recursive: true });
  fs.writeFileSync(path.join(profileDir, `${profileId}.json`), JSON.stringify(profile, null, 2));

  onProgress({ stage: 'done' });
  return profileId;
}

async function installQuilt(mcVersion, loaderVersion, onProgress = () => {}) {
  onProgress({ stage: 'fetchingProfile' });
  const res = await fetch(`${QUILT_META}/versions/loader/${mcVersion}/${loaderVersion}/profile/json`);
  if (!res.ok) throw new Error('Quilt profili alınamadı (Minecraft/yükleyici sürüm uyumunu kontrol edin)');
  const profile = await res.json();

  const versionsDir = getVersionsRootDir();
  const profileId = profile.id;
  const profileDir = path.join(versionsDir, profileId);
  fs.mkdirSync(profileDir, { recursive: true });
  fs.writeFileSync(path.join(profileDir, `${profileId}.json`), JSON.stringify(profile, null, 2));

  onProgress({ stage: 'done' });
  return profileId;
}

/**
 * Forge / NeoForge: resmi "installer.jar" indirilip --installClient parametresiyle
 * java üzerinden çalıştırılır. Bu işlem bir Java kurulumu gerektirir ve resmi
 * Forge/NeoForge sürüm biçimlendirmesine bağlı olduğundan, sürüm numaraları zaman
 * içinde değişebilir. Bu fonksiyon en güncel resmi installer dağıtım şemasını baz alır.
 */
async function installForgeLike({ type, mcVersion, loaderVersion, javaBinPath, onProgress = () => {} }) {
  onProgress({ stage: 'resolvingInstaller' });

  const installerUrl = type === 'neoforge'
    ? `https://maven.neoforged.net/releases/net/neoforged/neoforge/${loaderVersion}/neoforge-${loaderVersion}-installer.jar`
    : `https://maven.minecraftforge.net/net/minecraftforge/forge/${mcVersion}-${loaderVersion}/forge-${mcVersion}-${loaderVersion}-installer.jar`;

  const versionsDir = getVersionsRootDir();
  const installerPath = path.join(versionsDir, `${type}-installer-${loaderVersion}.jar`);

  onProgress({ stage: 'downloading' });
  const res = await fetch(installerUrl);
  if (!res.ok) throw new Error(`${type === 'neoforge' ? 'NeoForge' : 'Forge'} yükleyicisi indirilemedi (sürüm numarasını kontrol edin)`);
  const buffer = await res.buffer();
  fs.writeFileSync(installerPath, buffer);

  onProgress({ stage: 'installing' });
  // --installClient, launchManager.js'nin oyunu başlatırken baktığı AYNI .minecraft
  // kök dizinini almalı; aksi halde kurulan sürüm launcher tarafından görünmez.
  await new Promise((resolve, reject) => {
    const proc = spawn(javaBinPath, ['-jar', installerPath, '--installClient', getMinecraftRootDir()], {
      cwd: versionsDir
    });
    let stderr = '';
    proc.stderr.on('data', d => { stderr += d.toString(); });
    proc.on('close', code => {
      if (code === 0) resolve();
      else reject(new Error(`${type} kurulumu başarısız oldu (kod ${code}): ${stderr.slice(-500)}`));
    });
    proc.on('error', reject);
  });

  onProgress({ stage: 'done' });
  // Forge/NeoForge kurulumu, sürüm klasörünü doğrudan .minecraft/versions altına yazar;
  // profil adı geleneksel olarak "<mcVersion>-forge-<loaderVersion>" biçimindedir.
  return type === 'neoforge' ? `neoforge-${loaderVersion}` : `${mcVersion}-forge-${loaderVersion}`;
}

module.exports = {
  listFabricLoaderVersions,
  listQuiltLoaderVersions,
  installFabric,
  installQuilt,
  installForgeLike,
  getVersionsRootDir
};
