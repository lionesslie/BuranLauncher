window.Pages = window.Pages || {};

window.Pages.resources = {
  selectedInstanceId: null,
  activeType: 'resourcepack', // 'resourcepack' | 'shader'
  _scrollHandler: null,
  _searchState: null,

  async render() {
    const el = document.getElementById('page-resources');
    const instances = await window.buran.instances.list();

    el.innerHTML = `
      <h1 class="page-title">${t('resources.title')}</h1>
      <div class="toolbar">
        <div class="toolbar-left">
          <div class="type-switch">
            <button class="type-switch-btn ${this.activeType === 'resourcepack' ? 'active' : ''}" data-type="resourcepack">${t('resources.typeResourcepack')}</button>
            <button class="type-switch-btn ${this.activeType === 'shader' ? 'active' : ''}" data-type="shader">${t('resources.typeShader')}</button>
          </div>
          <input type="search" id="rs-query" placeholder="${t('resources.searchPlaceholder')}" style="width:240px;" />
        </div>
        <div class="toolbar-right">
          <select id="rs-instance">
            <option value="">${t('modstore.selectInstanceFirst')}</option>
            ${instances.map(i => `<option value="${i.id}">${escapeHtml(i.name)}</option>`).join('')}
          </select>
        </div>
      </div>
      <p class="subtle" style="margin:-8px 0 16px;">${t('resources.autoVersionHint')}</p>
      <div id="rs-results"></div>
      <p class="subtle" id="rs-loading-more" style="text-align:center; padding:12px; display:none;">${t('common.loading')}</p>
    `;

    const queryInput = el.querySelector('#rs-query');
    const instanceSelect = el.querySelector('#rs-instance');
    instanceSelect.value = this.selectedInstanceId || '';
    instanceSelect.onchange = () => { this.selectedInstanceId = instanceSelect.value || null; this.freshSearch(); };

    el.querySelectorAll('.type-switch-btn').forEach(btn => {
      btn.onclick = () => {
        this.activeType = btn.getAttribute('data-type');
        el.querySelectorAll('.type-switch-btn').forEach(b => b.classList.remove('active'));
        btn.classList.add('active');
        this.freshSearch();
      };
    });

    const doSearch = debounce(() => this.freshSearch(), 350);
    queryInput.oninput = doSearch;

    const contentEl = document.querySelector('.content');
    if (this._scrollHandler) contentEl.removeEventListener('scroll', this._scrollHandler);
    this._scrollHandler = debounce(() => {
      if (!document.getElementById('page-resources').classList.contains('active')) return;
      const nearBottom = contentEl.scrollTop + contentEl.clientHeight >= contentEl.scrollHeight - 300;
      if (nearBottom) this.loadMore();
    }, 150);
    contentEl.addEventListener('scroll', this._scrollHandler);

    this.freshSearch();
  },

  async freshSearch() {
    const el = document.getElementById('page-resources');
    const query = el.querySelector('#rs-query').value;
    const results = el.querySelector('#rs-results');
    const instances = await window.buran.instances.list();
    const instance = instances.find(i => i.id === this.selectedInstanceId);

    this._searchState = {
      query,
      projectType: this.activeType,
      gameVersion: instance ? instance.version : null,
      offset: 0,
      limit: 20,
      hasMore: true,
      loading: false,
      installedProjectIds: instance
        ? new Set((await window.buran.modrinth.listInstalledContent(instance.id, this.activeType)).map(m => m.projectId).filter(Boolean))
        : new Set()
    };

    results.innerHTML = loadingHTML();
    try {
      const data = await window.buran.modrinth.searchContent({ ...this._searchState });
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
    document.getElementById('rs-loading-more').style.display = 'block';
    try {
      const data = await window.buran.modrinth.searchContent({ ...state });
      const results = document.getElementById('rs-results');
      this.appendResults(results, data.hits || []);
      state.offset += (data.hits || []).length;
      state.hasMore = (data.hits || []).length >= state.limit;
    } catch (err) {
      state.hasMore = false;
    } finally {
      state.loading = false;
      document.getElementById('rs-loading-more').style.display = 'none';
    }
  },

  appendResults(container, hits) {
    if (!hits.length && !container.children.length) {
      container.innerHTML = `<p class="subtle">${t('modstore.noResults')}</p>`;
      return;
    }
    const installedIds = (this._searchState && this._searchState.installedProjectIds) || new Set();
    const frag = document.createElement('div');
    frag.innerHTML = hits.map((h, i) => {
      const projectId = h.project_id || h.slug;
      const isInstalled = installedIds.has(projectId);
      return `
      <div class="mod-row" style="animation-delay:${Math.min(i * 30, 300)}ms">
        <img class="mod-row-icon" src="${h.icon_url || ''}" onerror="this.style.visibility='hidden'" />
        <div class="mod-row-info">
          <div class="mod-row-title">${escapeHtml(h.title)}</div>
          <div class="mod-row-desc">${escapeHtml(h.description || '')}</div>
        </div>
        <span class="badge">${h.downloads ? h.downloads.toLocaleString() : 0} ${t('modstore.downloads')}</span>
        ${isInstalled
          ? `<span class="badge success">${t('modstore.installed')}</span>`
          : `<button class="btn" data-install="${projectId}">${t('modstore.install')}</button>`}
      </div>
    `;
    }).join('');

    while (frag.firstChild) container.appendChild(frag.firstChild);

    container.querySelectorAll('[data-install]').forEach(btn => {
      btn.onclick = async () => {
        if (!this.selectedInstanceId) { showToast(t('modstore.selectInstanceFirst'), 'error'); return; }
        const projectId = btn.getAttribute('data-install');
        btn.disabled = true;
        btn.textContent = '...';
        try {
          // Sürüm seçimi tamamen otomatik: backend her zaman profilin
          // Minecraft sürümüyle uyumlu EN GÜNCEL sürümü seçer.
          await window.buran.modrinth.installContent(projectId, this.selectedInstanceId, this.activeType);
          btn.textContent = t('modstore.installed');
          btn.classList.add('secondary');
          if (this._searchState) this._searchState.installedProjectIds.add(projectId);
          showToast(t('toast.modInstalled'), 'success');
        } catch (err) {
          btn.disabled = false;
          btn.textContent = t('modstore.install');
          showToast(t('toast.downloadError', { error: err.message }), 'error');
        }
      };
    });
  }
};

function debounce(fn, ms) {
  let timer;
  return (...args) => { clearTimeout(timer); timer = setTimeout(() => fn(...args), ms); };
}
