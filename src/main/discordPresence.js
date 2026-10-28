// Discord Rich Presence: oyun açıkken Discord profilinde/durumunda
// "[Profil adı] oynuyor" gibi bir durum gösterir.
//
// NOT (doğrulama gerekli): @xhayper/discord-rpc paketinin tam API'si
// `npm install` sonrası node_modules/@xhayper/discord-rpc belgeleriyle
// doğrulanmalı — burada en yaygın kullanım deseni (Client + login +
// user.setActivity) esas alınmıştır.
//
// Discord yüklü değilse, kapalıysa, DISCORD_CLIENT_ID tanımlı değilse ya da
// bağlantı herhangi bir sebeple başarısız olursa bu modül SESSİZCE devre dışı
// kalır — launcher'ın normal çalışmasını asla engellemez ya da hata fırlatmaz.
// ANCAK: tüm bağlantı/durum adımları artık hem electron-log'a HEM DE
// renderer'daki Günlük paneline ("Discord" kaynağıyla) gönderiliyor, böylece
// bir şey çalışmazsa sebebi diskteki log dosyasını açmadan görülebilir.
const log = require('electron-log');
const store = require('./store');

let client = null;
let isReady = false;
let initPromise = null;
let mainWindowRef = null;

function notify(level, message) {
  if (level === 'error') log.error('[Discord] ' + message);
  else if (level === 'warn') log.warn('[Discord] ' + message);
  else log.info('[Discord] ' + message);

  if (mainWindowRef && !mainWindowRef.isDestroyed()) {
    mainWindowRef.webContents.send('discord:log', { level, source: 'Discord', message });
  }
}

function isEnabled() {
  const settings = store.get('settings');
  return settings.discordPresenceEnabled !== false && !!process.env.DISCORD_CLIENT_ID;
}

/**
 * mainWindow referansı, bağlantı durumunu renderer'daki Günlük paneline
 * yazabilmek için app başlangıcında bir kez verilir.
 */
async function init(mainWindow) {
  if (mainWindow) mainWindowRef = mainWindow;
  if (initPromise) return initPromise; // zaten bağlanıyor/bağlandı, tekrar başlatma

  if (!isEnabled()) {
    const settings = store.get('settings');
    if (!settings.discordPresenceEnabled) {
      notify('info', 'Devre dışı (Ayarlar\'dan kapalı).');
    } else {
      notify('info', 'DISCORD_CLIENT_ID tanımlı değil, bağlantı denenmiyor (.env dosyasını kontrol edin).');
    }
    return;
  }

  notify('info', 'DISCORD_CLIENT_ID bulundu, Discord\'a bağlanılıyor...');

  initPromise = (async () => {
    try {
      const { Client } = require('@xhayper/discord-rpc');
      client = new Client({ clientId: process.env.DISCORD_CLIENT_ID });

      const readyPromise = new Promise(resolve => {
        client.on('ready', () => {
          isReady = true;
          notify('success', 'Bağlandı ve hazır.');
          resolve();
        });
      });

      client.on('disconnected', () => {
        isReady = false;
        notify('warn', 'Bağlantı koptu.');
      });

      let loginFailed = false;
      await client.login().catch(err => {
        loginFailed = true;
        notify('warn', 'Bağlanılamadı — Discord masaüstü uygulaması açık mı? (' + err.message + ')');
        client = null;
      });

      if (loginFailed) return;

      // login() çözüldükten sonra 'ready' olayının gelmesini en fazla 6 saniye
      // bekle; önceki sürümde bu bekleme YOKTU ve setActivity çağrıları
      // isReady henüz true olmadan sessizce hiçbir şey yapmadan dönüyordu.
      await Promise.race([readyPromise, new Promise(resolve => setTimeout(resolve, 6000))]);
      if (!isReady) {
        notify('warn', '"ready" sinyali 6 saniyede gelmedi, durum gösterilemeyebilir.');
      }
    } catch (err) {
      notify('error', 'Başlatılamadı: ' + err.message);
      client = null;
    }
  })();

  return initPromise;
}

async function setActivitySafe(activity) {
  if (!client) {
    notify('warn', 'Durum gönderilemedi: Discord\'a bağlı değil.');
    return;
  }
  if (!isReady) {
    notify('warn', 'Durum gönderilemedi: bağlantı henüz "hazır" değil.');
    return;
  }
  try {
    if (client.user && typeof client.user.setActivity === 'function') {
      await client.user.setActivity(activity);
    } else if (typeof client.setActivity === 'function') {
      await client.setActivity(activity);
    } else {
      notify('error', 'setActivity fonksiyonu bulunamadı (paket API\'si beklenenden farklı olabilir).');
      return;
    }
    notify('info', `Durum güncellendi: "${activity.details}"`);
  } catch (err) {
    notify('error', 'Durum güncellenemedi: ' + err.message);
  }
}

const LOADER_LABELS = {
  fabric: 'Fabric',
  quilt: 'Quilt',
  forge: 'Forge',
  neoforge: 'NeoForge'
};

/**
 * Bir profil/instance oynanmaya başladığında çağrılır.
 *
 * Alan isimleri (details/state/timestamps/largeImage.../smallImage...),
 * Discord'un resmi Rich Presence aktivite şemasıyla birebir aynıdır.
 * BİLİNÇLİ OLARAK EKLENMEYENLER: partyId/partySize/partyMax/joinSecret gibi
 * çoklu oyuncu/parti alanları — Buran Launcher gerçek bir multiplayer
 * oturumu/parti verisi takip etmediği için bunları sahte doldurmuyoruz.
 */
async function setPlaying(instanceName, mcVersion, loader) {
  if (!isEnabled()) return;
  if (!client || !isReady) await init(mainWindowRef);

  const loaderLabel = loader && LOADER_LABELS[loader];

  await setActivitySafe({
    details: `${instanceName} oynuyor`,
    state: mcVersion ? `Minecraft ${mcVersion}${loaderLabel ? ' · ' + loaderLabel : ''}` : undefined,
    startTimestamp: new Date(),
    largeImageKey: 'buran_logo', // Discord Developer Portal -> Rich Presence -> Art Assets altında bu isimle yüklenmeli
    largeImageText: 'Buran Launcher',
    smallImageKey: loaderLabel ? loader : undefined,
    smallImageText: loaderLabel,
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
    notify('info', 'Durum temizlendi.');
  } catch (err) {
    notify('warn', 'Durum temizlenemedi: ' + err.message);
  }
}

function destroy() {
  if (client) {
    try { client.destroy(); } catch (err) { /* yoksay */ }
  }
  client = null;
  isReady = false;
  initPromise = null;
}

module.exports = { init, setPlaying, clearActivity, destroy, isEnabled };
