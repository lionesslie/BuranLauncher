window.Pages = window.Pages || {};

window.Pages.installedmods = {
  selectedInstanceId: null,

  async render() {
    const el = document.getElementById('page-installedmods');
    const instances = await window.buran.instances.list();

    el.innerHTML = `
      <h1 class="page-title">${t('installedMods.title')}</h1>
      <div class="toolbar">
        <div class="toolbar-left">
          <select id="im-instance" style="min-width:220px;">
            <option value="">${t('modstore.selectInstanceFirst')}</option>
            ${instances.map(i => `<option value="${i.id}">${escapeHtml(i.name)}</option>`).join('')}
          </select>
        </div>
        <div class="toolbar-right">
          <button class="btn secondary" id="im-check-updates" disabled>${t('modstore.checkUpdates')}</button>
        </div>
      </div>
      <div id="im-list"></div>
    `;

    const instanceSelect = el.querySelector('#im-instance');
    const checkBtn = el.querySelector('#im-check-updates');
    instanceSelect.value = this.selectedInstanceId || '';

    const load = async () => {
      this.selectedInstanceId = instanceSelect.value || null;
      checkBtn.disabled = !this.selectedInstanceId;
      if (!this.selectedInstanceId) {
        document.getElementById('im-list').innerHTML = `<p class="subtle">${t('modstore.selectInstanceFirst')}</p>`;
        return;
      }
      const mods = await window.buran.modrinth.listInstalled(this.selectedInstanceId);
      this.draw(mods, {});
    };

    instanceSelect.onchange = load;
    checkBtn.onclick = async () => {
      const list = document.getElementById('im-list');
      list.innerHTML = loadingHTML();
      try {
        const mods = await window.buran.modrinth.listInstalled(this.selectedInstanceId);
        const updates = await window.buran.modrinth.checkUpdates(this.selectedInstanceId);
        const updateMap = {};
        updates.forEach(u => { updateMap[u.projectId] = u; });
        this.draw(mods, updateMap);
        if (!updates.length) showToast(t('modstore.upToDate'), 'success');
      } catch (err) {
        showToast(t('toast.genericError', { error: err.message }), 'error');
      }
    };

    await load();
  },

  draw(mods, updateMap) {
    const list = document.getElementById('im-list');
    if (!mods.length) {
      list.innerHTML = `<p class="subtle">${t('modstore.noResults')}</p>`;
      return;
    }
    list.innerHTML = mods.map((m, i) => {
      const update = m.projectId ? updateMap[m.projectId] : null;
      return `
        <div class="mod-row" style="animation-delay:${Math.min(i * 30, 300)}ms">
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
          await window.buran.modrinth.updateMod(window.Pages.installedmods.selectedInstanceId, btn.getAttribute('data-update'));
          showToast(t('toast.modUpdated'), 'success');
          const mods = await window.buran.modrinth.listInstalled(window.Pages.installedmods.selectedInstanceId);
          window.Pages.installedmods.draw(mods, {});
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
          await window.buran.modrinth.toggleMod(window.Pages.installedmods.selectedInstanceId, btn.getAttribute('data-toggle'), !enabled);
          const mods = await window.buran.modrinth.listInstalled(window.Pages.installedmods.selectedInstanceId);
          window.Pages.installedmods.draw(mods, {});
        } catch (err) {
          showToast(t('toast.genericError', { error: err.message }), 'error');
        }
      };
    });
    list.querySelectorAll('[data-remove]').forEach(btn => {
      btn.onclick = async () => {
        try {
          await window.buran.modrinth.removeMod(window.Pages.installedmods.selectedInstanceId, btn.getAttribute('data-remove'));
          showToast(t('toast.modRemoved'), 'success');
          const mods = await window.buran.modrinth.listInstalled(window.Pages.installedmods.selectedInstanceId);
          window.Pages.installedmods.draw(mods, {});
        } catch (err) {
          showToast(t('toast.genericError', { error: err.message }), 'error');
        }
      };
    });
  }
};
