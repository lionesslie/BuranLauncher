window.Pages = window.Pages || {};

window.Pages.profiles = {
  async render() {
    const el = document.getElementById('page-profiles');
    const instances = await window.buran.instances.list();

    el.innerHTML = `
      <div class="toolbar">
        <h1 class="page-title" style="margin:0;">${t('profiles.title')}</h1>
        <div class="toolbar-right">
          <button class="btn secondary" id="profiles-import">${t('profiles.import')}</button>
          <button class="btn" id="profiles-new">${t('profiles.newProfile')}</button>
        </div>
      </div>
      <div class="grid" id="profiles-grid"></div>
    `;

    const grid = el.querySelector('#profiles-grid');
    grid.innerHTML = instances.map((inst, i) => this.cardHtml(inst, i)).join('') || `<p class="subtle">${t('home.noInstances')}</p>`;
    this.bindCardEvents(grid);

    el.querySelector('#profiles-new').onclick = () => this.openCreateModal();
    el.querySelector('#profiles-import').onclick = async () => {
      try {
        const result = await window.buran.instances.import();
        if (result) { showToast(t('toast.profileCreated'), 'success'); this.render(); }
      } catch (err) { showToast(t('toast.genericError', { error: err.message }), 'error'); }
    };
  },

  cardHtml(inst, index = 0) {
    const lastPlayed = inst.lastPlayed ? new Date(inst.lastPlayed).toLocaleDateString() : t('profiles.never');
    const hours = Math.floor((inst.totalPlaytimeMs || 0) / 3600000);
    const isRunning = window.runningInstances && window.runningInstances.has(inst.id);
    return `
      <div class="card instance-card" data-id="${inst.id}" style="animation-delay:${Math.min(index * 45, 400)}ms">
        <div class="instance-card-icon instance-card-icon-grass"></div>
        <div class="instance-card-name">${escapeHtml(inst.name)}</div>
        <div class="instance-card-meta">${inst.version} · ${inst.loader}${inst.loaderVersion ? ' ' + inst.loaderVersion : ''}</div>
        <div class="instance-card-meta">${t('profiles.lastPlayed')}: ${lastPlayed} · ${t('profiles.totalPlaytime')}: ${hours} sa</div>
        <div class="instance-card-actions">
          ${isRunning
            ? `<button class="btn danger" data-action="stop" data-id="${inst.id}">${t('profiles.stop')}</button>`
            : `<button class="btn" data-action="play" data-id="${inst.id}">${t('profiles.play')}</button>`}
          <button class="btn secondary" data-action="edit" data-id="${inst.id}" ${isRunning ? 'disabled' : ''}>${t('profiles.edit')}</button>
          <button class="btn secondary" data-action="duplicate" data-id="${inst.id}">${t('profiles.duplicate')}</button>
          <button class="btn secondary" data-action="export" data-id="${inst.id}">${t('profiles.export')}</button>
          <button class="btn danger" data-action="delete" data-id="${inst.id}" ${isRunning ? 'disabled' : ''}>${t('profiles.delete')}</button>
        </div>
      </div>
    `;
  },

  bindCardEvents(container) {
    container.querySelectorAll('[data-action]').forEach(btn => {
      const id = btn.getAttribute('data-id');
      const action = btn.getAttribute('data-action');
      btn.onclick = async () => {
        try {
          if (action === 'play') return this.playInstance(id);
          if (action === 'stop') return this.stopInstance(id);
          if (action === 'edit') return this.openEditModal(id);
          if (action === 'duplicate') {
            await window.buran.instances.duplicate(id);
            showToast(t('toast.profileCreated'), 'success');
            return this.render();
          }
          if (action === 'export') {
            const dest = await window.buran.instances.export(id);
            if (dest) showToast(dest, 'success');
            return;
          }
          if (action === 'delete') {
            const ok = await confirmDialog(t('profiles.deleteConfirm', { name: btn.closest('.instance-card').querySelector('.instance-card-name').textContent }));
            if (!ok) return;
            await window.buran.instances.delete(id);
            showToast(t('toast.profileDeleted'), 'success');
            return this.render();
          }
        } catch (err) {
          showToast(t('toast.genericError', { error: err.message }), 'error');
        }
      };
    });
  },

  async playInstance(id) {
    try {
      showToast(t('toast.launchStarted'));
      document.getElementById('log-panel').classList.add('open');
      window.runningInstances.add(id);
      this.render(); // "Oyna" düğmesini hemen "Durdur"a çevir
      await window.buran.launch.start(id);
    } catch (err) {
      window.runningInstances.delete(id);
      showToast(t('toast.launchError', { error: err.message }), 'error');
      this.render();
    }
  },

  async stopInstance(id) {
    try {
      await window.buran.launch.kill(id);
      window.runningInstances.delete(id);
      showToast(t('toast.gameStopped'), 'success');
      this.render();
    } catch (err) {
      showToast(t('toast.genericError', { error: err.message }), 'error');
    }
  },

  async openCreateModal() {
    const versionType = 'release';
    const versions = await window.buran.versions.list(versionType);
    const box = openModal(this.formHtml({ versions, versionType }));
    this.bindForm(box, null);
  },

  async openEditModal(id) {
    const instance = (await window.buran.instances.list()).find(i => i.id === id);
    const versionType = instance.versionType || 'release';
    const versions = await window.buran.versions.list(versionType);
    const box = openModal(this.formHtml({ versions, versionType, instance }));
    this.bindForm(box, instance);
  },

  formHtml({ versions, versionType, instance }) {
    const isEdit = !!instance;
    return `
      <h2>${isEdit ? t('profiles.edit') : t('profiles.newProfile')}</h2>
      <div class="field">
        <label>${t('profiles.nameLabel')}</label>
        <input type="text" id="f-name" value="${instance ? escapeHtml(instance.name) : ''}" style="width:100%;" />
      </div>
      <div class="field">
        <label>${t('profiles.versionLabel')}</label>
        <div style="display:flex; gap:8px;">
          <select id="f-version-type" style="width:140px;">
            <option value="release" ${versionType === 'release' ? 'selected' : ''}>${t('profiles.versionType.release')}</option>
            <option value="snapshot" ${versionType === 'snapshot' ? 'selected' : ''}>${t('profiles.versionType.snapshot')}</option>
            <option value="old_beta" ${versionType === 'old_beta' ? 'selected' : ''}>${t('profiles.versionType.old_beta')}</option>
            <option value="old_alpha" ${versionType === 'old_alpha' ? 'selected' : ''}>${t('profiles.versionType.old_alpha')}</option>
          </select>
          <select id="f-version" style="flex:1;">
            ${versions.map(v => `<option value="${v.id}" ${instance && instance.version === v.id ? 'selected' : ''}>${v.id}</option>`).join('')}
          </select>
        </div>
      </div>
      <div class="field">
        <label>${t('profiles.loaderLabel')}</label>
        <select id="f-loader" style="width:100%;">
          <option value="vanilla" ${instance && instance.loader === 'vanilla' ? 'selected' : ''}>${t('profiles.loaderNone')}</option>
          <option value="fabric" ${instance && instance.loader === 'fabric' ? 'selected' : ''}>Fabric</option>
          <option value="quilt" ${instance && instance.loader === 'quilt' ? 'selected' : ''}>Quilt</option>
          <option value="forge" ${instance && instance.loader === 'forge' ? 'selected' : ''}>Forge</option>
          <option value="neoforge" ${instance && instance.loader === 'neoforge' ? 'selected' : ''}>NeoForge</option>
        </select>
      </div>
      <div class="field" id="loader-version-field" style="display:none;">
        <label>${t('profiles.loaderVersionLabel')}</label>
        <select id="f-loader-version" style="width:100%;"></select>
      </div>
      <div class="modal-actions">
        <button class="btn secondary" id="form-cancel">${t('profiles.cancel')}</button>
        <button class="btn" id="form-submit">${isEdit ? t('profiles.save') : t('profiles.create')}</button>
      </div>
    `;
  },

  bindForm(box, instance) {
    const loaderSelect = box.querySelector('#f-loader');
    const loaderVersionField = box.querySelector('#loader-version-field');
    const loaderVersionSelect = box.querySelector('#f-loader-version');
    const versionSelect = box.querySelector('#f-version');
    const versionTypeSelect = box.querySelector('#f-version-type');

    versionTypeSelect.onchange = async () => {
      versionSelect.innerHTML = `<option>${t('common.loading')}</option>`;
      const versions = await window.buran.versions.list(versionTypeSelect.value);
      versionSelect.innerHTML = versions.map(v => `<option value="${v.id}">${v.id}</option>`).join('');
    };

    async function refreshLoaderVersions() {
      const loader = loaderSelect.value;
      if (loader === 'vanilla') { loaderVersionField.style.display = 'none'; return; }
      loaderVersionField.style.display = 'block';
      loaderVersionSelect.innerHTML = `<option>${t('common.loading')}</option>`;
      try {
        if (loader === 'fabric') {
          const versions = await window.buran.loader.fabricVersions();
          loaderVersionSelect.innerHTML = versions.map(v => `<option value="${v.version}">${v.version}</option>`).join('');
        } else if (loader === 'quilt') {
          const versions = await window.buran.loader.quiltVersions();
          loaderVersionSelect.innerHTML = versions.map(v => `<option value="${v.version}">${v.version}</option>`).join('');
        } else {
          loaderVersionSelect.innerHTML = `<option value="">${t('profiles.loaderVersionLabel')} (${versionSelect.value})</option>`;
          loaderVersionSelect.innerHTML = `<input-fallback>`;
          loaderVersionField.innerHTML = `
            <label>${t('profiles.loaderVersionLabel')}</label>
            <input type="text" id="f-loader-version-text" placeholder="örn: 47.2.20" style="width:100%;" />
          `;
        }
        if (instance && instance.loaderVersion && loaderVersionSelect.querySelector) {
          const opt = Array.from(loaderVersionSelect.options || []).find(o => o.value === instance.loaderVersion);
          if (opt) loaderVersionSelect.value = instance.loaderVersion;
        }
      } catch (err) {
        loaderVersionField.innerHTML = `<p class="subtle">${t('toast.genericError', { error: err.message })}</p>`;
      }
    }

    loaderSelect.onchange = refreshLoaderVersions;
    if (instance && instance.loader !== 'vanilla') refreshLoaderVersions();

    box.querySelector('#form-cancel').onclick = closeModal;
    box.querySelector('#form-submit').onclick = async () => {
      const name = box.querySelector('#f-name').value;
      const version = versionSelect.value;
      const loader = loaderSelect.value;
      let loaderVersion = null;
      if (loader !== 'vanilla') {
        const textInput = box.querySelector('#f-loader-version-text');
        loaderVersion = textInput ? textInput.value : loaderVersionSelect.value;
      }

      try {
        const payload = { name, version, versionType: versionTypeSelect.value, loader, loaderVersion };
        let targetId;
        if (instance) {
          await window.buran.instances.update(instance.id, payload);
          targetId = instance.id;
        } else {
          const created = await window.buran.instances.create(payload);
          targetId = created.id;
        }

        if (loader !== 'vanilla' && loaderVersion) {
          try {
            await window.buran.loader.install({ type: loader, mcVersion: version, loaderVersion });
          } catch (err) {
            showToast(t('toast.genericError', { error: err.message }), 'error');
          }
        }

        closeModal();
        showToast(t('toast.profileCreated'), 'success');
        window.Pages.profiles.render();
      } catch (err) {
        showToast(t('toast.genericError', { error: err.message }), 'error');
      }
    };
  }
};
