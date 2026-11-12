// Uygulama giriş noktası: sayfa yönlendirme, title bar, durum çubuğu, genel kurulum
const PAGES = ['home', 'profiles', 'modstore', 'modpacks', 'installedmods', 'resources', 'worlds', 'serverhost', 'accounts', 'settings'];

// Şu an çalışan instance'ların kimliklerini tutar; "Oyna" <-> "Durdur"
// düğme durumunu belirlemek için home.js ve profiles.js tarafından okunur.
window.runningInstances = new Set();

function switchPage(name) {
  document.querySelectorAll('.sidebar-nav li').forEach(li => {
    li.classList.toggle('active', li.getAttribute('data-page') === name);
  });
  document.querySelectorAll('.page').forEach(p => p.classList.remove('active'));
  document.getElementById(`page-${name}`).classList.add('active');
  window.Pages[name].render();
}

window.renderApp = function () {
  const current = document.querySelector('.sidebar-nav li.active')?.getAttribute('data-page') || 'home';
  switchPage(current);
};

// Hesabın 2D kafa görselini kullanıcı adına göre getirir (minotar.net).
// NOT: Önceden UUID tabanlı Crafatar kullanılıyordu, ancak Microsoft
// hesaplarında gerçek skin yerine bazen varsayılan Alex görseli dönüyordu.
// Kullanıcı adı bazlı sorgu (Mojang API'den anlık skin çözümlemesi yapar)
// bu sorunu ortadan kaldırır; offline hesaplarda da kullanıcı adına göre
// tutarlı bir varsayılan (Steve/Alex) gösterir.
function accountAvatarUrl(account) {
  const username = encodeURIComponent(account.username || 'Steve');
  return `https://minotar.net/helm/${username}/32.png`;
}
window.accountAvatarUrl = accountAvatarUrl;

// Sidebar altındaki hesap çubuğunu günceller (kullanıcı adı + çevrimiçi/çevrimdışı)
window.updateSidebarAccount = async function () {
  const account = await window.buran.accounts.getActive();
  const nameEl = document.getElementById('sidebar-username');
  const statusEl = document.getElementById('sidebar-status');
  const avatarEl = document.getElementById('sidebar-avatar');
  const chip = document.getElementById('sidebar-account');
  if (!nameEl) return;
  if (account) {
    nameEl.textContent = account.username;
    const offline = (account.type || 'offline') === 'offline';
    statusEl.textContent = offline ? t('accounts.typeOffline') : t('accounts.typeMicrosoft');
    statusEl.classList.toggle('online', !offline);
    chip.title = `${account.username} · ${offline ? t('accounts.typeOffline') : t('accounts.typeMicrosoft')}`;
    avatarEl.innerHTML = '';
    const img = document.createElement('img');
    img.src = accountAvatarUrl(account);
    img.alt = account.username;
    img.onerror = () => { img.style.display = 'none'; };
    avatarEl.appendChild(img);
  } else {
    nameEl.textContent = t('titlebar.noAccount');
    statusEl.textContent = t('accounts.typeOffline');
    statusEl.classList.remove('online');
    avatarEl.innerHTML = '';
  }
};

// Hero arkaplanı: özel arkaplan seçildiyse onu, yoksa gradyan kullanır
window.applyBackground = async function () {
  const heroBg = document.getElementById('hero-bg');
  if (!heroBg) return;
  try {
    const state = await window.buran.background.get();
    if (state.enabled && state.dataUrl) {
      heroBg.classList.add('has-image');
      heroBg.style.setProperty('--hero-image', `url("${state.dataUrl}")`);
    } else {
      heroBg.classList.remove('has-image');
      heroBg.style.removeProperty('--hero-image');
    }
  } catch (err) {
    heroBg.classList.remove('has-image');
    heroBg.style.removeProperty('--hero-image');
  }
};

// Alt durum çubuğu: gerçek RAM / gerçek Java sürümü / oyun dizini / canlı durum
const RAM_POLL_INTERVAL_MS = 3000;

function formatGigabytes(bytes) {
  return (Number(bytes) / 1073741824).toFixed(1);
}

// Etkinlik metni; null = meşgul değil. Olaylarla beslenir (aşağıda).
let statusBusyText = null;
let statusBusyAt = 0;

