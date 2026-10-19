window.Pages = window.Pages || {};

window.Pages.modpacks = {
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
      <div id="mp-results"></div>
    `;

    const queryInput = el.querySelector('#mp-query');
    const results = el.querySelector('#mp-results');

    const doSearch = debounce(async () => {
      results.innerHTML = `<p class="subtle">${t('common.loading')}</p>`;
      try {
        const data = await window.buran.modrinth.searchModpacks({ query: queryInput.value });
        this.renderResults(results, data.hits || []);
      } catch (err) {
        results.innerHTML = `<p class="subtle">${t('toast.genericError', { error: err.message })}</p>`;
      }
    }, 350);
    queryInput.oninput = doSearch;
    doSearch();

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

  renderResults(container, hits) {
    if (!hits.length) {
      container.innerHTML = `<p class="subtle">${t('modstore.noResults')}</p>`;
      return;
    }
    container.innerHTML = `<div class="grid">${hits.map(h => `
      <div class="card">
        <div class="instance-card-name">${escapeHtml(h.title)}</div>
        <div class="instance-card-meta">${(h.downloads || 0).toLocaleString()} ${t('modstore.downloads')}</div>
        <div class="mod-row-desc" style="white-space:normal; margin:8px 0;">${escapeHtml(h.description || '')}</div>
      </div>
    `).join('')}</div>`;
  }
};
