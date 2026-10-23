window.Pages = window.Pages || {};

window.Pages.accounts = {
  async render() {
    const el = document.getElementById('page-accounts');
    const accounts = await window.buran.accounts.list();
    const active = await window.buran.accounts.getActive();

    el.innerHTML = `
      <h1 class="page-title">${t('accounts.title')}</h1>
      <div class="toolbar">
        <div class="toolbar-left">
          <button class="btn" id="acc-ms">${t('accounts.addMicrosoft')}</button>
          <button class="btn secondary" id="acc-offline">${t('accounts.addOffline')}</button>
        </div>
      </div>
      <div id="acc-list"></div>
    `;

    const list = el.querySelector('#acc-list');
    list.innerHTML = accounts.map((a, i) => `
      <div class="mod-row" style="animation-delay:${i * 40}ms">
        <img class="account-avatar" style="width:36px;height:36px;" src="${window.accountAvatarUrl(a)}" onerror="this.style.visibility='hidden'" />
        <div class="mod-row-info">
          <div class="mod-row-title">${escapeHtml(a.username)}</div>
          <div class="mod-row-desc">${a.type === 'microsoft' ? t('accounts.typeMicrosoft') : t('accounts.typeOffline')}</div>
        </div>
        ${active && active.id === a.id ? `<span class="badge success">${t('accounts.active')}</span>` : `<button class="btn secondary" data-setactive="${a.id}">${t('accounts.setActive')}</button>`}
        <button class="btn danger" data-remove="${a.id}">${t('accounts.remove')}</button>
      </div>
    `).join('') || `<p class="subtle">${t('accounts.typeOffline')} / ${t('accounts.typeMicrosoft')} ${t('home.noInstances')}</p>`;

    list.querySelectorAll('[data-setactive]').forEach(btn => {
      btn.onclick = async () => {
        await window.buran.accounts.setActive(btn.getAttribute('data-setactive'));
        this.render();
        window.updateTitlebarAccount();
      };
    });
    list.querySelectorAll('[data-remove]').forEach(btn => {
      btn.onclick = async () => {
        const ok = await confirmDialog(btn.closest('.mod-row').querySelector('.mod-row-title').textContent);
        if (!ok) return;
        await window.buran.accounts.remove(btn.getAttribute('data-remove'));
        this.render();
        window.updateTitlebarAccount();
      };
    });

    el.querySelector('#acc-ms').onclick = async () => {
      try {
        await window.buran.accounts.loginMicrosoft();
        showToast(t('toast.loginSuccess'), 'success');
        this.render();
        window.updateTitlebarAccount();
      } catch (err) {
        // Asıl hata mesajı gösterilir (önceden jenerik mesaj hatayı gizliyordu)
        showToast(`${t('toast.loginFailed')}: ${err.message}`, 'error', 7000);
        appendLog(`[hata] Microsoft girişi: ${err.message}`);
      }
    };

    el.querySelector('#acc-offline').onclick = () => {
      const box = openModal(`
        <h2>${t('accounts.addOffline')}</h2>
        <div class="field">
          <label>${t('accounts.offlineUsername')}</label>
          <input type="text" id="offline-username" style="width:100%;" />
        </div>
        <div class="modal-actions">
          <button class="btn secondary" id="offline-cancel">${t('profiles.cancel')}</button>
          <button class="btn" id="offline-submit">${t('accounts.addOffline')}</button>
        </div>
      `);
      box.querySelector('#offline-cancel').onclick = closeModal;
      box.querySelector('#offline-submit').onclick = async () => {
        const username = box.querySelector('#offline-username').value;
        try {
          await window.buran.accounts.addOffline(username);
          closeModal();
          showToast(t('toast.loginSuccess'), 'success');
          this.render();
          window.updateTitlebarAccount();
        } catch (err) {
          showToast(t('toast.genericError', { error: err.message }), 'error');
        }
      };
    };
  }
};
