// Wiring Diagrams, the contents: Ford's list of diagrams for this vehicle, gray (or hidden) when they don't apply to
// this VIN, with the filter box and sorting. Also the page's loading and error states.
'use strict';

// ---------- contents ----------
let toc = null; // { info, items }
function titleOf(cell) {
  const n = cellNo(cell);
  const it = toc?.items.find((x) => cellNo(x.Number) === n);
  return it ? clean(it.Title) : '';
}

function showState(...kids) { $('w-body').replaceChildren(el('div', { class: 'state' }, ...kids)); }
function errorState(res, retry, what) {
  return [el('h2', { text: res.needsLogin ? 'Sign in to PTS' : `Couldn't load ${what}` }),
    el('p', { text: res.needsLogin ? 'PTS needs you to sign in again.' : res.message || '' }),
    el('div', { class: 'actions' }, res.needsLogin
      ? el('button', { class: 'primary', text: 'Sign in to PTS', onclick: async () => { await api.signIn(); retry(); } })
      : el('button', { class: 'primary', text: 'Try again', onclick: retry }),
    el('button', { class: 'ghost', text: 'Show log', onclick: () => api.openLog() }))];
}

async function loadToc() {
  showState(el('p', { text: 'Loading the wiring book…' }));
  let res;
  try { res = await api.wiringToc(); } catch (e) { res = { error: true, message: String(e?.message || e) }; }
  if (res?.error) { showState(...errorState(res, loadToc, 'the wiring book')); return false; }
  toc = { info: res.info || {}, items: (res.items || []).filter((x) => x && x.Number != null) };
  features = res.features && Array.isArray(res.features.pfc) ? res.features : null;
  $('w-only-label').hidden = !features;
  if (!$('w-list-view').hidden) $('w-sub').textContent = `${toc.info.title || 'Wiring'} · ${toc.items.length} diagrams`;
  else if (state.at) $('w-sub').textContent = `${toc.info.title || 'Wiring'}${state.at.item ? ` · showing ${state.at.item}` : ''}`;
  api.log(`wiring: ${toc.items.length} cells, ${features ? 'VIN options known' : 'no VIN options (nothing grayed)'}`);
  drawList();
  return true;
}

const TYPE_NOTE = { Connectors: 'Connector views', LocIndex: 'Location charts' };

function drawList() {
  if (!toc) return;
  const q = state.q.toLowerCase();
  let list = toc.items.map((it) => ({ it, ok: cellApplies(it) }));
  if (q) list = list.filter(({ it }) => `${it.Number} ${cellNo(it.Number)} ${it.Title}`.toLowerCase().includes(q));
  if (state.only && features) list = list.filter((x) => x.ok);
  list.sort(state.sort === 'title'
    ? (a, b) => clean(a.it.Title).localeCompare(clean(b.it.Title))
    : (a, b) => (Number(cellNo(a.it.Number)) - Number(cellNo(b.it.Number))) || String(a.it.Number).localeCompare(String(b.it.Number)));
  const grayed = features ? toc.items.filter((it) => !cellApplies(it)).length : 0;
  const head = el('p', { class: 't-count', text: `${list.length} of ${toc.items.length} diagrams` +
    (grayed && !state.only ? ` · gray ones (${grayed}) don't apply to this VIN` : '') });
  const ul = el('div', { class: 'w-list' });
  for (const { it, ok } of list) {
    let hover = null;
    ul.append(el('button', { class: `w-item${ok ? '' : ' na'}`, title: ok ? '' : "Doesn't apply to this VIN", onclick: () => openCell(it),
      onpointerenter: () => { if (!it.Type || it.Type === 'Page') hover = setTimeout(() => warm(it.Number, 1), 120); },
      onpointerleave: () => clearTimeout(hover) },
      el('span', { class: 'num', text: String(it.Number) }),
      el('span', { class: 'name', text: clean(it.Title) }),
      TYPE_NOTE[it.Type] ? el('span', { class: 'tag', text: `${TYPE_NOTE[it.Type]} · Ford's page` }) : null));
  }
  if (!list.length) ul.append(el('p', { class: 'muted w-none' }, q ? 'No diagram has this in its name. ' : 'Nothing matches.',
    q ? el('button', { class: 'ghost small', text: `Search the wiring for “${state.q}” ›`, onclick: () => { $('w-sq').value = state.q; runSearch(); openSearch(); } }) : null));
  $('w-body').replaceChildren(head, ul);
}

function openCell(it) {
  if (it.Type && it.Type !== 'Page') { // the connector list and location index are Ford's own pages
    api.wiringFord({ kind: it.Type, cell: cellNo(it.Number), page: 1, title: clean(it.Title) });
    toast("Opening Ford's page as a tab…");
    return;
  }
  backList = [];
  openPage({ cell: cellNo(it.Number), page: 1, item: '', title: clean(it.Title) });
}

function showList() {
  if (state.at) backList.push(state.at);
  state.at = null; save();
  hidePop();
  $('w-page-view').hidden = true;
  $('w-list-view').hidden = false;
  for (const id of ['btn-contents', 'w-pager', 'w-zoom', 'btn-print']) $(id).hidden = true;
  $('btn-back').hidden = true;
  $('w-title').textContent = 'Wiring Diagrams';
  $('w-sub').textContent = toc ? `${toc.info.title || 'Wiring'} · ${toc.items.length} diagrams` : "Ford's wiring book for this vehicle";
  if (!toc) loadToc(); else drawList();
}
