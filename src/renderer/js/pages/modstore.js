window.Pages = window.Pages || {};

window.Pages.modstore = {
  selectedInstanceId: null,
  _scrollHandler: null,
  _searchState: null,

  async render() {
    const el = document.getElementById('page-modstore');
    const instances = await window.buran.instances.list();

    el.innerHTML = `
      <h1 class="page-title">${t('modstore.title')}</h1>
      <div class="toolbar">
        <div class="toolbar-left">
          <input type="search" id="ms-query" placeholder="${t('modstore.searchPlaceholder')}" style="width:260px;" />
          <select id="ms-sort">
            <option value="relevance">${t('modstore.sort.relevance')}</option>
            <option value="downloads">${t('modstore.sort.downloads')}</option>
            <option value="newest">${t('modstore.sort.newest')}</option>
            <option value="updated">${t('modstore.sort.updated')}</option>
          </select>
        </div>
        <div class="toolbar-right">
          <select id="ms-instance">
            <option value="">${t('modstore.selectInstanceFirst')}</option>
            ${instances.map(i => `<option value="${i.id}">${escapeHtml(i.name)}</option>`).join('')}
          </select>
        </div>
      </div>
      <div id="ms-results"></div>
      <p class="subtle" id="ms-loading-more" style="text-align:center; padding:12px; display:none;">${t('common.loading')}</p>
    `;

    const results = el.querySelector('#ms-results');
    const queryInput = el.querySelector('#ms-query');
    const sortSelect = el.querySelector('#ms-sort');
    const instanceSelect = el.querySelector('#ms-instance');
    instanceSelect.value = this.selectedInstanceId || '';
    instanceSelect.onchange = () => { this.selectedInstanceId = instanceSelect.value || null; this.freshSearch(); };

    const doSearch = debounce(() => this.freshSearch(), 350);
    queryInput.oninput = doSearch;
    sortSelect.onchange = doSearch;

    // Sınırsız kaydırma: ana içerik alanı kaydırıldıkça alt sınıra yaklaşınca sonraki sayfa yüklenir
    const contentEl = document.querySelector('.content');
    if (this._scrollHandler) contentEl.removeEventListener('scroll', this._scrollHandler);
    this._scrollHandler = debounce(() => {
      if (!document.getElementById('page-modstore').classList.contains('active')) return;
      const nearBottom = contentEl.scrollTop + contentEl.clientHeight >= contentEl.scrollHeight - 300;
      if (nearBottom) this.loadMore();
    }, 150);
    contentEl.addEventListener('scroll', this._scrollHandler);

    this.freshSearch();
  },

  async freshSearch() {
    const el = document.getElementById('page-modstore');
    const queryInput = el.querySelector('#ms-query');
    const sortSelect = el.querySelector('#ms-sort');
    const results = el.querySelector('#ms-results');
    const instances = await window.buran.instances.list();
    const instance = instances.find(i => i.id === this.selectedInstanceId);

    this._searchState = {
      query: queryInput.value,
      sort: sortSelect.value,
      loader: instance && instance.loader !== 'vanilla' ? instance.loader : null,
      gameVersion: instance ? instance.version : null,
      offset: 0,
      limit: 20,
      hasMore: true,
      loading: false,
      installedProjectIds: instance ? new Set((await window.buran.modrinth.listInstalled(instance.id)).map(m => m.projectId).filter(Boolean)) : new Set()
    };

    results.innerHTML = `<p class="subtle">${t('common.loading')}</p>`;
    try {
      const data = await window.buran.modrinth.search({ ...this._searchState, limit: this._searchState.limit, offset: 0 });
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
    document.getElementById('ms-loading-more').style.display = 'block';
    try {
      const data = await window.buran.modrinth.search({ ...state, offset: state.offset });
      const results = document.getElementById('ms-results');
      this.appendResults(results, data.hits || []);
      state.offset += (data.hits || []).length;
      state.hasMore = (data.hits || []).length >= state.limit;
    } catch (err) {
      state.hasMore = false;
    } finally {
      state.loading = false;
      document.getElementById('ms-loading-more').style.display = 'none';
    }
  },

  appendResults(container, hits) {
    if (!hits.length && !container.children.length) {
      container.innerHTML = `<p class="subtle">${t('modstore.noResults')}</p>`;
      return;
    }
    const installedIds = (this._searchState && this._searchState.installedProjectIds) || new Set();
    const frag = document.createElement('div');
    frag.innerHTML = hits.map(h => {
      const projectId = h.project_id || h.slug;
      const isInstalled = installedIds.has(projectId);
      return `
      <div class="mod-row">
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
          await window.buran.modrinth.installMod(projectId, this.selectedInstanceId, null);
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
