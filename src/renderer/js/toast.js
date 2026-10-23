// Paylaşılan yükleniyor spinner'ı: "Yükleniyor..." yazısı yerine kullanılır.
function loadingHTML(label) {
  return `<div class="spinner-wrap"><div class="spinner"></div><span>${label || t('common.loading')}</span></div>`;
}

function showToast(message, type = 'info', timeoutMs = 4000) {
  const container = document.getElementById('toast-container');
  const el = document.createElement('div');
  el.className = `toast ${type}`;
  el.textContent = message;
  container.appendChild(el);
  setTimeout(() => el.remove(), timeoutMs);
}
