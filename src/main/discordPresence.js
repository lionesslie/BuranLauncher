// Discord Rich Presence: oyun açıkken Discord profilinde/durumunda
// "[Profil adı] oynuyor" gibi bir durum gösterir.
//
// NOT (doğrulama gerekli): @xhayper/discord-rpc paketinin tam API'si
// `npm install` sonrası node_modules/@xhayper/discord-rpc belgeleriyle
// doğrulanmalı — burada en yaygın kullanım deseni (Client + login +
// user.setActivity) esas alınmıştır. API küçük farklılıklar gösterirse
// (örn. setActivity doğrudan client üzerinde olabilir), aşağıdaki
// setActivitySafe() fonksiyonu birkaç olası çağrı şeklini sırayla dener.
//
// Discord yüklü değilse, kapalıysa, DISCORD_CLIENT_ID tanımlı değilse ya da
// bağlantı herhangi bir sebeple başarısız olursa bu modül SESSİZCE devre dışı
// kalır — launcher'ın normal çalışmasını asla engellemez ya da hata fırlatmaz.
const log = require('electron-log');
const store = require('./store');

let client = null;
let isReady = false;
let connectAttempted = false;

function isEnabled() {
  const settings = store.get('settings');
  return settings.discordPresenceEnabled !== false && !!process.env.DISCORD_CLIENT_ID;
}

async function init() {
  if (client) return; // zaten bağlı/bağlanıyor
  connectAttempted = true;

  if (!isEnabled()) {
    log.info('Discord Rich Presence devre dışı (ayar kapalı veya DISCORD_CLIENT_ID tanımsız).');
    return;
  }

  try {
    const { Client } = require('@xhayper/discord-rpc');
    client = new Client({ clientId: process.env.DISCORD_CLIENT_ID });

    client.on('ready', () => {
      isReady = true;
      log.info('Discord Rich Presence bağlandı.');
    });

    // Discord masaüstü istemcisi açık değilse bu reddedilir; bu normaldir,
    // sessizce yoksayılır (launcher'ı etkilemez).
    await client.login().catch(err => {
      log.info('Discord Rich Presence bağlanamadı (Discord açık olmayabilir): ' + err.message);
      client = null;
    });
  } catch (err) {
    log.warn('Discord Rich Presence başlatılamadı: ' + err.message);
    client = null;
  }
}

async function setActivitySafe(activity) {
  if (!client || !isReady) return;
  try {
    // Bilinen olası API şekilleri sırayla denenir (sürüme göre değişebilir).
    if (client.user && typeof client.user.setActivity === 'function') {
      await client.user.setActivity(activity);
    } else if (typeof client.setActivity === 'function') {
      await client.setActivity(activity);
    }
  } catch (err) {
    log.warn('Discord durumu güncellenemedi: ' + err.message);
  }
}

/**
 * Bir profil/instance oynanmaya başladığında çağrılır.
 */
async function setPlaying(instanceName, mcVersion) {
  if (!isEnabled()) return;
  // Ayar sonradan açıldıysa veya ilk bağlantı denemesi başarısız olduysa
  // (örn. Discord o an kapalıydı), her oyun başlatmada yeniden denenir.
  if (!client) await init();

  await setActivitySafe({
    details: `${instanceName} oynuyor`,
    state: mcVersion ? `Minecraft ${mcVersion}` : undefined,
    startTimestamp: new Date(),
    largeImageKey: 'buran_logo', // Discord Developer Portal'da "Rich Presence Assets" altında bu isimle bir görsel yüklenmeli
    largeImageText: 'Buran Launcher',
    instance: false
  });
}

/**
 * Oyun kapandığında çağrılır; durumu temizler.
 */
async function clearActivity() {
  if (!client || !isReady) return;
  try {
    if (client.user && typeof client.user.clearActivity === 'function') {
      await client.user.clearActivity();
    } else if (typeof client.clearActivity === 'function') {
      await client.clearActivity();
    }
  } catch (err) {
    log.warn('Discord durumu temizlenemedi: ' + err.message);
  }
}

function destroy() {
  if (client) {
    try { client.destroy(); } catch (err) { /* yoksay */ }
  }
  client = null;
  isReady = false;
}

module.exports = { init, setPlaying, clearActivity, destroy, isEnabled };
