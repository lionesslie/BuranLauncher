window.Pages = window.Pages || {};

window.Pages.serverhost = {
  viewingServerId: null, // null => liste görünümü, aksi halde yönetim görünümü
  _consoleUnsub: false,

  async render() {
    if (this.viewingServerId) {
      await this.renderDetail(this.viewingServerId);
    } else {
      await this.renderList();
    }
  },

  async renderList() {
    const el = document.getElementById('page-serverhost');
    const servers = await window.buran.servers.list();

    el.innerHTML = `
      <div class="toolbar">
        <h1 class="page-title" style="margin:0;">${t('serverhost.title')}</h1>
        <button class="btn" id="sh-new">${t('serverhost.newServer')}</button>
      </div>
      <div class="grid" id="sh-grid"></div>
    `;

    const grid = el.querySelector('#sh-grid');
    if (!servers.length) {
      grid.innerHTML = `<p class="subtle">${t('serverhost.empty')}</p>`;
    } else {
      const runningFlags = await Promise.all(servers.map(s => window.buran.servers.isRunning(s.id)));
      grid.innerHTML = servers.map((s, i) => this.cardHtml(s, runningFlags[i], i)).join('');
      this.bindListEvents(grid);
    }

    el.querySelector('#sh-new').onclick = () => this.openCreateModal();
  },

  cardHtml(s, isRunning, index) {
    return `
      <div class="card instance-card" data-id="${s.id}" style="animation-delay:${Math.min(index * 45, 400)}ms">
        <div class="instance-card-icon" data-icon-el="server"></div>
        <div class="instance-card-name">${escapeHtml(s.name)}</div>
        <div class="instance-card-meta">${s.version} · ${s.loader}${s.loaderVersion ? ' ' + s.loaderVersion : ''}</div>
        <div class="instance-card-meta">
          <span class="badge ${isRunning ? 'success' : ''}">${isRunning ? t('serverhost.running') : t('serverhost.stopped')}</span>
        </div>
        <div class="instance-card-actions">
          ${isRunning
            ? `<button class="btn danger" data-action="stop" data-id="${s.id}">${t('profiles.stop')}</button>`
            : `<button class="btn" data-action="start" data-id="${s.id}">${t('serverhost.start')}</button>`}
          <button class="btn secondary" data-action="manage" data-id="${s.id}">${t('serverhost.manage')}</button>
          <button class="btn danger" data-action="delete" data-id="${s.id}" ${isRunning ? 'disabled' : ''}>${t('profiles.delete')}</button>
        </div>
      </div>
    `;
  },

  bindListEvents(grid) {
    grid.querySelectorAll('[data-icon-el="server"]').forEach(e => { e.innerHTML = icon('server'); });
    grid.querySelectorAll('[data-action]').forEach(btn => {
      const id = btn.getAttribute('data-id');
      const action = btn.getAttribute('data-action');
      btn.onclick = async () => {
        try {
          if (action === 'start') {
            await window.buran.servers.start(id);
            showToast(t('serverhost.started'), 'success');
            this.renderList();
          } else if (action === 'stop') {
            await window.buran.servers.stop(id);
            showToast(t('serverhost.stopping'), 'success');
            setTimeout(() => this.renderList(), 1500);
          } else if (action === 'manage') {
            this.viewingServerId = id;
            this.render();
          } else if (action === 'delete') {
            const name = btn.closest('.instance-card').querySelector('.instance-card-name').textContent;
            const ok = await confirmDialog(t('serverhost.deleteConfirm', { name }));
            if (!ok) return;
            await window.buran.servers.delete(id);
            showToast(t('toast.profileDeleted'), 'success');
            this.renderList();
          }
        } catch (err) {
          showToast(t('toast.genericError', { error: err.message }), 'error');
        }
      };
    });
  },

  async openCreateModal() {
    const versionType = 'release';
    const versions = await window.buran.versions.list(versionType);
    const box = openModal(this.formHtml({ versions, versionType }));
    this.bindForm(box);
  },

  formHtml({ versions, versionType }) {
    return `
      <h2>${t('serverhost.newServer')}</h2>
      <div class="field">
        <label>${t('profiles.nameLabel')}</label>
        <input type="text" id="sf-name" style="width:100%;" />
      </div>
      <div class="field">
        <label>${t('profiles.versionLabel')}</label>
        <div style="display:flex; gap:8px;">
          <select id="sf-version-type" style="width:140px;">
            <option value="release" selected>${t('profiles.versionType.release')}</option>
            <option value="snapshot">${t('profiles.versionType.snapshot')}</option>
          </select>
          <select id="sf-version" style="flex:1;">
            ${versions.map(v => `<option value="${v.id}">${v.id}</option>`).join('')}
          </select>
        </div>
      </div>
      <div class="field">
        <label>${t('profiles.loaderLabel')}</label>
        <select id="sf-loader" style="width:100%;">
          <option value="vanilla">${t('profiles.loaderNone')}</option>
          <option value="fabric">Fabric</option>
          <option value="quilt">Quilt</option>
          <option value="forge">Forge</option>
          <option value="neoforge">NeoForge</option>
        </select>
      </div>
      <div class="field" id="sf-loader-version-field" style="display:none;">
        <label>${t('profiles.loaderVersionLabel')}</label>
        <select id="sf-loader-version" style="width:100%;"></select>
      </div>
      <div class="field">
        <label>${t('settings.ram')}</label>
        <div style="display:flex; gap:12px;">
          <div style="flex:1;">
            <label>${t('settings.ramMin')} (MB)</label>
            <input type="number" id="sf-ram-min" value="1024" min="512" step="256" style="width:100%;" />
          </div>
          <div style="flex:1;">
            <label>${t('settings.ramMax')} (MB)</label>
            <input type="number" id="sf-ram-max" value="2048" min="1024" step="256" style="width:100%;" />
          </div>
        </div>
      </div>
      <div class="field">
        <label style="display:flex; align-items:flex-start; gap:8px; cursor:pointer;">
          <input type="checkbox" id="sf-eula" style="margin-top:3px;" />
          <span>${t('serverhost.eulaLabel')} <a href="#" id="sf-eula-link" style="color:var(--accent);">${t('serverhost.eulaLink')}</a></span>
        </label>
      </div>
      <div id="sf-progress" class="subtle" style="display:none; margin-bottom:10px;"></div>
      <div class="modal-actions">
        <button class="btn secondary" id="sf-cancel">${t('profiles.cancel')}</button>
        <button class="btn" id="sf-submit" disabled>${t('serverhost.create')}</button>
      </div>
    `;
  },

  bindForm(box) {
    const loaderSelect = box.querySelector('#sf-loader');
    const loaderVersionField = box.querySelector('#sf-loader-version-field');
    const loaderVersionSelect = box.querySelector('#sf-loader-version');
    const versionTypeSelect = box.querySelector('#sf-version-type');
    const versionSelect = box.querySelector('#sf-version');
    const eulaCheckbox = box.querySelector('#sf-eula');
    const submitBtn = box.querySelector('#sf-submit');

    box.querySelector('#sf-eula-link').onclick = (e) => {
      e.preventDefault();
      showToast('https://www.minecraft.net/en-us/eula', 'info', 6000);
    };
    eulaCheckbox.onchange = () => { submitBtn.disabled = !eulaCheckbox.checked; };

    versionTypeSelect.onchange = async () => {
      versionSelect.innerHTML = `<option>${t('common.loading')}</option>`;
      const versions = await window.buran.versions.list(versionTypeSelect.value);
      versionSelect.innerHTML = versions.map(v => `<option value="${v.id}">${v.id}</option>`).join('');
    };

    async function refreshLoaderVersions() {
      const loader = loaderSelect.value;
      if (loader === 'vanilla') { loaderVersionField.style.display = 'none'; return; }
      loaderVersionField.style.display = 'block';
      if (loader === 'fabric' || loader === 'quilt') {
        loaderVersionSelect.innerHTML = `<option>${t('common.loading')}</option>`;
        const versions = loader === 'fabric'
          ? await window.buran.loader.fabricVersions()
          : await window.buran.loader.quiltVersions();
        loaderVersionSelect.innerHTML = versions.map(v => `<option value="${v.version}">${v.version}</option>`).join('');
      } else {
        loaderVersionField.innerHTML = `
          <label>${t('profiles.loaderVersionLabel')}</label>
          <input type="text" id="sf-loader-version-text" placeholder="örn: 47.2.20" style="width:100%;" />
        `;
      }
    }
    loaderSelect.onchange = refreshLoaderVersions;

    box.querySelector('#sf-cancel').onclick = closeModal;
    submitBtn.onclick = async () => {
      const name = box.querySelector('#sf-name').value;
      const version = versionSelect.value;
      const loader = loaderSelect.value;
      let loaderVersion = null;
      if (loader !== 'vanilla') {
        const textInput = box.querySelector('#sf-loader-version-text');
        loaderVersion = textInput ? textInput.value : loaderVersionSelect.value;
      }
      const ramMin = parseInt(box.querySelector('#sf-ram-min').value, 10);
      const ramMax = parseInt(box.querySelector('#sf-ram-max').value, 10);

      const progressEl = box.querySelector('#sf-progress');
      progressEl.style.display = 'block';
      progressEl.textContent = t('serverhost.step.preparing');
      submitBtn.disabled = true;

      const offProgress = (p) => {
        progressEl.textContent = p.stage === 'resolvingInstaller' ? t('serverhost.step.resolving')
          : p.stage === 'downloading' || p.stage === 'downloadingServer' ? t('serverhost.step.downloading')
          : p.stage === 'installing' ? t('serverhost.step.installing')
          : p.stage === 'done' ? t('serverhost.step.done')
          : p.stage;
      };
      window.buran.servers.onCreateProgress(offProgress);

      try {
        await window.buran.servers.create({ name, version, loader, loaderVersion, ramMin, ramMax, eulaAccepted: eulaCheckbox.checked });
        closeModal();
        showToast(t('serverhost.created'), 'success');
        window.Pages.serverhost.renderList();
      } catch (err) {
        progressEl.style.display = 'none';
        submitBtn.disabled = false;
        showToast(t('toast.genericError', { error: err.message }), 'error');
      }
    };
  },

  // ---------------- Yönetim (detay) görünümü ----------------

  async renderDetail(serverId) {
    const el = document.getElementById('page-serverhost');
    let server;
    try {
      const servers = await window.buran.servers.list();
      server = servers.find(s => s.id === serverId);
    } catch (err) { /* aşağıda ele alınır */ }

    if (!server) {
      this.viewingServerId = null;
      return this.renderList();
    }

    const isRunning = await window.buran.servers.isRunning(serverId);
    const props = await window.buran.servers.getProperties(serverId);

    el.innerHTML = `
      <div class="toolbar">
        <button class="btn secondary" id="sd-back">← ${t('common.close')}</button>
        <h1 class="page-title" style="margin:0;">${escapeHtml(server.name)}</h1>
        <div class="toolbar-right">
          ${isRunning
            ? `<button class="btn danger" id="sd-stop">${t('profiles.stop')}</button>`
            : `<button class="btn" id="sd-start">${t('serverhost.start')}</button>`}
          <button class="btn secondary" id="sd-open-folder">${t('serverhost.openFolder')}</button>
          <button class="btn danger" id="sd-delete" ${isRunning ? 'disabled' : ''}>${t('profiles.delete')}</button>
        </div>
      </div>

      <div class="card" style="margin-bottom:16px;">
        <label>${t('serverhost.console')}</label>
        <div id="sd-console" style="background:var(--bg-0); border-radius:8px; height:260px; overflow-y:auto; padding:10px; font-family:monospace; font-size:11.5px; margin:8px 0;"></div>
        <div style="display:flex; gap:8px;">
          <input type="text" id="sd-command" placeholder="${t('serverhost.commandPlaceholder')}" style="flex:1;" ${isRunning ? '' : 'disabled'} />
          <button class="btn secondary" id="sd-send" ${isRunning ? '' : 'disabled'}>${t('serverhost.send')}</button>
        </div>
      </div>

      <div class="card" style="margin-bottom:16px;">
        <label>${t('settings.ram')}</label>
        <div style="display:flex; gap:12px; margin:8px 0;">
          <div style="flex:1;">
            <label>${t('settings.ramMin')} (MB)</label>
            <input type="number" id="sd-ram-min" value="${server.ramMin}" min="512" step="256" style="width:100%;" />
          </div>
          <div style="flex:1;">
            <label>${t('settings.ramMax')} (MB)</label>
            <input type="number" id="sd-ram-max" value="${server.ramMax}" min="1024" step="256" style="width:100%;" />
          </div>
        </div>
        <button class="btn secondary" id="sd-save-ram">${t('profiles.save')}</button>
      </div>

      <div class="card">
        <label>${t('serverhost.properties')}</label>
        <div style="display:grid; grid-template-columns:1fr 1fr; gap:12px; margin:8px 0;">
          <div><label>${t('serverhost.props.port')}</label><input type="text" id="sp-port" value="${escapeHtml(props['server-port'] || '25565')}" style="width:100%;" /></div>
          <div><label>${t('serverhost.props.maxPlayers')}</label><input type="text" id="sp-maxplayers" value="${escapeHtml(props['max-players'] || '20')}" style="width:100%;" /></div>
          <div><label>${t('serverhost.props.gamemode')}</label>
            <select id="sp-gamemode" style="width:100%;">
              ${['survival', 'creative', 'adventure', 'spectator'].map(g => `<option value="${g}" ${props.gamemode === g ? 'selected' : ''}>${g}</option>`).join('')}
            </select>
          </div>
          <div><label>${t('serverhost.props.difficulty')}</label>
            <select id="sp-difficulty" style="width:100%;">
              ${['peaceful', 'easy', 'normal', 'hard'].map(d => `<option value="${d}" ${props.difficulty === d ? 'selected' : ''}>${d}</option>`).join('')}
            </select>
          </div>
          <div style="grid-column:1/-1;"><label>${t('serverhost.props.motd')}</label><input type="text" id="sp-motd" value="${escapeHtml(props.motd || '')}" style="width:100%;" /></div>
        </div>
        <div style="display:flex; gap:16px; margin-bottom:12px;">
          <label style="display:flex; align-items:center; gap:6px;"><input type="checkbox" id="sp-pvp" ${props.pvp === 'true' ? 'checked' : ''} /> ${t('serverhost.props.pvp')}</label>
          <label style="display:flex; align-items:center; gap:6px;"><input type="checkbox" id="sp-online" ${props['online-mode'] === 'true' ? 'checked' : ''} /> ${t('serverhost.props.onlineMode')}</label>
          <label style="display:flex; align-items:center; gap:6px;"><input type="checkbox" id="sp-whitelist" ${props['white-list'] === 'true' ? 'checked' : ''} /> ${t('serverhost.props.whitelist')}</label>
        </div>
        <button class="btn secondary" id="sd-save-props">${t('profiles.save')}</button>
        <p class="subtle" style="margin-top:8px;">${t('serverhost.propsRestartHint')}</p>
      </div>
    `;

    el.querySelector('#sd-back').onclick = () => { this.viewingServerId = null; this.render(); };
    el.querySelector('#sd-open-folder').onclick = () => window.buran.servers.openFolder(serverId);
    el.querySelector('#sd-delete').onclick = async () => {
      const ok = await confirmDialog(t('serverhost.deleteConfirm', { name: server.name }));
      if (!ok) return;
      try {
        await window.buran.servers.delete(serverId);
        this.viewingServerId = null;
        showToast(t('toast.profileDeleted'), 'success');
        this.render();
      } catch (err) {
        showToast(t('toast.genericError', { error: err.message }), 'error');
      }
    };

    const startBtn = el.querySelector('#sd-start');
    if (startBtn) startBtn.onclick = async () => {
      try {
        document.getElementById('sd-console').innerHTML = '';
        await window.buran.servers.start(serverId);
        showToast(t('serverhost.started'), 'success');
        this.renderDetail(serverId);
      } catch (err) {
        showToast(t('toast.genericError', { error: err.message }), 'error');
      }
    };
    const stopBtn = el.querySelector('#sd-stop');
    if (stopBtn) stopBtn.onclick = async () => {
      try {
        await window.buran.servers.stop(serverId);
        showToast(t('serverhost.stopping'), 'success');
      } catch (err) {
        showToast(t('toast.genericError', { error: err.message }), 'error');
      }
    };

    const cmdInput = el.querySelector('#sd-command');
    const sendCmd = async () => {
      const cmd = cmdInput.value.trim();
      if (!cmd) return;
      try {
        await window.buran.servers.sendCommand(serverId, cmd);
        cmdInput.value = '';
      } catch (err) {
        showToast(t('toast.genericError', { error: err.message }), 'error');
      }
    };
    el.querySelector('#sd-send').onclick = sendCmd;
    cmdInput.onkeydown = (e) => { if (e.key === 'Enter') sendCmd(); };

    el.querySelector('#sd-save-ram').onclick = async () => {
      try {
        await window.buran.servers.update(serverId, {
          ramMin: parseInt(el.querySelector('#sd-ram-min').value, 10),
          ramMax: parseInt(el.querySelector('#sd-ram-max').value, 10)
        });
        showToast(t('toast.settingsSaved'), 'success');
      } catch (err) {
        showToast(t('toast.genericError', { error: err.message }), 'error');
      }
    };

    el.querySelector('#sd-save-props').onclick = async () => {
      try {
        await window.buran.servers.setProperties(serverId, {
          'server-port': el.querySelector('#sp-port').value,
          'max-players': el.querySelector('#sp-maxplayers').value,
          'gamemode': el.querySelector('#sp-gamemode').value,
          'difficulty': el.querySelector('#sp-difficulty').value,
          'motd': el.querySelector('#sp-motd').value,
          'pvp': String(el.querySelector('#sp-pvp').checked),
          'online-mode': String(el.querySelector('#sp-online').checked),
          'white-list': String(el.querySelector('#sp-whitelist').checked)
        });
        showToast(t('toast.settingsSaved'), 'success');
      } catch (err) {
        showToast(t('toast.genericError', { error: err.message }), 'error');
      }
    };

    this.wireConsole(serverId);
  },

  wireConsole(serverId) {
    const consoleEl = document.getElementById('sd-console');
    const appendLine = (text, cls = '') => {
      const line = document.createElement('div');
      if (cls) line.style.color = cls;
      line.textContent = text;
      consoleEl.appendChild(line);
      while (consoleEl.children.length > 500) consoleEl.removeChild(consoleEl.firstChild);
      consoleEl.scrollTop = consoleEl.scrollHeight;
    };

    window.buran.servers.onLog(({ serverId: sid, line }) => {
      if (sid !== serverId || this.viewingServerId !== serverId) return;
      const text = line.replace(/\r/g, '').trim();
      if (!text) return;
      const cls = /error|severe/i.test(text) ? '#ff8585' : /warn/i.test(text) ? '#f5c97a' : null;
      appendLine(text, cls);
    });

    window.buran.servers.onClosed(({ serverId: sid }) => {
      if (sid !== serverId || this.viewingServerId !== serverId) return;
      appendLine('--- ' + t('serverhost.stopped') + ' ---', '#7b8191');
      this.renderDetail(serverId);
    });
  }
};
