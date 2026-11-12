window.Pages = window.Pages || {};

// Ana sayfa seçili profil kimliği (hero OYNA + sürüm listesi bunu kullanır)
function selectedInstanceId() {
  return localStorage.getItem('buran.selectedInstance') || '';
}
function setSelectedInstanceId(id) {
  localStorage.setItem('buran.selectedInstance', id);
}

window.Pages.home = {
  async render() {
    const el = document.getElementById('page-home');
    const instances = await window.buran.instances.list();
    const account = await window.buran.accounts.getActive();

    // Seçili profil: kayıtlı id listede yoksa en son oynananı seç
    let selected = instances.find(i => i.id === selectedInstanceId());
    if (!selected && instances.length) {
      selected = [...instances].sort((a, b) => (b.lastPlayed || 0) - (a.lastPlayed || 0))[0];
      setSelectedInstanceId(selected.id);
    }

    el.innerHTML = `      <div class="hero">
        <div class="hero-bg" id="hero-bg"></div>
        <div class="hero-shade"></div>
        <button class="hero-gear" id="hero-gear" title="${t('sidebar.settings')}">
          <span class="icon" data-icon="settings"></span>
        </button>
        <div class="hero-content">
          <img class="hero-logo" src="assets/icons/minecraft-logo.png" alt="Minecraft" draggable="false" />
          <div class="hero-tagline">${t('home.tagline')}</div>
          <div class="hero-bar"></div>
          <button class="hero-play" id="hero-play">
            <span class="icon" data-icon="play"></span><span id="hero-play-label">${t('home.playNow')}</span>
          </button>
        </div>
        ${selected ? `
        <div class="hero-version" id="hero-version" title="">
          <div class="hero-version-main">${escapeHtml(selected.version)}</div>
          <div class="hero-version-sub">${escapeHtml(loaderLabel(selected.loader))} &bull; Java Edition</div>
          <span class="hero-version-chevron" data-icon="chevron-down"></span>
          <div class="hero-version-menu" id="hero-version-menu"></div>
        </div>` : ''}
      </div>

      <div class="home-grid">
        <div class="card home-card">
          <div class="home-card-head">
            <span class="icon" data-icon="box"></span>
            <span class="home-card-title">${t('home.versionSelect')}</span>
            <button class="home-card-link" id="home-seeall">${t('home.seeAll')} <span class="icon" data-icon="arrow-right"></span></button>
          </div>
          <div id="home-versions"></div>
        </div>
      </div>
    `;

    renderStaticIcons();
    await window.applyBackground();

    // --- Hero davranışları ---
    el.querySelector('#hero-gear').onclick = () => switchPage('settings');

    const playBtn = el.querySelector('#hero-play');
    const playLabel = el.querySelector('#hero-play-label');
    function refreshPlayButton() {
      if (!selected) return;
      const running = window.runningInstances && window.runningInstances.has(selected.id);
      playBtn.classList.toggle('stop', running);
      playLabel.textContent = running ? t('profiles.stop') : t('home.playNow');
    }
    refreshPlayButton();
    playBtn.onclick = async () => {
      if (!selected) return;
      const running = window.runningInstances && window.runningInstances.has(selected.id);
      if (running) {
        window.Pages.profiles.stopInstance(selected.id).then(() => window.Pages.home.render());
        return;
      }
      playBtn.disabled = true;
      try {
        await window.Pages.profiles.playInstance(selected.id);
      } finally {
        playBtn.disabled = false;
        window.Pages.home.render();
      }
    };

    // Sürüm seçici menüsü
    if (selected) {
      const versionChip = el.querySelector('#hero-version');
      const menu = el.querySelector('#hero-version-menu');
      menu.innerHTML = instances.map(inst => `
        <div class="hero-version-menu-item ${inst.id === selected.id ? 'selected' : ''}" data-id="${inst.id}">
          <img src="assets/icons/grass-profile.png" alt="" draggable="false" />
          <span class="vm-name">${escapeHtml(inst.name)}</span>
          <span class="vm-check" data-icon="check-circle"></span>
        </div>
      `).join('');
      renderStaticIcons();
      versionChip.onclick = (e) => {
        e.stopPropagation();
        menu.classList.toggle('open');
      };
      menu.querySelectorAll('.hero-version-menu-item').forEach(item => {
        item.onclick = () => {
          setSelectedInstanceId(item.getAttribute('data-id'));
          menu.classList.remove('open');
          window.Pages.home.render();
        };
      });
      document.addEventListener('click', () => menu.classList.remove('open'), { once: true });
    }

    // --- Sürüm Seçimi listesi ---
    const versionsEl = el.querySelector('#home-versions');
    if (!instances.length) {
      versionsEl.innerHTML = `
        <div class="news-empty">${t('home.noInstances')}</div>
        <button class="version-row-create" id="home-create-first">
          <span class="icon" data-icon="plus"></span>${t('home.createFirst')}
        </button>`;
      versionsEl.querySelector('#home-create-first').onclick = () => {
        switchPage('profiles');
        setTimeout(() => window.Pages.profiles.openCreateModal(), 150);
      };
    } else {
      const recent = [...instances].sort((a, b) => (b.lastPlayed || 0) - (a.lastPlayed || 0)).slice(0, 4);
      versionsEl.innerHTML = recent.map(inst => {
        const isSelected = selected && inst.id === selected.id;
        const running = window.runningInstances && window.runningInstances.has(inst.id);
        return `
        <div class="version-row ${isSelected ? 'selected' : ''}" data-id="${inst.id}">
          <img src="assets/icons/grass-profile.png" alt="" draggable="false" />
          <div class="vr-info">
            <div class="vr-name">${escapeHtml(inst.name)}</div>
            <div class="vr-meta">${escapeHtml(inst.version)} &bull; ${escapeHtml(loaderLabel(inst.loader))}</div>
          </div>
          <div class="vr-actions">
            ${isSelected ? `<span class="badge-selected">${t('home.selected')}</span>` : ''}
            <button class="icon-btn accent" data-play="${inst.id}" title="${running ? t('profiles.stop') : t('home.quickPlay')}">
              <span class="icon" data-icon="${running ? 'x' : 'play'}"></span>
            </button>
          </div>
        </div>`;
      }).join('') + `
      <button class="version-row-create" id="home-create-more">
        <span class="icon" data-icon="plus"></span>${t('home.newProfile')}
      </button>`;
      renderStaticIcons();
      versionsEl.querySelectorAll('[data-play]').forEach(btn => {
        btn.onclick = (e) => {
          e.stopPropagation();
          const id = btn.getAttribute('data-play');
          const running = window.runningInstances && window.runningInstances.has(id);
          if (running) {
            window.Pages.profiles.stopInstance(id).then(() => window.Pages.home.render());
          } else {
            window.Pages.profiles.playInstance(id).then(() => window.Pages.home.render());
          }
        };
      });
      // Satıra tıklamak profili seçili hale getirir (hero OYNA bu profili başlatır)
      versionsEl.querySelectorAll('.version-row').forEach(row => {
        row.onclick = () => {
          const id = row.getAttribute('data-id');
          if (id && id !== selectedInstanceId()) {
            setSelectedInstanceId(id);
            window.Pages.home.render();
          }
        };
      });
      versionsEl.querySelector('#home-create-more').onclick = () => {
        switchPage('profiles');
        setTimeout(() => window.Pages.profiles.openCreateModal(), 150);
      };
    }

    el.querySelector('#home-seeall').onclick = () => switchPage('profiles');
  }
};

function loaderLabel(loader) {
  if (!loader) return 'Vanilla';
  return loader.charAt(0).toUpperCase() + loader.slice(1);
}

function escapeHtml(str) {
  const d = document.createElement('div');
  d.textContent = str;
  return d.innerHTML;
}
