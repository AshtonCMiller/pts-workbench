// Diagnostics, starting the page (the last search comes back), and the search box and buttons. Loaded last, after the
// other files (see common.js).
'use strict';

async function start() {
  const init = await api.init();
  if (init?.state) state = { ...state, ...init.state };
  $('d-sub').textContent = [init?.ymm, init?.vin, init?.ro ? `RO ${init.ro}` : ''].filter(Boolean).join(' · ');
  $('d-q').value = state.query || '';
  $('d-exact').checked = !!state.exact;
  if (state.query) run(state.query); else showHint();
  $('d-q').focus();
}

api.onOpen?.((link) => { if (link?.query) { $('d-q').value = link.query; run(link.query); } });
$('d-form').addEventListener('submit', (e) => { e.preventDefault(); run($('d-q').value); });
$('d-exact').addEventListener('change', () => { if (state.query) run($('d-q').value); });
$('btn-cv').onclick = loadVehicleDtcs;
window.addEventListener('error', (e) => api.log(`diagnostics error: ${e.message} at ${e.lineno}`));
start();