// Meşgul metnini ayarlar; boş/belirsiz metin durum "Hazır"/"Oynanıyor"na döner.
window.setStatusBusy = function (text) {
  statusBusyText = text || null;
  statusBusyAt = Date.now();
  window.updateStatusState();
};

async function updateRamDisplay() {
  const el = document.getElementById('status-ram-text');
  if (!el) return;
  try {
    const memory = await window.buran.system.memory();
    const total = Number(memory.total) || 0;
    const used = Number(memory.used) || 0;
    const percent = total > 0 ? Math.round((used / total) * 100) : 0;
    el.textContent = `${t('status.ram')}: ${formatGigabytes(used)} / ${formatGigabytes(total)} GB (%${percent})`;
  } catch (err) {
    // Sistem bilgisi alınamazsa ayarlardaki tahsis değerine düş
    try {
      const s = await window.buran.app.status();
      const ramMax = Number(s.ramMax) || 4096;
      el.textContent = `${t('status.ram')}: ${(ramMax / 1024).toFixed(0)} GB`;
    } catch (err2) {
      el.textContent = `${t('status.ram')}: —`;
    }
  }
}

async function updateJavaDisplay() {
  const el = document.getElementById('status-java-text');
  if (!el) return;
  let javaLabel = null;
  try {
    const info = await window.buran.java.version();
    if (info && info.version) {
      // Java 8 sürümleri eski biçimde (1.8.0_451) gelir
      javaLabel = info.version.startsWith('1.8')
        ? `Java 8 (${info.version})`
        : `Java ${info.version}`;
      el.title = info.path || '';
    }
  } catch (err) {
    javaLabel = null;
  }
  if (!javaLabel) {
    try {
      const s = await window.buran.app.status();
      const javaPath = s.javaPath || '';
      const match = javaPath.match(/jdk[-_]?(\d+)/i) || javaPath.match(/jdk(\d+\.\d+[\d.+]*)/i);
      if (match) javaLabel = `Java ${match[1]}`;
      else if (javaPath) javaLabel = javaPath.split(/[\\/]/).pop();
      else javaLabel = `${t('status.java')}: ${t('status.javaAuto')}`;
    } catch (err) {
      javaLabel = `${t('status.java')}: ${t('status.javaAuto')}`;
    }
  }
  el.textContent = `${t('status.java')}: ${javaLabel}`;
}

window.updateStatusBar = async function () {
  try {
    const s = await window.buran.app.status();
    const dirEl = document.getElementById('status-dir');
    const dirText = document.getElementById('status-dir-text');
    dirText.textContent = `${t('status.gameDir')}: ${s.gameDir}`;
    dirEl.title = `${s.gameDir} · ${t('status.openFolder')}`;
    dirEl.classList.add('clickable');
    dirEl.onclick = async () => {
      try {
        await window.buran.app.openPath(s.gameDir);
      } catch (err) {
        showToast(err.message || t('toast.genericError'), 'error');
      }
    };
    await Promise.all([updateRamDisplay(), updateJavaDisplay()]);
    document.getElementById('status-version')?.remove();
  } catch (err) {
    // durum çubuğu kritik değil
  }
};

window.updateStatusState = function () {
  const el = document.getElementById('status-state-text');
  const container = document.getElementById('status-state');
  if (!el) return;
  const running = window.runningInstances ? window.runningInstances.size : 0;
  if (statusBusyText) {
    el.textContent = statusBusyText;
    container.classList.add('busy');
    container.classList.remove('status-ready');
  } else if (running > 0) {
    el.textContent = t('status.playing');
    container.classList.remove('busy');
    container.classList.remove('status-ready');
  } else {
    el.textContent = t('status.ready');
    container.classList.remove('busy');
    container.classList.add('status-ready');
  }
};

