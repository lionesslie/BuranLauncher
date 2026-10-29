// Sunucu Barındırma: launcher içinden Minecraft sunucusu oluşturma,
// başlatma/durdurma, canlı konsol ve server.properties yönetimi.
const fs = require('fs');
const path = require('path');
const { spawn } = require('child_process');
const { app, shell } = require('electron');
const { v4: uuidv4 } = require('uuid');
const log = require('electron-log');
const store = require('./store');
const javaManager = require('./javaManager');
const versionManager = require('./versionManager');
const loaderManager = require('./loaderManager');

const activeServers = new Map(); // serverId -> ChildProcess

function getServersRootDir() {
  const dir = path.join(app.getPath('userData'), 'servers');
  fs.mkdirSync(dir, { recursive: true });
  return dir;
}

function getServerDir(serverId) {
  return path.join(getServersRootDir(), serverId);
}

function listServers() {
  return store.get('servers');
}

function getServer(serverId) {
  const server = store.get('servers').find(s => s.id === serverId);
  if (!server) throw new Error('Sunucu bulunamadı: ' + serverId);
  return server;
}

// --- server.properties basit key=value ayrıştırıcı (yorum satırlarını korur) ---
function readProperties(filePath) {
  if (!fs.existsSync(filePath)) return {};
  const lines = fs.readFileSync(filePath, 'utf8').split('\n');
  const props = {};
  for (const line of lines) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith('#')) continue;
    const idx = trimmed.indexOf('=');
    if (idx === -1) continue;
    props[trimmed.slice(0, idx)] = trimmed.slice(idx + 1);
  }
  return props;
}

function writeProperties(filePath, props) {
  const existing = fs.existsSync(filePath) ? fs.readFileSync(filePath, 'utf8').split('\n') : [];
  const seen = new Set();
  const outLines = existing.map(line => {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith('#')) return line;
    const idx = trimmed.indexOf('=');
    if (idx === -1) return line;
    const key = trimmed.slice(0, idx);
    if (Object.prototype.hasOwnProperty.call(props, key)) {
      seen.add(key);
      return `${key}=${props[key]}`;
    }
    return line;
  });
  for (const [key, value] of Object.entries(props)) {
    if (!seen.has(key)) outLines.push(`${key}=${value}`);
  }
  fs.writeFileSync(filePath, outLines.join('\n'));
}

function getServerProperties(serverId) {
  return readProperties(path.join(getServerDir(serverId), 'server.properties'));
}

function setServerProperties(serverId, patch) {
  const propsPath = path.join(getServerDir(serverId), 'server.properties');
  writeProperties(propsPath, patch);
  return readProperties(propsPath);
}

/**
 * Yeni bir sunucu oluşturur: dosyaları indirir/kurar, eula.txt ve
 * server.properties'i yazar. EULA onayı olmadan ASLA eula=true yazılmaz.
 */
async function createServer({ name, version, loader, loaderVersion, ramMin, ramMax, eulaAccepted, onProgress = () => {} }) {
  if (!name || !name.trim()) throw new Error('Sunucu adı boş olamaz');
  if (!eulaAccepted) throw new Error('Mojang EULA kabul edilmeden sunucu oluşturulamaz.');

  const serverId = uuidv4();
  const serverDir = getServerDir(serverId);
  fs.mkdirSync(serverDir, { recursive: true });

  let launchCommand;
  try {
    if (loader === 'vanilla') {
      onProgress({ stage: 'downloadingServer' });
      const detail = await versionManager.getVersionDetail(version);
      const serverDl = detail.downloads && detail.downloads.server;
      if (!serverDl) throw new Error('Bu sürüm için resmi sunucu dosyası yok (bazı eski sürümlerde sunucu yayınlanmamıştır)');
      const fetch = require('node-fetch');
      const res = await fetch(serverDl.url);
      if (!res.ok) throw new Error('Sunucu dosyası indirilemedi: HTTP ' + res.status);
      fs.writeFileSync(path.join(serverDir, 'server.jar'), await res.buffer());
      launchCommand = { type: 'jar', jarFile: 'server.jar' };
    } else if (loader === 'fabric') {
      launchCommand = await loaderManager.installFabricServer(serverDir, version, loaderVersion, onProgress);
    } else if (loader === 'quilt') {
      launchCommand = await loaderManager.installQuiltServer(serverDir, version, loaderVersion, onProgress);
    } else if (loader === 'forge' || loader === 'neoforge') {
      const settings = store.get('settings');
      const javaPath = settings.javaPath || javaManager.detectSystemJava();
      if (!javaPath) throw new Error('Forge/NeoForge sunucu kurulumu için Java gerekli (Ayarlar\'dan kurun)');
      launchCommand = await loaderManager.installForgeLikeServer(serverDir, { type: loader, mcVersion: version, loaderVersion, javaBinPath: javaPath, onProgress });
    } else {
      throw new Error('Bilinmeyen yükleyici: ' + loader);
    }
  } catch (err) {
    // Kurulum başarısız olduysa yarım kalan klasörü temizle
    try { fs.rmSync(serverDir, { recursive: true, force: true }); } catch (cleanupErr) { /* yoksay */ }
    throw err;
  }

  // EULA - sadece kullanıcı açıkça onayladıysa yazılır
  fs.writeFileSync(path.join(serverDir, 'eula.txt'), '# https://www.minecraft.net/en-us/eula\neula=true\n');

  // server.properties varsayılanları
  writeProperties(path.join(serverDir, 'server.properties'), {
    'server-port': '25565',
    'max-players': '20',
    'gamemode': 'survival',
    'difficulty': 'normal',
    'motd': name.trim(),
    'pvp': 'true',
    'online-mode': 'true',
    'white-list': 'false',
    'level-name': 'world'
  });

  const server = {
    id: serverId,
    name: name.trim(),
    version,
    loader,
    loaderVersion: loaderVersion || null,
    ramMin: ramMin || 1024,
    ramMax: ramMax || 2048,
    launchCommand,
    createdAt: Date.now(),
    lastStarted: null
  };

  const servers = store.get('servers');
  servers.push(server);
  store.set('servers', servers);

  onProgress({ stage: 'done' });
  return server;
}

