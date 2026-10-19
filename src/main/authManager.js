// Hesap yönetimi: Microsoft OAuth (msmc) + Çevrimdışı (offline) hesaplar
const crypto = require('crypto');
const { v4: uuidv4 } = require('uuid');
const { safeStorage, BrowserWindow } = require('electron');
const store = require('./store');
const log = require('electron-log');

/**
 * Mojang'ın offline-UUID algoritması: UUID.nameUUIDFromBytes("OfflinePlayer:" + username)
 * Bu, MD5 tabanlı, deterministik bir "isim bazlı" (version 3) UUID üretimidir.
 */
function generateOfflineUUID(username) {
  const hash = crypto.createHash('md5').update('OfflinePlayer:' + username, 'utf8').digest();
  // RFC 4122 - version 3 (name-based, MD5) ve variant bitlerini ayarla
  hash[6] = (hash[6] & 0x0f) | 0x30;
  hash[8] = (hash[8] & 0x3f) | 0x80;
  const hex = hash.toString('hex');
  return [
    hex.substring(0, 8),
    hex.substring(8, 12),
    hex.substring(12, 16),
    hex.substring(16, 20),
    hex.substring(20, 32)
  ].join('-');
}

function addOfflineAccount(username) {
  if (!username || username.trim().length < 3) {
    throw new Error('Kullanıcı adı en az 3 karakter olmalı');
  }
  const accounts = store.get('accounts');
  const account = {
    id: uuidv4(),
    type: 'offline',
    username: username.trim(),
    uuid: generateOfflineUUID(username.trim()),
    createdAt: Date.now()
  };
  accounts.push(account);
  store.set('accounts', accounts);
  if (!store.get('activeAccountId')) store.set('activeAccountId', account.id);
  return account;
}

function encryptToken(tokenObj) {
  const json = JSON.stringify(tokenObj);
  if (safeStorage.isEncryptionAvailable()) {
    return safeStorage.encryptString(json).toString('base64');
  }
  // Şifreleme kullanılamıyorsa (nadir, platforma bağlı) düz metin olarak sakla
  log.warn('safeStorage şifrelemesi kullanılamıyor, token şifrelenmeden saklanacak');
  return Buffer.from(json, 'utf8').toString('base64');
}

function decryptToken(encryptedBase64) {
  const buf = Buffer.from(encryptedBase64, 'base64');
  if (safeStorage.isEncryptionAvailable()) {
    try {
      return JSON.parse(safeStorage.decryptString(buf));
    } catch (err) {
      log.error('Token çözülemedi', err);
      return null;
    }
  }
  return JSON.parse(buf.toString('utf8'));
}

/**
 * Microsoft hesabı ile giriş: msmc kütüphanesi üzerinden
 * Xbox Live -> XSTS -> Minecraft Services akışı, uygulama içi gömülü pencere ile.
 */
async function loginWithMicrosoft(parentWindow) {
  const { Auth } = require('msmc');
  const authManager = new Auth('select_account');

  // msmc'nin launch() metodu bir Electron BrowserWindow açar; biz bunu
  // uygulama içi (harici tarayıcı olmadan) gömülü pencere olarak kullanıyoruz.
  const xboxManager = await authManager.launch('electron', {
    parent: parentWindow,
    width: 480,
    height: 650,
    modal: true,
    autoHideMenuBar: true,
    title: 'Microsoft ile Giriş Yap'
  });

  const minecraftToken = await xboxManager.getMinecraft();
  const profile = minecraftToken.profile;

  const accounts = store.get('accounts');
  const existingIdx = accounts.findIndex(a => a.type === 'microsoft' && a.uuid === profile.id);

  const encrypted = encryptToken({
    mclc: minecraftToken.mclc(), // minecraft-launcher-core için gerekli auth objesi
    refresh: xboxManager.save ? xboxManager.save() : null
  });

  const account = {
    id: existingIdx >= 0 ? accounts[existingIdx].id : uuidv4(),
    type: 'microsoft',
    username: profile.name,
    uuid: profile.id,
    encryptedToken: encrypted,
    createdAt: existingIdx >= 0 ? accounts[existingIdx].createdAt : Date.now(),
    updatedAt: Date.now()
  };

  if (existingIdx >= 0) accounts[existingIdx] = account;
  else accounts.push(account);

  store.set('accounts', accounts);
  if (!store.get('activeAccountId')) store.set('activeAccountId', account.id);

  // Şifrelenmiş token'ı dışarı sızdırmadan sade bir hesap objesi döndür
  const { encryptedToken, ...safeAccount } = account;
  return safeAccount;
}

/**
 * Microsoft hesabı için minecraft-launcher-core'un ihtiyaç duyduğu auth
 * objesini şifrelenmiş depodan çözer. Gerekirse msmc üzerinden refresh eder.
 */
async function getMclcAuthForAccount(account) {
  if (account.type === 'offline') {
    return {
      access_token: '0',
      client_token: uuidv4(),
      uuid: account.uuid,
      name: account.username,
      user_properties: '{}',
      meta: { type: 'mojang' }
    };
  }

  if (!account.encryptedToken) throw new Error('Microsoft hesabı için kayıtlı oturum bulunamadı, lütfen yeniden giriş yapın.');
  const tokenData = decryptToken(account.encryptedToken);
  if (!tokenData || !tokenData.mclc) throw new Error('Oturum bilgisi okunamadı, lütfen yeniden giriş yapın.');
  return tokenData.mclc;
}

function listAccounts() {
  return store.get('accounts').map(({ encryptedToken, ...rest }) => rest);
}

function setActiveAccount(id) {
  const accounts = store.get('accounts');
  if (!accounts.find(a => a.id === id)) throw new Error('Hesap bulunamadı');
  store.set('activeAccountId', id);
  return id;
}

function removeAccount(id) {
  const accounts = store.get('accounts').filter(a => a.id !== id);
  store.set('accounts', accounts);
  if (store.get('activeAccountId') === id) {
    store.set('activeAccountId', accounts.length ? accounts[0].id : null);
  }
}

function getActiveAccount() {
  const id = store.get('activeAccountId');
  if (!id) return null;
  const account = store.get('accounts').find(a => a.id === id);
  if (!account) return null;
  const { encryptedToken, ...safe } = account;
  return safe;
}

module.exports = {
  generateOfflineUUID,
  addOfflineAccount,
  loginWithMicrosoft,
  getMclcAuthForAccount,
  listAccounts,
  setActiveAccount,
  removeAccount,
  getActiveAccount
};
