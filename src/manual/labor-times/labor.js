// Labor Times, the buttons, and starting the page: the saved tray and last search come back, SLTS opens, and the
// search form is set the way it was. Loaded last, after the other files (see common.js).
'use strict';

// ---------- start ----------
$('l-type').addEventListener('change', () => changeType($('l-type').value));
$('l-form').addEventListener('submit', (e) => { e.preventDefault(); search(); });
$('l-pick').addEventListener('change', () => search());
$('btn-tray').onclick = () => { $('l-tray').scrollIntoView({ behavior: 'smooth' }); $('l-tray').classList.add('l-flash'); setTimeout(() => $('l-tray').classList.remove('l-flash'), 1200); };

async function init() {
  const info = await api.init().catch(() => null);
  if (info?.state && typeof info.state === 'object') saved = { tray: [], type: '', pick: '', q: '', ...info.state };
  if (!Array.isArray(saved.tray)) saved.tray = [];
  vehicleLine = [info?.ymm, info?.vin, info?.ro ? `RO ${info.ro}` : ''].filter(Boolean).join(' · ');
  $('l-sub').textContent = vehicleLine ? `Ford SLTS · ${vehicleLine}` : 'Ford SLTS';
  renderTray();
  showState(el('div', { class: 'spinner' }), 'Opening SLTS…');
  busy = true;
  try {
    await start();
    if (saved.type && !WORD(saved.type) && saved.type !== currentType()) await postBack({ DropDownList_SearchType: saved.type }, { eventTarget: 'DropDownList_SearchType' });
    renderForm();
    if (WORD(saved.type)) showWordForm();
    if (saved.q) $('l-q').value = saved.q;
    hint();
  } catch (e) { failed(e, init); } finally { busy = false; }
}
init();
