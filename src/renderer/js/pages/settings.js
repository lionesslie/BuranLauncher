window.Pages = window.Pages || {};

window.Pages.settings = {
  async render() {
    const el = document.getElementById('page-settings');
    const settings = await window.buran.settings.get();
    const currentLang = I18N_STATE.lang;

    el.innerHTML = `
      <h1 class="page-title">${t('settings.title')}</h1>

      <div class="card" style="margin-bottom:16px;">
        <label>${t('settings.language')}</label>
        <select id="s-lang" style="width:200px;">
          <option value="tr" ${currentLang === 'tr' ? 'selected' : ''}>Türkçe</option>
          <option value="ru" ${currentLang === 'ru' ? 'selected' : ''}>Русский</option>
        </select>
      </div>

      <div class="card" style="margin-bottom:16px;">
        <label>${t('settings.updates')}</label>
        <p class="subtle" style="margin:4px 0 10px;">${t('settings.currentVersion')}: v${await window.buran.updater.getVersion()}</p>
        <div id="update-status" class="subtle" style="margin-bottom:10px; min-height:16px;"></div>
        <div style="display:flex; gap:8px;">
          <button class="btn secondary" id="s-check-update">${t('settings.checkUpdates')}</button>
          <button class="btn" id="s-download-update" style="display:none;">${t('settings.downloadUpdate')}</button>
          <button class="btn" id="s-install-update" style="display:none;">${t('settings.installUpdate')}</button>
        </div>
      </div>

      <div class="card" style="margin-bottom:16px;">
        <label>${t('settings.ram')}</label>
        <div style="display:flex; gap:20px; margin-top:8px;">
          <div style="flex:1;">
            <label>${t('settings.ramMin')} (MB)</label>
            <input type="number" id="s-ram-min" value="${settings.ramMin}" min="512" step="256" style="width:100%;" />
          </div>
          <div style="flex:1;">
            <label>${t('settings.ramMax')} (MB)</label>
            <input type="number" id="s-ram-max" value="${settings.ramMax}" min="1024" step="256" style="width:100%;" />
          </div>
        </div>
      </div>

      <div class="card" style="margin-bottom:16px;">
        <label>${t('settings.javaPath')}</label>
        <div style="display:flex; gap:8px; margin-bottom:10px;">
          <input type="text" id="s-java-path" value="${escapeHtml(settings.javaPath || '')}" style="flex:1;" />
          <button class="btn secondary" id="s-java-auto">${t('settings.javaAuto')}</button>
          <button class="btn secondary" id="s-java-browse">${t('settings.javaBrowse')}</button>
        </div>
        <label>${t('settings.javaDownload')}</label>
        <div style="display:flex; gap:8px; margin-top:6px;">
          ${[8, 17, 21, 25].map(v => `<button class="btn secondary" data-java-dl="${v}">Java ${v}</button>`).join('')}
        </div>
        <p class="subtle" style="margin-top:8px;">${t('settings.javaAutoHint')}</p>
      </div>

      <div class="card" style="margin-bottom:16px;">
        <label>${t('settings.extraArgs')}</label>
        <input type="text" id="s-extra-args" value="${escapeHtml(settings.extraArgs || '')}" style="width:100%;" placeholder="-XX:+UseG1GC" />
      </div>

      <div class="card" style="margin-bottom:16px;">
        <label>${t('settings.launchBehavior')}</label>
        <select id="s-launch-behavior" style="width:220px;">
          <option value="keepOpen" ${settings.launchBehavior === 'keepOpen' ? 'selected' : ''}>${t('settings.launchBehaviorOptions.keepOpen')}</option>
          <option value="minimize" ${settings.launchBehavior === 'minimize' ? 'selected' : ''}>${t('settings.launchBehaviorOptions.minimize')}</option>
          <option value="close" ${settings.launchBehavior === 'close' ? 'selected' : ''}>${t('settings.launchBehaviorOptions.close')}</option>
        </select>
      </div>

      <div class="card" style="margin-bottom:16px; display:flex; flex-direction:column; gap:10px;">
        <label style="display:flex; align-items:center; gap:8px;">
          <input type="checkbox" id="s-start-boot" ${settings.startOnBoot ? 'checked' : ''} /> ${t('settings.startOnBoot')}
        </label>
        <label style="display:flex; align-items:center; gap:8px;">
          <input type="checkbox" id="s-tray" ${settings.minimizeToTray ? 'checked' : ''} /> ${t('settings.minimizeToTray')}
        </label>
      </div>

      <div class="card" style="margin-bottom:16px;">
        <label style="display:flex; align-items:center; gap:8px; margin:0;">
          <input type="checkbox" id="s-discord" ${settings.discordPresenceEnabled ? 'checked' : ''} /> ${t('settings.discordPresence')}
        </label>
        <p class="subtle" style="margin:8px 0 0;">${t('settings.discordPresenceHint')}</p>
      </div>

      <div class="card" style="margin-bottom:16px;">
        <label>${t('settings.background')}</label>
        <label style="display:flex; align-items:center; gap:8px; margin:8px 0;">
          <input type="checkbox" id="s-bg-enabled" ${settings.backgroundEnabled ? 'checked' : ''} /> ${t('settings.backgroundEnabled')}
        </label>
        <div style="display:flex; gap:8px;">
          <button class="btn secondary" id="s-bg-choose">${t('settings.backgroundChoose')}</button>
          <button class="btn secondary" id="s-bg-reset">${t('settings.backgroundReset')}</button>
        </div>
      </div>

      <button class="btn" id="s-save">${t('profiles.save')}</button>
    `;

    // --- Otomatik güncelleme ---
    const updateStatusEl = el.querySelector('#update-status');
    const checkBtn = el.querySelector('#s-check-update');
    const downloadBtn = el.querySelector('#s-download-update');
    const installBtn = el.querySelector('#s-install-update');

    function renderUpdateStatus(data) {
      if (!data) return;
      if (data.stage === 'checking') updateStatusEl.textContent = t('settings.updateStatus.checking');
      else if (data.stage === 'none') updateStatusEl.textContent = t('settings.updateStatus.none');
      else if (data.stage === 'available') {
        updateStatusEl.textContent = t('settings.updateStatus.available', { version: data.version });
        downloadBtn.style.display = 'inline-block';
      } else if (data.stage === 'downloading') {
        updateStatusEl.textContent = t('settings.updateStatus.downloading', { percent: data.percent });
        downloadBtn.style.display = 'none';
      } else if (data.stage === 'downloaded') {
        updateStatusEl.textContent = t('settings.updateStatus.downloaded', { version: data.version });
        downloadBtn.style.display = 'none';
        installBtn.style.display = 'inline-block';
      } else if (data.stage === 'error') {
        updateStatusEl.textContent = t('settings.updateStatus.error', { error: data.message });
      }
    }
    window.buran.updater.onStatus(renderUpdateStatus);

    checkBtn.onclick = async () => {
      updateStatusEl.textContent = t('settings.updateStatus.checking');
      try {
        const result = await window.buran.updater.check();
        if (result && result.dev) updateStatusEl.textContent = t('settings.updateStatus.devMode');
      } catch (err) {
        updateStatusEl.textContent = t('settings.updateStatus.error', { error: err.message });
      }
    };
    downloadBtn.onclick = async () => {
      try {
        await window.buran.updater.download();
      } catch (err) {
        showToast(t('toast.genericError', { error: err.message }), 'error');
      }
    };
    installBtn.onclick = async () => {
      const ok = await confirmDialog(t('settings.updateStatus.installConfirm'));
      if (!ok) return;
      await window.buran.updater.install();
    };

    el.querySelector('#s-lang').onchange = async (e) => {
      await changeLang(e.target.value);
      window.renderApp();
    };

    el.querySelector('#s-java-auto').onclick = async () => {
      const javaPath = await window.buran.java.detect();
      if (javaPath) el.querySelector('#s-java-path').value = javaPath;
      else showToast(t('toast.genericError', { error: 'Java bulunamadı' }), 'error');
    };
    el.querySelector('#s-java-browse').onclick = async () => {
      const p = await window.buran.java.browse();
      if (p) el.querySelector('#s-java-path').value = p;
    };
    el.querySelectorAll('[data-java-dl]').forEach(btn => {
      btn.onclick = async () => {
        const majorVersion = parseInt(btn.getAttribute('data-java-dl'), 10);
        btn.disabled = true;
        try {
          showToast(t('common.loading'));
          const javaPath = await window.buran.java.download(majorVersion);
          el.querySelector('#s-java-path').value = javaPath;
          showToast(t('toast.javaDownloaded'), 'success');
        } catch (err) {
          showToast(t('toast.genericError', { error: err.message }), 'error');
        } finally {
          btn.disabled = false;
        }
      };
    });

    el.querySelector('#s-bg-choose').onclick = async () => {
      try {
        const result = await window.buran.background.choose();
        if (result) { await window.applyBackground(); showToast(t('toast.settingsSaved'), 'success'); }
      } catch (err) {
        showToast(t('toast.genericError', { error: err.message }), 'error');
      }
    };
    el.querySelector('#s-bg-reset').onclick = async () => {
      await window.buran.background.reset();
      await window.applyBackground();
      showToast(t('toast.settingsSaved'), 'success');
    };
    el.querySelector('#s-bg-enabled').onchange = async (e) => {
      await window.buran.settings.set({ backgroundEnabled: e.target.checked });
      await window.applyBackground();
    };

    el.querySelector('#s-save').onclick = async () => {
      const patch = {
        ramMin: parseInt(el.querySelector('#s-ram-min').value, 10),
        ramMax: parseInt(el.querySelector('#s-ram-max').value, 10),
        javaPath: el.querySelector('#s-java-path').value,
        extraArgs: el.querySelector('#s-extra-args').value,
        launchBehavior: el.querySelector('#s-launch-behavior').value,
        startOnBoot: el.querySelector('#s-start-boot').checked,
        minimizeToTray: el.querySelector('#s-tray').checked,
        backgroundEnabled: el.querySelector('#s-bg-enabled').checked,
        discordPresenceEnabled: el.querySelector('#s-discord').checked
      };
      await window.buran.settings.set(patch);
      showToast(t('toast.settingsSaved'), 'success');
    };
  }
};
