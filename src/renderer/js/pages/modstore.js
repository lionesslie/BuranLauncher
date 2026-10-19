window.Pages = window.Pages || {};

window.Pages.modstore = {
  selectedInstanceId: null,

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
      <div class="card" id="ms-installed-card" style="margin-bottom:18px; display:none;">
        <div class="toolbar" style="margin-bottom:10px;">
          <strong>${t('modstore.installedTitle')}</strong>
          <button class="btn secondary" id="ms-check-updates">${t('modstore.checkUpdates')}</button>
        </div>
        <div id="ms-installed-list"></div>
      </div>

      <div id="ms-results"></div>
    `;

    const results = el.querySelector('#ms-results');
    const queryInput = el.querySelector('#ms-query');
    const sortSelect = el.querySelector('#ms-sort');
    const instanceSelect = el.querySelector('#ms-instance');
    instanceSelect.value = this.selectedInstanceId || '';
    instanceSelect.onchange = () => {
      this.selectedInstanceId = instanceSelect.value || null;
      this.renderInstalledSection();
    };

    this.renderInstalledSection();

    const doSearch = debounce(async () => {
      results.innerHTML = `<p class="subtle">${t('common.loading')}</p>`;
      try {
        const instance = instances.find(i => i.id === this.selectedInstanceId);
        const data = await window.buran.modrinth.search({
          query: queryInput.value,
          projectType: 'mod',
          sort: sortSelect.value,
          loader: instance && instance.loader !== 'vanilla' ? instance.loader : null,
          gameVersion: instance ? instance.version : null
        });
        this.renderResults(results, data.hits || []);
      } catch (err) {
        results.innerHTML = `<p class="subtle">${t('toast.genericError', { error: err.message })}</p>`;
      }
    }, 350);

    queryInput.oninput = doSearch;
    sortSelect.onchange = doSearch;
    doSearch();
  },

  async renderInstalledSection() {
    const card = document.getElementById('ms-installed-card');
    const list = document.getElementById('ms-installed-list');
    if (!this.selectedInstanceId) { card.style.display = 'none'; return; }

    card.style.display = 'block';
    const mods = await window.buran.modrinth.listInstalled(this.selectedInstanceId);
    this.drawInstalledList(list, mods, {});

    document.getElementById('ms-check-updates').onclick = async () => {
      list.innerHTML = `<p class="subtle">${t('common.loading')}</p>`;
      try {
        const updates = await window.buran.modrinth.checkUpdates(this.selectedInstanceId);
        const updateMap = {};
        updates.forEach(u => { updateMap[u.projectId] = u; });
        this.drawInstalledList(list, mods, updateMap);
        if (!updates.length) showToast(t('modstore.upToDate'), 'success');
      } catch (err) {
        showToast(t('toast.genericError', { error: err.message }), 'error');
      }
    };
  },

  drawInstalledList(list, mods, updateMap) {
    if (!mods.length) {
      list.innerHTML = `<p class="subtle">${t('modstore.noResults')}</p>`;
      return;
    }
    list.innerHTML = mods.map(m => {
      const update = m.projectId ? updateMap[m.projectId] : null;
      return `
        <div class="mod-row">
          <div class="mod-row-info">
            <div class="mod-row-title">${escapeHtml(m.title)}</div>
            <div class="mod-row-desc">${m.versionNumber ? m.versionNumber : m.filename}</div>
          </div>
          ${update ? `<span class="badge">${t('modstore.updateAvailable', { version: update.latestVersionNumber })}</span>` : ''}
          ${update ? `<button class="btn" data-update="${m.projectId}">${t('modstore.update')}</button>` : ''}
          <button class="btn secondary" data-toggle="${m.filename}" data-enabled="${m.enabled}">${m.enabled ? t('modstore.disable') : t('modstore.enable')}</button>
          <button class="btn danger" data-remove="${m.filename}">${t('modstore.remove')}</button>
        </div>
      `;
    }).join('');

    list.querySelectorAll('[data-update]').forEach(btn => {
      btn.onclick = async () => {
        btn.disabled = true;
        try {
          await window.buran.modrinth.updateMod(window.Pages.modstore.selectedInstanceId, btn.getAttribute('data-update'));
          showToast(t('toast.modUpdated'), 'success');
          window.Pages.modstore.renderInstalledSection();
        } catch (err) {
          btn.disabled = false;
          showToast(t('toast.downloadError', { error: err.message }), 'error');
        }
      };
    });
    list.querySelectorAll('[data-toggle]').forEach(btn => {
      btn.onclick = async () => {
        const enabled = btn.getAttribute('data-enabled') === 'true';
        try {
          await window.buran.modrinth.toggleMod(window.Pages.modstore.selectedInstanceId, btn.getAttribute('data-toggle'), !enabled);
          window.Pages.modstore.renderInstalledSection();
        } catch (err) {
          showToast(t('toast.genericError', { error: err.message }), 'error');
        }
      };
    });
    list.querySelectorAll('[data-remove]').forEach(btn => {
      btn.onclick = async () => {
        try {
          await window.buran.modrinth.removeMod(window.Pages.modstore.selectedInstanceId, btn.getAttribute('data-remove'));
          showToast(t('toast.modRemoved'), 'success');
          window.Pages.modstore.renderInstalledSection();
        } catch (err) {
          showToast(t('toast.genericError', { error: err.message }), 'error');
        }
      };
    });
  },

  renderResults(container, hits) {
    if (!hits.length) {
      container.innerHTML = `<p class="subtle">${t('modstore.noResults')}</p>`;
      return;
    }
    container.innerHTML = hits.map(h => `
      <div class="mod-row">
        <div class="mod-row-info">
          <div class="mod-row-title">${escapeHtml(h.title)}</div>
          <div class="mod-row-desc">${escapeHtml(h.description || '')}</div>
        </div>
        <span class="badge">${h.downloads ? h.downloads.toLocaleString() : 0} ${t('modstore.downloads')}</span>
        <button class="btn" data-install="${h.project_id || h.slug}">${t('modstore.install')}</button>
      </div>
    `).join('');

    container.querySelectorAll('[data-install]').forEach(btn => {
      btn.onclick = async () => {
        if (!this.selectedInstanceId) { showToast(t('modstore.selectInstanceFirst'), 'error'); return; }
        const projectId = btn.getAttribute('data-install');
        btn.disabled = true;
        btn.textContent = '...';
        try {
          await window.buran.modrinth.installMod(projectId, this.selectedInstanceId, null);
          btn.textContent = t('modstore.installed');
          showToast(t('toast.modInstalled'), 'success');
          this.renderInstalledSection();
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
