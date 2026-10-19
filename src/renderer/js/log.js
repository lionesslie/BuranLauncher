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
    if (status && status.task) appendLog(`[durum] ${status.task} (${status.total ? Math.round((status.current / status.total) * 100) : 0}%)`);
  });
  window.buran.launch.onClosed(({ code }) => {
    appendLog(`--- Oyun kapandı (kod: ${code}) ---`);
  });
});
