// Uygulama giriş noktası: sayfa yönlendirme, title bar, genel kurulum
const PAGES = ['home', 'profiles', 'modstore', 'modpacks', 'installedmods', 'resources', 'accounts', 'settings'];

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

window.updateTitlebarAccount = async function () {
  const account = await window.buran.accounts.getActive();
  document.getElementById('titlebar-username').textContent = account ? account.username : t('titlebar.noAccount');
  const avatar = document.getElementById('titlebar-avatar');
  avatar.innerHTML = '';
  if (account) {
    const img = document.createElement('img');
    img.src = accountAvatarUrl(account);
    img.alt = account.username;
    img.style.cssText = 'width:100%;height:100%;object-fit:cover;border-radius:6px;image-rendering:pixelated;';
    img.onerror = () => { img.style.display = 'none'; };
    avatar.appendChild(img);
  }
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

window.applyBackground = async function () {
  const layer = document.getElementById('bg-layer');
  try {
    const state = await window.buran.background.get();
    layer.classList.remove('bg-off', 'bg-animated');
    if (state.enabled && state.dataUrl) {
      layer.style.backgroundImage = `url("${state.dataUrl}")`;
    } else {
      layer.style.backgroundImage = '';
      layer.classList.add('bg-animated');
    }
  } catch (err) {
    layer.classList.add('bg-animated');
  }
};

async function bootstrap() {
  renderStaticIcons();
  await initI18n();
  applyStaticI18n();
  renderStaticIcons();

  document.querySelectorAll('.sidebar-nav li').forEach(li => {
    li.addEventListener('click', () => switchPage(li.getAttribute('data-page')));
  });

  document.getElementById('btn-minimize').onclick = () => window.buran.window.minimize();
  document.getElementById('btn-maximize').onclick = () => window.buran.window.maximize();
  document.getElementById('btn-close').onclick = () => window.buran.window.close();

  // Oyun kapandığında (ister launcher'dan "Durdur" ile, ister oyuncu kendi
  // kapatsın) ilgili instance'ı çalışanlar listesinden çıkarıp aktif sayfayı
  // tazeler, böylece "Durdur" düğmesi otomatik olarak "Oyna"ya döner.
  window.buran.launch.onClosed(({ instanceId }) => {
    window.runningInstances.delete(instanceId);
    const activeLi = document.querySelector('.sidebar-nav li.active');
    const current = activeLi ? activeLi.getAttribute('data-page') : 'home';
    if (current === 'home' || current === 'profiles') switchPage(current);
  });

  await updateTitlebarAccount();
  await applyBackground();
  switchPage('home');
}

document.addEventListener('DOMContentLoaded', bootstrap);
