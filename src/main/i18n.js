// Basit JSON tabanlı i18n sistemi (ana süreç tarafı)
const fs = require('fs');
const path = require('path');
const { app } = require('electron');
const store = require('./store');

const SUPPORTED_LANGS = ['tr', 'ru'];
const DEFAULT_LANG = 'tr';

function resolveLocalesDir() {
  // Paketlenmiş uygulamada da doğru çalışması için app.getAppPath() kullanılır
  return path.join(app.getAppPath(), 'locales');
}

function detectSystemLang() {
  const sysLang = (app.getLocale() || '').toLowerCase();
  if (sysLang.startsWith('ru')) return 'ru';
  if (sysLang.startsWith('tr')) return 'tr';
  return DEFAULT_LANG;
}

function getCurrentLang() {
  const saved = store.get('language');
  if (saved && SUPPORTED_LANGS.includes(saved)) return saved;
  return detectSystemLang();
}

function setLang(lang) {
  if (!SUPPORTED_LANGS.includes(lang)) throw new Error('Desteklenmeyen dil: ' + lang);
  store.set('language', lang);
  return lang;
}

function loadTranslations(lang) {
  const filePath = path.join(resolveLocalesDir(), `${lang}.json`);
  const raw = fs.readFileSync(filePath, 'utf-8');
  return JSON.parse(raw);
}

function getTranslations() {
  const lang = getCurrentLang();
  try {
    return { lang, translations: loadTranslations(lang) };
  } catch (err) {
    // Dosya okunamazsa varsayılana düş
    return { lang: DEFAULT_LANG, translations: loadTranslations(DEFAULT_LANG) };
  }
}

module.exports = { getCurrentLang, setLang, getTranslations, SUPPORTED_LANGS };
