function appendLog(line) {
  const out = document.getElementById('log-output');
  out.textContent += line + '\n';
  out.scrollTop = out.scrollHeight;
}

document.addEventListener('DOMContentLoaded', () => {
  const panel = document.getElementById('log-panel');
  document.getElementById('log-panel').querySelector('.log-panel-header').addEventListener('click', (e) => {
    if (e.target.id === 'log-clear') return;
    panel.classList.toggle('open');
  });
  document.getElementById('log-clear').addEventListener('click', () => {
    document.getElementById('log-output').textContent = '';
  });

  window.buran.launch.onLog(({ line }) => appendLog(line));
  window.buran.launch.onStatus(({ status }) => {
    if (!status) return;
    if (status.task === 'java-auto-download') {
      const stage = status.stage;
      const javaMajor = status.javaMajor ? ` (Java ${status.javaMajor})` : '';
      const label = stage === 'cached' ? `Yerel Java bulundu${javaMajor}`
        : stage === 'autoDownloadStart' ? `Uygun Java bulunamadı, otomatik indiriliyor${javaMajor}...`
        : stage === 'fetchingMeta' ? `Java sürüm bilgisi alınıyor${javaMajor}...`
        : stage === 'downloading' ? `Java indiriliyor${javaMajor}...`
        : stage === 'extracting' ? `Java çıkartılıyor${javaMajor}...`
        : stage === 'done' ? `Java kurulumu tamamlandı${javaMajor}`
        : `${stage}${javaMajor}`;
      appendLog(`[java] ${label}`);
      return;
    }
    if (status.task) appendLog(`[durum] ${status.task} (${status.total ? Math.round((status.current / status.total) * 100) : 0}%)`);
  });
  window.buran.launch.onClosed(({ code }) => {
    appendLog(`--- Oyun kapandı (kod: ${code}) ---`);
  });
});
