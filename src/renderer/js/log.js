// Yapılandırılmış, filtrelenebilir, aranabilir Günlük (Log) paneli.
// Her kayıt {ts, level, source, message} şeklinde tutulur; ham mclc
// çıktısı basit kurallarla anlamlı seviye/kaynak etiketlerine ayrıştırılır.

const LOG_MAX_ENTRIES = 600;
let LOG_ENTRIES = [];
let LOG_FILTER = 'all'; // all | error | warn | game | system
let LOG_SEARCH = '';
let LOG_AUTOSCROLL = true;

const LOG_LEVEL_ICON = {
  info: 'log-info',
  debug: 'log-debug',
  warn: 'log-warn',
  error: 'log-error',
  success: 'log-success'
};

// Kaynak (source) kategorileri filtre gruplarıyla eşleştirilir:
// 'game' filtresi Oyun+Java+İndirme'yi, 'system' filtresi Sistem+Hesap'ı kapsar.
const SOURCE_FILTER_GROUP = {
  game: ['Oyun', 'Java', 'İndirme'],
  system: ['Sistem', 'Hesap', 'Discord']
};

function formatTime(ts) {
  const d = new Date(ts);
  const pad = n => String(n).padStart(2, '0');
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

function matchesFilter(entry) {
  if (LOG_FILTER !== 'all') {
    if (LOG_FILTER === 'error' && entry.level !== 'error') return false;
    if (LOG_FILTER === 'warn' && entry.level !== 'warn') return false;
    if (LOG_FILTER === 'game' && !SOURCE_FILTER_GROUP.game.includes(entry.source)) return false;
    if (LOG_FILTER === 'system' && !SOURCE_FILTER_GROUP.system.includes(entry.source)) return false;
  }
  if (LOG_SEARCH && !entry.message.toLowerCase().includes(LOG_SEARCH) && !entry.source.toLowerCase().includes(LOG_SEARCH)) {
    return false;
  }
  return true;
}

function createEntryNode(entry) {
  const el = document.createElement('div');
  el.className = `log-entry level-${entry.level}`;
  const iconName = LOG_LEVEL_ICON[entry.level] || 'log-info';
  el.innerHTML = `
    <span class="log-entry-icon">${icon(iconName)}</span>
    <span class="log-entry-time">${formatTime(entry.ts)}</span>
    <span class="log-entry-source">${escapeHtml(entry.source)}</span>
    <span class="log-entry-message">${escapeHtml(entry.message)}</span>
  `;
  return el;
}

function isScrolledToBottom(el) {
  return el.scrollHeight - el.scrollTop - el.clientHeight < 40;
}

function appendEntryDOM(entry) {
  const output = document.getElementById('log-output');
  if (!output) return;
  if (!matchesFilter(entry)) return;

  // Boş durum mesajı varsa kaldır
  const empty = output.querySelector('.log-empty');
  if (empty) empty.remove();

  const wasAtBottom = isScrolledToBottom(output);
  output.appendChild(createEntryNode(entry));

  // DOM'u şişirmemek için görünür satır sayısını sınırla
  while (output.children.length > 300) {
    output.removeChild(output.firstChild);
  }

  if (wasAtBottom && LOG_AUTOSCROLL) {
    output.scrollTop = output.scrollHeight;
    hideJumpButton();
  } else {
    showJumpButton();
  }
}

function showJumpButton() {
  const btn = document.getElementById('log-jump-bottom');
  if (btn) btn.style.display = 'block';
}
function hideJumpButton() {
  const btn = document.getElementById('log-jump-bottom');
  if (btn) btn.style.display = 'none';
}

function fullRerender() {
  const output = document.getElementById('log-output');
  if (!output) return;
  output.innerHTML = '';
  const matching = LOG_ENTRIES.filter(matchesFilter).slice(-300);
  if (!matching.length) {
    output.innerHTML = `<div class="log-empty">${t('log.empty')}</div>`;
  } else {
    matching.forEach(entry => output.appendChild(createEntryNode(entry)));
  }
  output.scrollTop = output.scrollHeight;
  hideJumpButton();
}

/**
 * Yeni bir günlük kaydı ekler. level: info|debug|warn|error|success
 * source: 'Oyun' | 'Java' | 'İndirme' | 'Sistem' | 'Hesap' (serbest metin de olabilir)
 */
function pushLog({ level = 'info', source = 'Sistem', message }) {
  if (!message) return;
  const entry = { ts: Date.now(), level, source, message };
  LOG_ENTRIES.push(entry);
  if (LOG_ENTRIES.length > LOG_MAX_ENTRIES) LOG_ENTRIES.shift();
  appendEntryDOM(entry);
}
window.pushLog = pushLog;

// Geriye dönük uyumluluk: eski appendLog(line) çağrıları hâlâ çalışsın.
// Ham mclc satırlarını basit kurallarla sınıflandırır.
function appendLog(line) {
  if (line == null) return;
  const text = String(line).trim();
  if (!text) return;

  let level = 'info';
  let source = 'Oyun';
  let message = text;

  if (text.startsWith('[debug]')) {
    level = 'debug';
    source = 'Sistem';
    message = text.replace(/^\[debug\]\s*/, '');
  } else if (text.startsWith('[hata]') || text.startsWith('[error]')) {
    level = 'error';
    source = 'Hesap';
    message = text.replace(/^\[(hata|error)\]\s*/i, '');
  } else if (/\b(error|exception|failed|hata)\b/i.test(text)) {
    level = 'error';
  } else if (/\bwarn(ing)?\b/i.test(text)) {
    level = 'warn';
  } else if (text.startsWith('---')) {
    level = 'success';
    source = 'Oyun';
    message = text.replace(/^-+\s*/, '').replace(/\s*-+$/, '');
  }

  pushLog({ level, source, message });
}
window.appendLog = appendLog;

function escapeHtml(str) {
  const d = document.createElement('div');
  d.textContent = str;
  return d.innerHTML;
}

document.addEventListener('DOMContentLoaded', () => {
  const panel = document.getElementById('log-panel');
  const header = panel.querySelector('.log-panel-header');
  const output = document.getElementById('log-output');
  const searchInput = document.getElementById('log-search');
  const expandBtn = document.getElementById('log-expand');
  const jumpBtn = document.getElementById('log-jump-bottom');

  // Başlık şeridine tıklamak panel açar/kapar; ama filtre/arama/butonlara
  // tıklamak bunu TETİKLEMEMELİ (event.stopPropagation ile korunuyor).
  header.addEventListener('click', (e) => {
    if (e.target.closest('.log-filters, .log-search, #log-clear, #log-expand')) return;
    panel.classList.toggle('open');
  });

  document.getElementById('log-clear').addEventListener('click', (e) => {
    e.stopPropagation();
    LOG_ENTRIES = [];
    fullRerender();
  });

  expandBtn.addEventListener('click', (e) => {
    e.stopPropagation();
    panel.classList.toggle('expanded');
  });

  document.querySelectorAll('.log-filter-btn').forEach(btn => {
    btn.addEventListener('click', (e) => {
      e.stopPropagation();
      document.querySelectorAll('.log-filter-btn').forEach(b => b.classList.remove('active'));
      btn.classList.add('active');
      LOG_FILTER = btn.getAttribute('data-filter');
      fullRerender();
    });
  });

  let searchTimer;
  searchInput.addEventListener('click', (e) => e.stopPropagation());
  searchInput.addEventListener('input', () => {
    clearTimeout(searchTimer);
    searchTimer = setTimeout(() => {
      LOG_SEARCH = searchInput.value.trim().toLowerCase();
      fullRerender();
    }, 200);
  });

  output.addEventListener('scroll', () => {
    if (isScrolledToBottom(output)) hideJumpButton();
  });

  jumpBtn.addEventListener('click', (e) => {
    e.stopPropagation();
    output.scrollTop = output.scrollHeight;
    hideJumpButton();
  });

  fullRerender();

  // --- Discord Rich Presence bağlantı/durum olayları ---
  window.buran.discord.onLog(({ level, source, message }) => {
    pushLog({ level, source: source || 'Discord', message });
  });

  // --- Oyun/launch olaylarının günlüğe akması ---
  window.buran.launch.onLog(({ line }) => appendLog(line));

  window.buran.launch.onStatus(({ status }) => {
    if (!status) return;
    if (status.task === 'java-auto-download') {
      const stage = status.stage;
      const javaMajor = status.javaMajor ? ` (Java ${status.javaMajor})` : '';
      const label = stage === 'cached' ? `Yerel Java bulundu${javaMajor}`
        : stage === 'autoDownloadStart' ? `Uygun Java bulunamadı, otomatik indiriliyor${javaMajor}...`
        : stage === 'fetchingMeta' ? `Java sürüm bilgisi alınıyor${javaMajor}...`
        : stage === 'downloading' ? `Java indiriliyor${javaMajor}...`
        : stage === 'extracting' ? `Java çıkartılıyor${javaMajor}...`
        : stage === 'done' ? `Java kurulumu tamamlandı${javaMajor}`
        : `${stage}${javaMajor}`;
      pushLog({ level: stage === 'done' ? 'success' : 'info', source: 'Java', message: label });
      return;
    }
    if (status.task) {
      const pct = status.total ? Math.round((status.current / status.total) * 100) : 0;
      pushLog({ level: 'info', source: 'İndirme', message: `${status.task} (%${pct})` });
    }
  });

  window.buran.launch.onClosed(({ code }) => {
    pushLog({
      level: code === 0 ? 'success' : 'warn',
      source: 'Oyun',
      message: `Oyun kapandı (çıkış kodu: ${code})`
    });
  });
});
