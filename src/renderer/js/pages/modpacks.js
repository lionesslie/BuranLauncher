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

    el.querySelector('#mp-install-file').onclick = async () => {
      const progressCard = el.querySelector('#mp-progress');
      const progressText = el.querySelector('#mp-progress-text');
      const progressFill = el.querySelector('#mp-progress-fill');
      progressCard.style.display = 'block';

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

      try {
        const instance = await window.buran.modrinth.installModpackFile();
        if (instance) showToast(t('toast.profileCreated'), 'success');
      } catch (err) {
        showToast(t('toast.downloadError', { error: err.message }), 'error');
      } finally {
        setTimeout(() => { progressCard.style.display = 'none'; }, 1200);
      }
    };
  },

  async freshSearch() {
    const el = document.getElementById('page-modpacks');
    const query = el.querySelector('#mp-query').value;
    const results = el.querySelector('#mp-results');

    this._searchState = { query, offset: 0, limit: 20, hasMore: true, loading: false };

    results.innerHTML = `<p class="subtle">${t('common.loading')}</p>`;
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
    frag.innerHTML = hits.map(h => `
      <div class="card">
        <img class="mod-card-icon" src="${h.icon_url || ''}" onerror="this.style.display='none'" />
        <div class="instance-card-name">${escapeHtml(h.title)}</div>
        <div class="instance-card-meta">${(h.downloads || 0).toLocaleString()} ${t('modstore.downloads')}</div>
        <div class="mod-row-desc" style="white-space:normal; margin:8px 0;">${escapeHtml(h.description || '')}</div>
      </div>
    `).join('');
    while (frag.firstChild) container.appendChild(frag.firstChild);
  }
};

function debounce(fn, ms) {
  let timer;
  return (...args) => { clearTimeout(timer); timer = setTimeout(() => fn(...args), ms); };
}