// Launch ilerlemesini durum çubuğu metnine çevirir
function launchStatusText(status) {
  if (!status || !status.task) return null;
  const withCount = () =>
    Number.isFinite(status.current) && Number.isFinite(status.total) && status.total > 0
      ? ` (${status.current}/${status.total})`
      : '';
  switch (status.task) {
    case 'preparing':
      return t('status.preparing');
    case 'client':
      return `${t('status.downloadingMc')}${withCount()}`;
    case 'libraries':
    case 'assets':
      return `${t('status.downloadingFiles')}${withCount()}`;
    case 'java-auto-download':
      return t('status.downloadingJava');
    case 'loader-install':
      return t('status.installingLoader');
    case 'launching':
      return t('status.launching');
    default:
      return null;
  }
}

async function bootstrap() {
  renderStaticIcons();
  await initI18n();
  applyStaticI18n();
  renderStaticIcons();

  // Bu bir web sitesi değil, masaüstü launcher: tarayıcı davranışlarını kapat.
  document.addEventListener('contextmenu', (e) => e.preventDefault());
  document.addEventListener('keydown', (e) => {
    const key = e.key;
    if (key === 'F5' || key === 'F12') { e.preventDefault(); return; }
    if ((e.ctrlKey || e.metaKey) && !key.startsWith('F') && (key === 'r' || key === 'R' || key === 'u' || key === 'U')) {
      e.preventDefault();
      return;
    }
    if ((e.ctrlKey || e.metaKey) && e.shiftKey && (key === 'I' || key === 'i' || key === 'J' || key === 'j' || key === 'C' || key === 'c')) {
      e.preventDefault();
    }
  });

  document.querySelectorAll('.sidebar-nav li').forEach(li => {
    li.addEventListener('click', () => switchPage(li.getAttribute('data-page')));
  });

  document.getElementById('sidebar-account').onclick = () => switchPage('accounts');

  document.getElementById('btn-minimize').onclick = () => window.buran.window.minimize();
  document.getElementById('btn-maximize').onclick = () => window.buran.window.maximize();
  document.getElementById('btn-close').onclick = () => window.buran.window.close();

  // Oyun kapandığında (ister launcher'dan "Durdur" ile, ister oyuncu kendi
  // kapatsın) ilgili instance'ı çalışanlar listesinden çıkarıp aktif sayfayı
  // tazeler, böylece "Durdur" düğmesi otomatik olarak "Oyna"ya döner.
  window.buran.launch.onClosed(({ instanceId }) => {
    window.runningInstances.delete(instanceId);
    window.updateStatusState();
    const activeLi = document.querySelector('.sidebar-nav li.active');
    const current = activeLi ? activeLi.getAttribute('data-page') : 'home';
    if (current === 'home' || current === 'profiles') switchPage(current);
  });

  // --- Gerçek zamanlı durum çubuğu ---

  // Oyun başlatma ilerlemesi (indirme, hazırlama, loader kurulumu...)
  window.buran.launch.onStatus(({ status }) => {
    const text = launchStatusText(status);
    if (text) window.setStatusBusy(text);
  });

  // Modrinth indirmeleri (mod/kaynak paketi/shaderpack kurulumları)
  window.buran.modrinth.onProgress?.((payload) => {
    if (!payload || !payload.stage) return;
    if (payload.stage === 'done' || payload.stage === 'error') {
      window.setStatusBusy(null);
    } else if (payload.name) {
      window.setStatusBusy(`${t('status.downloading')}: ${payload.name}`);
    } else {
      window.setStatusBusy(t('status.downloading'));
    }
  });

  // Loader kurulumları (Fabric/Quilt/Forge/NeoForge)
  window.buran.loader.onProgress?.((payload) => {
    if (!payload || !payload.stage) return;
    if (payload.stage === 'done') {
      window.setStatusBusy(null);
    } else {
      window.setStatusBusy(t('status.installingLoader'));
    }
  });

  // RAM kullanımı canlı takip
  setInterval(updateRamDisplay, RAM_POLL_INTERVAL_MS);

  // Olay kaçarsa (örn. kurulum hatası olayı üretilmediyse) meşgul durumunu
  // çöz; boşta kalma koruması.
  setInterval(() => {
    if (statusBusyText && Date.now() - statusBusyAt > 120000) {
      window.setStatusBusy(null);
    }
  }, 30000);

  await updateSidebarAccount();
  await updateStatusBar();
  window.updateStatusState();
  switchPage('home');
}

document.addEventListener('DOMContentLoaded', bootstrap);
