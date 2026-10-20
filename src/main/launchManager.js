// minecraft-launcher-core (mclc) üzerinden oyunu child process olarak başlatır
const { Client } = require('minecraft-launcher-core');
const log = require('electron-log');
const store = require('./store');
const instanceManager = require('./instanceManager');
const authManager = require('./authManager');
const javaManager = require('./javaManager');
const { getMinecraftRootDir } = require('./paths');

const activeProcesses = new Map(); // instanceId -> ChildProcess

/**
 * onLog: (line) => void      - oyunun stdout/stderr çıktısı
 * onStatus: (status) => void - indirme/hazırlık aşaması bildirimleri
 * onClose: (code) => void    - oyun kapandığında
 */
async function launchInstance(instanceId, { onLog = () => {}, onStatus = () => {}, onClose = () => {} } = {}) {
  const instance = instanceManager.getInstance(instanceId);
  const account = authManager.getActiveAccount();
  if (!account) throw new Error('Önce bir hesap seçmelisiniz');

  const accounts = store.get('accounts');
  const fullAccount = accounts.find(a => a.id === account.id);
  const mclcAuth = await authManager.getMclcAuthForAccount(fullAccount);

  const settings = store.get('settings');
  let javaPath = settings.javaPath; // kullanıcının manuel seçtiği yol varsa ona dokunulmaz

  if (!javaPath) {
    javaPath = javaManager.detectSystemJava();
  }

  if (!javaPath) {
    // Ne manuel ayar ne de sistemde Java var: bu Minecraft sürümü için gereken
    // Java sürümü otomatik olarak tespit edilip indirilir (8/17/21/25 vb.).
    onStatus({ task: 'java-auto-download', instanceVersion: instance.version });
    try {
      javaPath = await javaManager.ensureJavaForVersion(instance.version, (p) => {
        onStatus({ task: 'java-auto-download', ...p });
      });
    } catch (err) {
      throw new Error('Java otomatik indirilemedi: ' + err.message + ' — Ayarlar üzerinden manuel bir Java yolu belirtebilirsiniz.');
    }
  }

  const instanceDir = instanceManager.getInstanceDir(instanceId);
  const rootDir = getMinecraftRootDir();

  const launcher = new Client();

  // Instance'a özel mods/resourcepacks/saves vb. klasörleri .minecraft kökü ile
  // birleştirmek için mclc'nin root + overrides mekanizması kullanılır.
  const opts = {
    authorization: mclcAuth,
    root: rootDir,
    version: {
      number: instance.version,
      type: instance.versionType
    },
    memory: {
      min: `${settings.ramMin}M`,
      max: `${settings.ramMax}M`
    },
    javaPath,
    overrides: {
      gameDirectory: instanceDir,
      detached: false
    }
  };

  if (instance.loader === 'fabric' && instance.loaderVersion) {
    opts.version.custom = `fabric-loader-${instance.loaderVersion}-${instance.version}`;
  } else if (instance.loader === 'quilt' && instance.loaderVersion) {
    opts.version.custom = `quilt-loader-${instance.loaderVersion}-${instance.version}`;
  } else if (instance.loader === 'forge' && instance.loaderVersion) {
    opts.version.custom = `${instance.version}-forge-${instance.loaderVersion}`;
  } else if (instance.loader === 'neoforge' && instance.loaderVersion) {
    opts.version.custom = `neoforge-${instance.loaderVersion}`;
  }

  if (settings.extraArgs && settings.extraArgs.trim()) {
    opts.customArgs = settings.extraArgs.trim().split(/\s+/);
  }

  const startedAt = Date.now();

  launcher.on('debug', msg => onLog(`[debug] ${msg}`));
  launcher.on('data', msg => onLog(msg.toString()));
  launcher.on('download-status', status => onStatus(status));
  launcher.on('progress', status => onStatus(status));
  launcher.on('close', code => {
    const durationMs = Date.now() - startedAt;
    instanceManager.recordPlaySession(instanceId, durationMs);
    activeProcesses.delete(instanceId);
    onClose(code);
  });

  try {
    const proc = await launcher.launch(opts);
    if (proc) activeProcesses.set(instanceId, proc);
    return true;
  } catch (err) {
    log.error('Oyun başlatılamadı', err);
    throw err;
  }
}

function isRunning(instanceId) {
  return activeProcesses.has(instanceId);
}

function killInstance(instanceId) {
  const proc = activeProcesses.get(instanceId);
  if (proc) {
    proc.kill();
    activeProcesses.delete(instanceId);
    return true;
  }
  return false;
}

module.exports = { launchInstance, isRunning, killInstance };
