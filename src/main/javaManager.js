// Java tespiti ve Adoptium (Eclipse Temurin) üzerinden otomatik Java indirme
const fs = require('fs');
const path = require('path');
const os = require('os');
const { execSync, spawn } = require('child_process');
const { app } = require('electron');
const fetch = require('node-fetch');
const extractZip = require('extract-zip');
const log = require('electron-log');

const ADOPTIUM_API = 'https://api.adoptium.net/v3';

function getPlatformInfo() {
  const platform = process.platform === 'win32' ? 'windows' : process.platform === 'linux' ? 'linux' : null;
  if (!platform) throw new Error('Desteklenmeyen platform: ' + process.platform);
  const arch = process.arch === 'x64' ? 'x64' : process.arch === 'arm64' ? 'aarch64' : 'x64';
  return { platform, arch };
}

function detectSystemJava() {
  try {
    const cmd = process.platform === 'win32' ? 'where java' : 'which java';
    const out = execSync(cmd, { encoding: 'utf8' }).trim().split('\n')[0];
    if (out) return out;
  } catch (err) {
    // java bulunamadı - sorun değil, kullanıcı manuel seçebilir ya da otomatik indirebiliriz
  }
  return null;
}

function getJavaInstallDir(majorVersion) {
  const dir = path.join(app.getPath('userData'), 'java', `jdk-${majorVersion}`);
  fs.mkdirSync(dir, { recursive: true });
  return dir;
}

/**
 * Minecraft sürümüne göre gereken Java major sürümünü belirler (yaklaşık kural).
 */
function requiredJavaVersionForMc(mcVersion) {
  const [major, minor] = mcVersion.split('.').map(n => parseInt(n, 10));
  if (major === 1 && minor >= 20) return 21;
  if (major === 1 && minor >= 18) return 17;
  if (major === 1 && minor >= 17) return 16;
  return 8;
}

async function downloadAndInstallJava(javaMajorVersion, onProgress = () => {}) {
  const { platform, arch } = getPlatformInfo();
  const installDir = getJavaInstallDir(javaMajorVersion);

  onProgress({ stage: 'fetchingMeta' });
  const metaUrl = `${ADOPTIUM_API}/assets/latest/${javaMajorVersion}/hotspot?architecture=${arch}&image_type=jdk&os=${platform}&vendor=eclipse`;
  const res = await fetch(metaUrl);
  if (!res.ok) throw new Error(`Adoptium API hatası: HTTP ${res.status}`);
  const assets = await res.json();
  if (!assets.length) throw new Error(`Java ${javaMajorVersion} için uygun paket bulunamadı`);

  const binary = assets[0].binary;
  const downloadUrl = binary.package.link;
  const archivePath = path.join(installDir, binary.package.name);

  onProgress({ stage: 'downloading' });
  const dres = await fetch(downloadUrl);
  if (!dres.ok) throw new Error(`Java indirilemedi: HTTP ${dres.status}`);
  const buffer = await dres.buffer();
  fs.writeFileSync(archivePath, buffer);

  onProgress({ stage: 'extracting' });
  if (archivePath.endsWith('.zip')) {
    await extractZip(archivePath, { dir: installDir });
  } else {
    // .tar.gz (Linux) - tar komutu ile çıkar
    execSync(`tar -xzf "${archivePath}" -C "${installDir}"`);
  }
  fs.unlinkSync(archivePath);

  // Çıkarılan alt klasörü bul (jdk-21.0.x+y gibi)
  const entries = fs.readdirSync(installDir).filter(f => fs.statSync(path.join(installDir, f)).isDirectory());
  if (!entries.length) throw new Error('Java paketi çıkarılamadı');
  const extractedDir = path.join(installDir, entries[0]);

  // Uygulama sadece Windows ve Linux'u hedeflediği için (bkz. getPlatformInfo),
  // macOS'a özgü "Contents/Home" alt yolu kaldırıldı.
  const javaBin = platform === 'windows'
    ? path.join(extractedDir, 'bin', 'java.exe')
    : path.join(extractedDir, 'bin', 'java');

  if (!fs.existsSync(javaBin)) {
    throw new Error('İndirilen Java paketi içinde java çalıştırılabilir dosyası bulunamadı: ' + javaBin);
  }
  if (platform === 'linux') {
    // Linux arşivlerinde çalıştırma izni bazen kayboluyor; garanti altına al
    fs.chmodSync(javaBin, 0o755);
  }

  onProgress({ stage: 'done' });
  return javaBin;
}

module.exports = {
  detectSystemJava,
  requiredJavaVersionForMc,
  downloadAndInstallJava,
  getPlatformInfo
};
