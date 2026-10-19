function openModal(innerHtml) {
  const overlay = document.getElementById('modal-overlay');
  const box = document.getElementById('modal-box');
  box.innerHTML = innerHtml;
  overlay.classList.add('open');
  return box;
}

function closeModal() {
  document.getElementById('modal-overlay').classList.remove('open');
  document.getElementById('modal-box').innerHTML = '';
}

function confirmDialog(message) {
  return new Promise(resolve => {
    const box = openModal(`
      <h2>${t('common.confirm')}</h2>
      <p class="subtle">${message}</p>
      <div class="modal-actions">
        <button class="btn secondary" id="confirm-no">${t('common.no')}</button>
        <button class="btn danger" id="confirm-yes">${t('common.yes')}</button>
      </div>
    `);
    box.querySelector('#confirm-yes').onclick = () => { closeModal(); resolve(true); };
    box.querySelector('#confirm-no').onclick = () => { closeModal(); resolve(false); };
  });
}

document.addEventListener('DOMContentLoaded', () => {
  document.getElementById('modal-overlay').addEventListener('click', (e) => {
    if (e.target.id === 'modal-overlay') closeModal();
  });
});