function updateServerSettings(serverId, patch) {
  const servers = store.get('servers');
  const idx = servers.findIndex(s => s.id === serverId);
  if (idx === -1) throw new Error('Sunucu bulunamadı');
  servers[idx] = { ...servers[idx], ...patch, id: servers[idx].id };
  store.set('servers', servers);
  return servers[idx];
}

function deleteServer(serverId) {
  if (activeServers.has(serverId)) {
    throw new Error('Çalışan bir sunucu silinemez, önce durdurun.');
  }
  const servers = store.get('servers').filter(s => s.id !== serverId);
  store.set('servers', servers);
  const dir = getServerDir(serverId);
  if (fs.existsSync(dir)) fs.rmSync(dir, { recursive: true, force: true });
}

function openServerFolder(serverId) {
  shell.openPath(getServerDir(serverId));
}

function isRunning(serverId) {
  return activeServers.has(serverId);
}

/**
 * Sunucuyu başlatır. launchCommand.type:
 *  - 'jar': java -Xms -Xmx -jar <jarFile> nogui
 *  - 'script': run.sh/run.bat çalıştırılır (modern Forge/NeoForge)
 */
function startServer(serverId, { onLog = () => {}, onClose = () => {} } = {}) {
  if (activeServers.has(serverId)) throw new Error('Sunucu zaten çalışıyor');

  const server = getServer(serverId);
  const serverDir = getServerDir(serverId);
  const settings = store.get('settings');
  const javaPath = settings.javaPath || javaManager.detectSystemJava();
  if (!javaPath) throw new Error('Java bulunamadı. Ayarlar\'dan Java yolunu belirtin veya otomatik indirin.');

  let proc;
  if (server.launchCommand.type === 'script') {
    const scriptPath = path.join(serverDir, server.launchCommand.scriptFile);
    proc = spawn(scriptPath, ['nogui'], { cwd: serverDir, shell: true });
  } else {
    const args = [`-Xms${server.ramMin}M`, `-Xmx${server.ramMax}M`, '-jar', server.launchCommand.jarFile, 'nogui'];
    proc = spawn(javaPath, args, { cwd: serverDir });
  }

  activeServers.set(serverId, proc);
  updateServerSettings(serverId, { lastStarted: Date.now() });

  proc.stdout.on('data', d => onLog(d.toString()));
  proc.stderr.on('data', d => onLog(d.toString()));
  proc.on('close', code => {
    activeServers.delete(serverId);
    onClose(code);
  });
  proc.on('error', err => {
    activeServers.delete(serverId);
    onLog('[hata] Sunucu başlatılamadı: ' + err.message);
    onClose(-1);
  });

  return true;
}

/**
 * Sunucuya zarif bir şekilde "stop" komutu gönderir (dünyayı kaydedip kapanır).
 * Süreç yanıt vermezse (örn. donmuşsa) birkaç saniye sonra zorla sonlandırılır.
 */
function stopServer(serverId) {
  const proc = activeServers.get(serverId);
  if (!proc) return false;
  try {
    proc.stdin.write('stop\n');
  } catch (err) {
    proc.kill();
    return true;
  }
  setTimeout(() => {
    if (activeServers.get(serverId) === proc) {
      log.warn('Sunucu "stop" komutuna 15 saniyede yanıt vermedi, zorla kapatılıyor.');
      proc.kill();
    }
  }, 15000);
  return true;
}

function sendCommand(serverId, command) {
  const proc = activeServers.get(serverId);
  if (!proc) throw new Error('Sunucu çalışmıyor');
  proc.stdin.write(command + '\n');
}

module.exports = {
  listServers,
  getServer,
  createServer,
  updateServerSettings,
  deleteServer,
  openServerFolder,
  isRunning,
  startServer,
  stopServer,
  sendCommand,
  getServerProperties,
  setServerProperties
};
