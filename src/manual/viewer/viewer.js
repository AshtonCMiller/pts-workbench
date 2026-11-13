// Workshop Manual, the layout (contents width, hiding the contents), keys, and starting the page: which book this tab
// shows (Workshop Manual or PC/ED), the saved procedures and folders come back, and the contents load. Loaded last,
// after the other files (see common.js).
'use strict';

// ---------- start ----------
async function start() {
  const init = await api.init();
  if (init?.book === 'pced') {
    bookKind = 'pced';
    document.title = 'PC/ED';
    $('book-title').textContent = 'PC/ED';
    $('search').placeholder = 'Search the contents or a DTC… (Ctrl+K)';
  }
  if (init?.state) state = { ...state, ...init.state };
  applyLayout();
  renderTabs();
  if (init?.open) openDoc(init.open, { newTab: true }); // sent here from the Diagnostics tab
  else showDoc();
  loadContents();
}
api.onOpen?.((link) => openDoc(link, { newTab: true }));

// ---------- layout ----------
function applyLayout() {
  document.documentElement.style.setProperty('--toc', `${Math.max(220, Math.min(700, state.tocWidth || 340))}px`);
  document.body.classList.toggle('toc-hidden', !!state.tocHidden);
}
$('btn-toc').onclick = () => { state.tocHidden = !state.tocHidden; applyLayout(); save(); };
$('toc-resize').addEventListener('mousedown', (e) => {
  e.preventDefault();
  const move = (ev) => { state.tocWidth = Math.max(220, Math.min(700, ev.clientX)); applyLayout(); };
  const up = () => { window.removeEventListener('mousemove', move); window.removeEventListener('mouseup', up); save(); };
  window.addEventListener('mousemove', move);
  window.addEventListener('mouseup', up);
});
let searchTimer = null;
$('search').addEventListener('input', (e) => {
  clearTimeout(searchTimer);
  searchTimer = setTimeout(() => { state.query = e.target.value; save(); if (tree) renderTree(); }, 150);
});
$('zoom').onclick = () => { $('zoom').hidden = true; };
let scrollTimer = null;
$('content').addEventListener('scroll', () => {
  clearTimeout(scrollTimer);
  scrollTimer = setTimeout(() => { rememberScroll(); save(); }, 300);
});
$('vin-only').addEventListener('change', () => {
  state.allOptions = !$('vin-only').checked;
  save();
  buildShown();
  drawFilter();
  renderTree();
});
window.addEventListener('keydown', (e) => {
  if (e.key === 'Escape') $('zoom').hidden = true;
  // Ctrl+F finds words in the page (the app's find bar); Ctrl+K searches the manual's contents
  if (e.ctrlKey && e.key.toLowerCase() === 'k') { e.preventDefault(); if (state.tocHidden) { state.tocHidden = false; applyLayout(); } $('search').focus(); $('search').select(); }
  if (e.ctrlKey && e.key.toLowerCase() === 'w' && state.active) { e.preventDefault(); closeDoc(state.active); }
  if (e.altKey && e.key === 'ArrowLeft') $('btn-back').click();
});
window.addEventListener('beforeunload', () => { rememberScroll(); api.save(JSON.parse(JSON.stringify(state))); });

window.addEventListener('error', (e) => api.log(`error: ${e.message} at ${e.lineno}`));
window.addEventListener('unhandledrejection', (e) => api.log(`error: ${e.reason && e.reason.stack || e.reason}`));
start();
