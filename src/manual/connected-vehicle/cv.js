// Connected Vehicle: the tabs along the top (Home, Over-the-air updates, Software), Refresh and Ford's page, and starting
// the page. Loaded last.
'use strict';

const VIEWS = { home: showHome, ota: showOta, subs: showSubs };

function show(view, fresh = false) {
  state.view = VIEWS[view] ? view : 'home';
  for (const b of document.querySelectorAll('#cv-tabs .d-tab')) b.classList.toggle('on', b.dataset.view === state.view);
  $('btn-ford').title = state.view === 'ota' ? "Open Ford's own OTA page as a tab" : "Open Ford's own Connected Vehicle page as a tab";
  save();
  drawPickBar(); // the picked DTCs bar is only on Home
  window.scrollTo(0, 0);
  VIEWS[state.view](fresh);
}

for (const b of document.querySelectorAll('#cv-tabs .d-tab')) b.onclick = () => show(b.dataset.view);
$('btn-refresh').onclick = () => show(state.view, true);
$('btn-ford').onclick = () => api.openPts(state.view === 'ota' ? 'ota' : 'cvd');

(async () => {
  const info = await api.init().catch(() => null);
  if (info?.vin) {
    $('cv-sub').replaceChildren(info.ymm ? `${info.ymm} · ` : '', el('span', { class: 'vin', title: 'Click to copy', onclick: () => copy(info.vin), text: info.vin }));
  }
  show(info?.state?.view || 'home');
})();
