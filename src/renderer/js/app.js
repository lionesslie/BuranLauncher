// Uygulama giriş noktası: sayfa yönlendirme, title bar, genel kurulum
const PAGES = ['home', 'profiles', 'modstore', 'modpacks', 'accounts', 'settings'];

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

  await updateTitlebarAccount();
  switchPage('home');
}

document.addEventListener('DOMContentLoaded', bootstrap);
