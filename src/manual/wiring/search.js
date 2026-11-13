// Wiring Diagrams, the Search window (Search button or Ctrl+K, anywhere in Wiring Diagrams): Ford's search, wiring
// only (components, connectors, splices, grounds, diagrams). It keeps its last search; a result opens its page with the
// item marked.
'use strict';

// ---------- search (Ford's search, wiring only: components, connectors, splices, grounds, diagrams) ----------
const KINDS = [['all', 'All'], ['component', 'Components'], ['connector', 'Connectors'], ['splice', 'Splices'], ['ground', 'Grounds'], ['diagram', 'Diagrams']];
const KIND_LABEL = { component: 'COMPONENT', connector: 'CONNECTOR', splice: 'SPLICE', ground: 'GROUND', diagram: 'DIAGRAM', other: 'WIRING' };
let search = { q: '', results: null, busy: false, error: '', kind: 'all', seq: 0, timer: null };
// The search window: opened from the Search button (or Ctrl+K) anywhere in Wiring Diagrams; it keeps its last search
function openSearch() {
  hidePop();
  $('w-modal').hidden = false;
  const q = $('w-sq');
  q.focus(); q.select();
  drawSearch();
}
function closeSearch() {
  $('w-modal').hidden = true;
  if (!$('w-page-view').hidden) $('w-stage').focus({ preventScroll: true });
}
function drawSearch() {
  if ($('w-modal').hidden) return;
  const found = searchSection();
  $('w-sbody').replaceChildren(found || el('div', { class: 'w-hintbox' },
    el('p', { text: 'Search this vehicle’s wiring for a component, connector, splice, ground or diagram.' }),
    el('p', { class: 'muted', text: 'For example: C1689 · G100 · S124 · battery monitoring sensor · PCM · power windows' })));
}
function kindOf(r) {
  const t = String(r.type || '').toLowerCase(), c = String(r.conn || '').toUpperCase();
  if (t === 'celttl') return 'diagram';
  if (/^S\d/.test(c) || t.includes('splice')) return 'splice';
  if (/^G\d/.test(c) || t.includes('ground')) return 'ground';
  if (t.includes('component')) return 'component';
  if (c || t === 'cfc' || t.includes('conn')) return 'connector';
  return 'other';
}
function runSearch() {
  clearTimeout(search.timer);
  const q = $('w-sq').value.trim();
  if (q.length < 2) { search = { ...search, q: '', results: null, busy: false, error: '' }; drawSearch(); return; }
  if (q === search.q && (search.results || search.busy)) return;
  search.q = q; search.busy = true; search.error = '';
  const seq = ++search.seq;
  search.timer = setTimeout(async () => {
    let res;
    try { res = await api.wiringSearch(q); } catch (e) { res = { error: true, message: String(e?.message || e) }; }
    if (seq !== search.seq) return;
    search.busy = false;
    if (res?.error) { search.results = []; search.error = res.needsLogin ? 'Sign in to PTS to search' : res.message || "Ford's search didn't answer"; }
    else search.results = (res.results || []).map((r) => ({ ...r, kind: kindOf(r) }));
    drawSearch();
  }, 350);
  drawSearch();
}
function openResult(r) {
  const cell = cellNo(r.cell);
  if (r.type === 'cfc' || cell === '150' || (!cell && r.url)) { // connector faces and the connector list: Ford's pages
    if (r.url) { closeSearch(); api.openArticle(r.url, clean(r.conn ? `${r.conn} face` : r.title).slice(0, 40)); toast("Opening Ford's page as a tab…"); }
    return;
  }
  if (!cell) return;
  closeSearch();
  // Ford's component titles end with where they are ("HEATED WIPER PARK RELAY: Power Distribution/BCM"): the name is before that
  const item = r.conn || (r.kind === 'component' ? clean(r.title).split(/:\s/)[0] : '');
  openPage({ cell, page: Number(r.page) || 1, item, title: r.kind === 'diagram' ? clean(r.title).replace(/^\d+\s+/, '') : '' });
}
function searchSection() {
  if (!search.q) return null;
  const box = el('section', { class: 'w-search' });
  if (search.busy && !search.results) { box.append(el('p', { class: 'muted', text: 'Searching Ford’s wiring…' })); return box; }
  if (search.error) { box.append(el('p', { class: 'muted', text: search.error })); return box; }
  const every = search.results || [];
  // "Only this vehicle": results Ford marks as not for this VIN are left out
  const all = state.only ? every.filter((r) => r.applies !== false) : every;
  const counts = {};
  for (const r of all) counts[r.kind] = (counts[r.kind] || 0) + 1;
  if (search.kind !== 'all' && !counts[search.kind]) search.kind = 'all';
  const tabs = el('nav', { class: 'd-tabs w-kinds' });
  for (const [k, label] of KINDS) {
    const n = k === 'all' ? all.length : counts[k] || 0;
    if (k !== 'all' && !n) continue;
    tabs.append(el('button', { class: `d-tab${search.kind === k ? ' on' : ''}`, onclick: () => { search.kind = k; drawSearch(); } }, label, el('span', { class: 'n', text: String(n) })));
  }
  box.append(tabs);
  const list = all.filter((r) => search.kind === 'all' || r.kind === search.kind);
  const order = { component: 0, connector: 1, splice: 2, ground: 3, diagram: 4, other: 5 };
  list.sort((a, b) => (order[a.kind] - order[b.kind]) || (a.applies === b.applies ? 0 : a.applies ? -1 : 1));
  const ul = el('div', { class: 'w-list' });
  for (const r of list) {
    const cell = cellNo(r.cell);
    const where = r.type === 'cfc' ? 'Pin view · Ford’s page' : cell === '151' ? `Location view 151-${Number(r.page) || 1}`
      : cell === '150' ? 'Connector list · Ford’s page' : cell ? `${cell}-${Number(r.page) || 1}${titleOf(cell) ? ` ${titleOf(cell)}` : ''}` : '';
    ul.append(el('button', { class: `w-item w-res${r.applies === false ? ' na' : ''}`, title: r.applies === false ? "Doesn't apply to this VIN" : '', onclick: () => openResult(r) },
      el('span', { class: `w-kind ${r.kind}`, text: KIND_LABEL[r.kind] }),
      el('span', { class: 'name', text: clean(r.title) }),
      el('span', { class: 'tag', text: where })));
  }
  if (!list.length) ul.append(el('p', { class: 'muted w-none', text: 'Ford’s search found nothing for this.' }));
  if (every.length > all.length) ul.append(el('p', { class: 'muted w-none' }, `${every.length - all.length} more for other options. `,
    el('button', { class: 'ghost small', text: 'Show them', onclick: () => { state.only = false; $('w-only').checked = false; save(); drawSearch(); drawList(); } })));
  box.append(ul);
  return box;
}
