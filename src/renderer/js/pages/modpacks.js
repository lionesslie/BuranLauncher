window.Pages = window.Pages || {};

window.Pages.modpacks = {
  _scrollHandler: null,
  _searchState: null,

  async render() {
    const el = document.getElementById('page-modpacks');
    el.innerHTML = `
      <h1 class="page-title">${t('modpacks.title')}</h1>
      <div class="toolbar">
        <input type="search" id="mp-query" placeholder="${t('modpacks.searchPlaceholder')}" style="width:280px;" />
        <button class="btn secondary" id="mp-install-file">.mrpack ${t('modpacks.install')}</button>
      </div>
      <div id="mp-progress" class="card" style="display:none; margin-bottom:16px;">
        <div id="mp-progress-text" class="subtle" style="margin-bottom:8px;"></div>
        <div class="progress-bar"><div class="progress-bar-fill" id="mp-progress-fill" style="width:0%;"></div></div>
      </div>
      <div class="grid" id="mp-results"></div>
      <p class="subtle" id="mp-loading-more" style="text-align:center; padding:12px; display:none;">${t('common.loading')}</p>
    `;

    const queryInput = el.querySelector('#mp-query');
    const doSearch = debounce(() => this.freshSearch(), 350);
    queryInput.oninput = doSearch;

    const contentEl = document.querySelector('.content');
    if (this._scrollHandler) contentEl.removeEventListener('scroll', this._scrollHandler);
    this._scrollHandler = debounce(() => {
      if (!document.getElementById('page-modpacks').classList.contains('active')) return;
      const nearBottom = contentEl.scrollTop + contentEl.clientHeight >= contentEl.scrollHeight - 300;
      if (nearBottom) this.loadMore();
    }, 150);
    contentEl.addEventListener('scroll', this._scrollHandler);

    this.freshSearch();

    el.querySelector('#mp-install-file').onclick = () => {
      this.runInstall(() => window.buran.modrinth.installModpackFile());
    };
  },

  async freshSearch() {
    const el = document.getElementById('page-modpacks');
    const query = el.querySelector('#mp-query').value;
    const results = el.querySelector('#mp-results');

    this._searchState = { query, offset: 0, limit: 20, hasMore: true, loading: false };

    results.innerHTML = loadingHTML();
    try {
      const data = await window.buran.modrinth.searchModpacks({ query, offset: 0, limit: this._searchState.limit });
      results.innerHTML = '';
      this.appendResults(results, data.hits || []);
      this._searchState.offset = (data.hits || []).length;
      this._searchState.hasMore = (data.hits || []).length >= this._searchState.limit;
    } catch (err) {
      results.innerHTML = `<p class="subtle">${t('toast.genericError', { error: err.message })}</p>`;
      this._searchState.hasMore = false;
    }
  },

  async loadMore() {
    const state = this._searchState;
    if (!state || state.loading || !state.hasMore) return;
    state.loading = true;
    document.getElementById('mp-loading-more').style.display = 'block';
    try {
      const data = await window.buran.modrinth.searchModpacks({ query: state.query, offset: state.offset, limit: state.limit });
      const results = document.getElementById('mp-results');
      this.appendResults(results, data.hits || []);
      state.offset += (data.hits || []).length;
      state.hasMore = (data.hits || []).length >= state.limit;
    } catch (err) {
      state.hasMore = false;
    } finally {
      state.loading = false;
      document.getElementById('mp-loading-more').style.display = 'none';
    }
  },

  appendResults(container, hits) {
    if (!hits.length && !container.children.length) {
      container.innerHTML = `<p class="subtle">${t('modstore.noResults')}</p>`;
      return;
    }
    const frag = document.createElement('div');
    frag.innerHTML = hits.map((h, i) => {
      const projectId = h.project_id || h.slug;
      return `
      <div class="card" style="animation-delay:${Math.min(i * 35, 350)}ms">
        <img class="mod-card-icon" src="${h.icon_url || ''}" onerror="this.style.display='none'" />
        <div class="instance-card-name">${escapeHtml(h.title)}</div>
        <div class="instance-card-meta">${(h.downloads || 0).toLocaleString()} ${t('modstore.downloads')}</div>
        <div class="mod-row-desc" style="white-space:normal; margin:8px 0;">${escapeHtml(h.description || '')}</div>
        <button class="btn" data-install-modpack="${projectId}" data-title="${escapeHtml(h.title)}">${t('modstore.install')}</button>
      </div>
    `;
    }).join('');
    while (frag.firstChild) container.appendChild(frag.firstChild);

    container.querySelectorAll('[data-install-modpack]').forEach(btn => {
      btn.onclick = () => this.openVersionModal(btn.getAttribute('data-install-modpack'), btn.getAttribute('data-title'));
    });
  },

  /**
   * Seçilen modpack için sürüm listesini çeker ve bir seçim penceresi açar.
   * En güncel sürüm otomatik seçili gelir, ama kullanıcı dropdown'dan
   * dilediği (örn. eski bir Minecraft sürümüne ait) sürümü seçebilir.
   */
  async openVersionModal(projectId, title) {
    const box = openModal(`
      <h2>${escapeHtml(title)}</h2>
      <p class="subtle" style="margin-bottom:14px;">${t('common.loading')}</p>
    `);

    let versions;
    try {
      versions = await window.buran.modrinth.getModpackVersions(projectId);
    } catch (err) {
      box.innerHTML = `
        <h2>${escapeHtml(title)}</h2>
        <p class="subtle">${t('toast.genericError', { error: err.message })}</p>
        <div class="modal-actions"><button class="btn secondary" id="mpv-close">${t('common.close')}</button></div>
      `;
      box.querySelector('#mpv-close').onclick = closeModal;
      return;
    }

    if (!versions.length) {
      box.innerHTML = `
        <h2>${escapeHtml(title)}</h2>
        <p class="subtle">${t('modpacks.noVersions')}</p>
        <div class="modal-actions"><button class="btn secondary" id="mpv-close">${t('common.close')}</button></div>
      `;
      box.querySelector('#mpv-close').onclick = closeModal;
      return;
    }

    box.innerHTML = `
      <h2>${escapeHtml(title)}</h2>
      <div class="field">
        <label>${t('modpacks.versionLabel')}</label>
        <select id="mpv-select" style="width:100%;">
          ${versions.map((v, i) => `
            <option value="${v.id}" ${i === 0 ? 'selected' : ''}>
              ${escapeHtml(v.version_number)} — Minecraft ${(v.game_versions || []).join(', ')}
            </option>
          `).join('')}
        </select>
        <p class="subtle" style="margin-top:8px;">${t('resources.autoVersionHint')}</p>
      </div>
      <div class="modal-actions">
        <button class="btn secondary" id="mpv-cancel">${t('profiles.cancel')}</button>
        <button class="btn" id="mpv-confirm">${t('modpacks.install')}</button>
      </div>
    `;

    box.querySelector('#mpv-cancel').onclick = closeModal;
    box.querySelector('#mpv-confirm').onclick = () => {
      const versionId = box.querySelector('#mpv-select').value;
      closeModal();
      this.runInstall(() => window.buran.modrinth.installModpackFromProject(projectId, versionId));
    };
  },

  /**
   * İlerleme kartını gösterip verilen kurulum fonksiyonunu çalıştırır.
   * Hem ".mrpack dosyasından kur" hem "Modrinth projesinden kur" akışları
   * bu ortak fonksiyonu kullanır.
   */
  runInstall(installFn) {
    const progressCard = document.getElementById('mp-progress');
    const progressText = document.getElementById('mp-progress-text');
    const progressFill = document.getElementById('mp-progress-fill');
    progressCard.style.display = 'block';
    progressText.textContent = t('modpacks.step.manifest');
    progressFill.style.width = '5%';

    const offProgress = (p) => {
      const label = p.stage === 'manifest' ? t('modpacks.step.manifest')
        : p.stage === 'downloadingMods' ? t('modpacks.step.downloadingMods', { current: p.current, total: p.total })
        : p.stage === 'creatingInstance' ? t('modpacks.step.creatingInstance')
        : t('modpacks.step.done');
      progressText.textContent = label;
      const pct = p.total ? Math.round((p.current / p.total) * 100) : (p.stage === 'done' ? 100 : 10);
      progressFill.style.width = pct + '%';
    };
    window.buran.modrinth.onModpackProgress(offProgress);

    (async () => {
      try {
        const instance = await installFn();
        if (instance) showToast(t('toast.profileCreated'), 'success');
      } catch (err) {
        showToast(t('toast.downloadError', { error: err.message }), 'error');
      } finally {
        setTimeout(() => { progressCard.style.display = 'none'; }, 1200);
      }
    })();
  }
};

function debounce(fn, ms) {
  let timer;
  return (...args) => { clearTimeout(timer); timer = setTimeout(() => fn(...args), ms); };
}
