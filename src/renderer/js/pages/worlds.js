window.Pages = window.Pages || {};

window.Pages.worlds = {
  selectedInstanceId: null,

  async render() {
    const el = document.getElementById('page-worlds');
    const instances = await window.buran.instances.list();

    el.innerHTML = `
      <h1 class="page-title">${t('worlds.title')}</h1>
      <div class="toolbar">
        <div class="toolbar-left">
          <select id="w-instance" style="min-width:220px;">
            <option value="">${t('modstore.selectInstanceFirst')}</option>
            ${instances.map(i => `<option value="${i.id}">${escapeHtml(i.name)}</option>`).join('')}
          </select>
        </div>
        <div class="toolbar-right">
          <button class="btn secondary" id="w-import" disabled>${t('worlds.import')}</button>
        </div>
      </div>
      <div class="grid" id="w-results"></div>
    `;

    const instanceSelect = el.querySelector('#w-instance');
    const importBtn = el.querySelector('#w-import');
    instanceSelect.value = this.selectedInstanceId || '';

    const load = async () => {
      this.selectedInstanceId = instanceSelect.value || null;
      importBtn.disabled = !this.selectedInstanceId;
      const results = document.getElementById('w-results');
      if (!this.selectedInstanceId) {
        results.innerHTML = `<p class="subtle">${t('modstore.selectInstanceFirst')}</p>`;
        return;
      }
      results.innerHTML = loadingHTML();
      try {
        const worlds = await window.buran.worlds.list(this.selectedInstanceId);
        this.draw(results, worlds);
      } catch (err) {
        results.innerHTML = `<p class="subtle">${t('toast.genericError', { error: err.message })}</p>`;
      }
    };

    instanceSelect.onchange = load;
    importBtn.onclick = async () => {
      try {
        const name = await window.buran.worlds.import(this.selectedInstanceId);
        if (name) { showToast(t('worlds.imported'), 'success'); load(); }
      } catch (err) {
        showToast(t('toast.genericError', { error: err.message }), 'error');
      }
    };

    await load();
  },

  draw(container, worlds) {
    if (!worlds.length) {
      container.innerHTML = `<p class="subtle">${t('worlds.empty')}</p>`;
      return;
    }
    container.innerHTML = worlds.map((w, i) => `
      <div class="card" style="animation-delay:${Math.min(i * 40, 400)}ms">
        <div class="instance-card-icon" style="${w.icon ? `background-image:url('${w.icon}'); background-size:cover; background-position:center;` : ''}">${w.icon ? '' : icon('box')}</div>
        <div class="instance-card-name">${escapeHtml(w.name)}</div>
        <div class="instance-card-meta">${formatBytes(w.sizeBytes)}</div>
        <div class="instance-card-meta">${w.lastModified ? new Date(w.lastModified).toLocaleString() : ''}</div>
        <div class="instance-card-actions">
          <button class="btn secondary" data-action="backup" data-name="${escapeHtml(w.folderName)}">${t('worlds.backup')}</button>
          <button class="btn secondary" data-action="open" data-name="${escapeHtml(w.folderName)}">${t('worlds.openFolder')}</button>
          <button class="btn danger" data-action="delete" data-name="${escapeHtml(w.folderName)}">${t('worlds.delete')}</button>
        </div>
      </div>
    `).join('');

    container.querySelectorAll('[data-action]').forEach(btn => {
      const folderName = btn.getAttribute('data-name');
      const action = btn.getAttribute('data-action');
      btn.onclick = async () => {
        try {
          if (action === 'backup') {
            const dest = await window.buran.worlds.backup(this.selectedInstanceId, folderName);
            if (dest) showToast(dest, 'success');
          } else if (action === 'open') {
            await window.buran.worlds.openFolder(this.selectedInstanceId, folderName);
          } else if (action === 'delete') {
            const ok = await confirmDialog(t('worlds.deleteConfirm', { name: folderName }));
            if (!ok) return;
            await window.buran.worlds.delete(this.selectedInstanceId, folderName);
            showToast(t('worlds.deleted'), 'success');
            const freshWorlds = await window.buran.worlds.list(this.selectedInstanceId);
            this.draw(container, freshWorlds);
          }
        } catch (err) {
          showToast(t('toast.genericError', { error: err.message }), 'error');
        }
      };
    });
  }
};

function formatBytes(bytes) {
  if (!bytes) return '0 MB';
  const mb = bytes / (1024 * 1024);
  if (mb < 1024) return mb.toFixed(1) + ' MB';
  return (mb / 1024).toFixed(2) + ' GB';
}
