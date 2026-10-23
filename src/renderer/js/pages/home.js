window.Pages = window.Pages || {};

window.Pages.home = {
  async render() {
    const el = document.getElementById('page-home');
    const instances = await window.buran.instances.list();
    const account = await window.buran.accounts.getActive();

    if (!instances.length) {
      el.innerHTML = `
        <h1 class="page-title">${t('home.welcome')}${account ? ', ' + account.username : ''}</h1>
        <div class="card" style="text-align:center; padding:48px;">
          <p class="subtle" style="margin-bottom:16px;">${t('home.noInstances')}</p>
          <button class="btn" id="home-create-first">${t('home.createFirst')}</button>
        </div>
      `;
      el.querySelector('#home-create-first').onclick = () => {
        document.querySelector('.sidebar-nav li[data-page="profiles"]').click();
        setTimeout(() => window.Pages.profiles.openCreateModal(), 150);
      };
      return;
    }

    const sorted = [...instances].sort((a, b) => (b.lastPlayed || 0) - (a.lastPlayed || 0));
    const recent = sorted.slice(0, 6);

    el.innerHTML = `
      <h1 class="page-title">${t('home.welcome')}${account ? ', ' + account.username : ''}</h1>
      <div class="grid">
        ${recent.map((inst, i) => {
          const isRunning = window.runningInstances && window.runningInstances.has(inst.id);
          return `
          <div class="card instance-card" data-id="${inst.id}" style="animation-delay:${i * 45}ms">
            <div class="instance-card-icon instance-card-icon-grass"></div>
            <div class="instance-card-name">${escapeHtml(inst.name)}</div>
            <div class="instance-card-meta">${inst.version} · ${inst.loader}</div>
            <div class="instance-card-actions">
              ${isRunning
                ? `<button class="btn danger stop-btn" data-id="${inst.id}">${t('profiles.stop')}</button>`
                : `<button class="btn play-btn" data-id="${inst.id}">${t('home.quickPlay')}</button>`}
            </div>
          </div>
        `;
        }).join('')}
      </div>
    `;
    el.querySelectorAll('.play-btn').forEach(btn => {
      btn.onclick = () => window.Pages.profiles.playInstance(btn.getAttribute('data-id')).then(() => window.Pages.home.render());
    });
    el.querySelectorAll('.stop-btn').forEach(btn => {
      btn.onclick = () => window.Pages.profiles.stopInstance(btn.getAttribute('data-id')).then(() => window.Pages.home.render());
    });
  }
};

function escapeHtml(str) {
  const d = document.createElement('div');
  d.textContent = str;
  return d.innerHTML;
}
