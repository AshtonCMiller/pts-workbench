// Wiring Diagrams, the buttons and keys, and starting the page: the saved state comes back (the page that was
// open, the filter, "Only this vehicle"). Loaded last, after the other files (see common.js for what each one does).
'use strict';

// ---------- buttons ----------
function fordPage() {
  if (state.at) api.wiringFord({ kind: 'page', cell: state.at.cell, page: state.at.page, item: state.at.item, title: titleOf(state.at.cell) || state.at.title });
  else api.wiringFord({ kind: 'TableOfContents', title: 'Wiring' });
  toast("Opening Ford's page as a tab…");
}
function goBack() {
  const prev = backList.pop();
  if (prev) openPage(prev, { push: false }); else showList();
}

$('btn-contents').addEventListener('click', () => { backList = []; showList(); });
$('btn-back').addEventListener('click', goBack);
$('btn-ford').addEventListener('click', fordPage);
$('btn-print').addEventListener('click', () => window.print());
$('btn-in').addEventListener('click', () => zoomAt(1.4));
$('btn-out').addEventListener('click', () => zoomAt(1 / 1.4));
$('btn-fit').addEventListener('click', fit);
$('w-form').addEventListener('submit', (e) => e.preventDefault());
$('w-q').addEventListener('input', () => { state.q = $('w-q').value.trim(); save(); drawList(); });
$('btn-search').addEventListener('click', openSearch);
$('w-sq').addEventListener('input', runSearch);
$('w-sq').addEventListener('keydown', (e) => {
  if (e.key === 'Enter') { e.preventDefault(); clearTimeout(search.timer); search.q = ''; runSearch(); }
  if (e.key === 'ArrowDown') { e.preventDefault(); $('w-sbody').querySelector('.w-res')?.focus(); }
});
$('w-sclose').addEventListener('click', closeSearch);
$('w-modal').addEventListener('pointerdown', (e) => { if (e.target === $('w-modal')) closeSearch(); });
$('w-sbody').addEventListener('keydown', (e) => {
  const items = [...$('w-sbody').querySelectorAll('.w-res')];
  const i = items.indexOf(document.activeElement);
  if (i < 0) return;
  if (e.key === 'ArrowDown') { e.preventDefault(); items[Math.min(items.length - 1, i + 1)].focus(); }
  if (e.key === 'ArrowUp') { e.preventDefault(); (i ? items[i - 1] : $('w-sq')).focus(); }
});
$('w-find').addEventListener('input', () => findOnPage($('w-find').value));
$('w-find').addEventListener('keydown', (e) => {
  if (e.key === 'Enter') { e.preventDefault(); findNext(e.shiftKey); }
  else if (e.key === 'Escape') { $('w-find').value = ''; findOnPage(''); $('w-stage').focus(); }
});
$('w-sort').addEventListener('change', () => { state.sort = $('w-sort').value; save(); drawList(); });
$('w-only').addEventListener('change', () => { state.only = $('w-only').checked; save(); drawList(); drawSearch(); if (state.at) pagesOf(state.at.cell).then((pg) => fillPager(state.at, pg)); });
document.addEventListener('keydown', (e) => {
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') { e.preventDefault(); openSearch(); return; }
  if (!$('w-modal').hidden) { if (e.key === 'Escape') { e.preventDefault(); closeSearch(); } return; }
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'f') {
    e.preventDefault();
    const box = $('w-page-view').hidden ? $('w-q') : $('w-find');
    box.focus(); box.select();
    return;
  }
  if ($('w-page-view').hidden || e.target.closest?.('input, select, textarea')) return;
  if (!['Shift', 'Control', 'Alt', 'Meta'].includes(e.key)) stopAnim();
  if (e.key === '+' || e.key === '=') zoomAt(1.4);
  else if (e.key === '-' || e.key === '_') zoomAt(1 / 1.4);
  else if (e.key === '0') fit();
  else if (e.key === 'Escape') hidePop();
  else if (e.key === 'Backspace' || (e.altKey && e.key === 'ArrowLeft')) goBack();
  else if (e.key === 'PageDown') $('btn-next').click();
  else if (e.key === 'PageUp') $('btn-prev').click();
  else return;
  e.preventDefault();
});
document.addEventListener('pointerdown', (e) => { if (!$('w-pop').hidden && !e.target.closest('#w-pop, #w-stage')) hidePop(); });

// Diagnostics sends wiring results here
api.onOpen?.((o) => { if (o?.wiring) { backList = []; openPage(o.wiring); } });

(async () => {
  stageEvents();
  const info = (await api.init().catch(() => null)) || {};
  const s = info.state || {};
  state = { q: String(s.q || ''), sort: s.sort === 'title' ? 'title' : 'cell', only: s.onlyVin !== false, at: s.at && s.at.cell ? s.at : null };
  $('w-q').value = state.q; $('w-sort').value = state.sort; $('w-only').checked = state.only;
  const go = info.open?.wiring || state.at;
  if (go) {
    openPage(go, { push: false });
    loadToc().then((ok) => { if (ok && state.at) $('w-title').textContent = `${state.at.cell}-${state.at.page}  ${titleOf(state.at.cell) || state.at.title}`; });
  } else {
    await loadToc();
  }
})();
