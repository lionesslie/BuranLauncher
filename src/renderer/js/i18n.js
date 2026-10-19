// Renderer tarafı i18n durumu ve yardımcı fonksiyonlar
let I18N_STATE = { lang: 'tr', translations: {} };

function t(key, vars = {}) {
  const parts = key.split('.');
  let node = I18N_STATE.translations;
  for (const p of parts) {
    if (node == null) break;
    node = node[p];
  }
  if (typeof node !== 'string') return key;
  return node.replace(/\{(\w+)\}/g, (_, k) => (vars[k] !== undefined ? vars[k] : `{${k}}`));
}

function applyStaticI18n() {
  document.querySelectorAll('[data-i18n]').forEach(el => {
    el.textContent = t(el.getAttribute('data-i18n'));
  });
  document.querySelectorAll('[data-i18n-placeholder]').forEach(el => {
    el.setAttribute('placeholder', t(el.getAttribute('data-i18n-placeholder')));
  });
  document.title = t('app.name');
}

async function initI18n() {
  I18N_STATE = await window.buran.i18n.get();
  applyStaticI18n();
}

async function changeLang(lang) {
  await window.buran.i18n.setLang(lang);
  await initI18n();
  document.dispatchEvent(new CustomEvent('langchange'));
}
