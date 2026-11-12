// Diagnostics: Ford's "Enhanced Search" for this vehicle, in the app's style. Type DTCs and/or words; results come
// back grouped like Ford's Diagnostics tab (TSB/GSB/SSM, pinpoint tests, procedures, PC/ED, wiring, videos).
// Pinpoint tests and procedures open in this vehicle's Workshop Manual tab, at the test.
'use strict';
const api = window.manual;
const $ = (id) => document.getElementById(id);
function el(tag, props = {}, ...kids) {
  const n = document.createElement(tag);
  for (const [k, v] of Object.entries(props)) {
    if (v == null || v === false) continue;
    if (k === 'class') n.className = v;
    else if (k === 'text') n.textContent = v;
    else if (k.startsWith('on')) n.addEventListener(k.slice(2), v);
    else n.setAttribute(k, v);
  }
  for (const c of kids) if (c != null && c !== false) n.append(c);
  return n;
}
function toast(text) {
  const t = $('o-toast');
  t.textContent = text; t.hidden = false;
  clearTimeout(toast.timer);
  toast.timer = setTimeout(() => { t.hidden = true; }, 1600);
}

// The groups, in Ford's order
const GROUPS = [
  { id: 'tsb', label: 'TSB / GSB / SSM', sources: ['tsb', 'gsb', 'ssm'] },
  { id: 'wsmppt', label: 'Pinpoint tests', sources: ['wsmppt'] },
  { id: 'wsmproc', label: 'Workshop procedures', sources: ['wsmproc'] },
  { id: 'pced', label: 'PC/ED', sources: ['pced'] },
  { id: 'wiring', label: 'Wiring', sources: ['wiring'] },
  { id: 'tsv', label: 'Support videos', sources: ['tsv'] },
];
const ROWS = 50;
// Like Ford's: P0420, U3000:49, U3000:49:ABS, B1A2B|87
const DTC_RE = /(?<=\s|^|[,;:])[PBUC][0-9A-F]{4,6}(?:[|:][0-9A-F]{2}(?:[|:][A-Z]{2,12})?|[|:][A-Z]{2,12})?(?=\s|$|[,;])/gi;

let state = { query: '', exact: false, group: '' };
let search = null; // { text, dtcs, counts, dtcInfo, lists: { group: { items, more, loading, error } } }
let token = 0;

function save() { api.save({ ...state }); }

function parseQuery(q) {
  const dtcs = [];
  let text = ` ${q} `.replace(DTC_RE, (m) => { dtcs.push(m.toUpperCase().replace(/\|/g, ':')); return ' '; });
  text = text.replace(/[,;]/g, ' ').replace(/\s+/g, ' ').trim();
  return { dtcs: [...new Set(dtcs)], text };
}

async function start() {
  const init = await api.init();
  if (init?.state) state = { ...state, ...init.state };
  $('d-sub').textContent = [init?.ymm, init?.vin, init?.ro ? `RO ${init.ro}` : ''].filter(Boolean).join(' · ');
  $('d-q').value = state.query || '';
  $('d-exact').checked = !!state.exact;
  if (state.query) run(state.query); else showHint();
  $('d-q').focus();
}

function showHint() {
  $('d-tabs').hidden = true;
  $('d-body').replaceChildren(el('div', { class: 'd-hint' },
    el('p', {}, 'Search this vehicle the way Ford\'s Diagnostics tab does: type one or more ', el('b', { text: 'DTCs' }),
      ' (P0420, U3000:49) and/or ', el('b', { text: 'words' }), ' (no crank, liftgate).'),
    el('p', { text: 'You get the matching TSBs, GSBs and SSMs, pinpoint tests and workshop procedures. Pinpoint tests open right at the test in this vehicle\'s Workshop Manual tab.' })));
}

function problem(res, retry) {
  const box = el('div', { class: 'o-card o-msg' });
  box.append(res.needsLogin ? 'Ford needs you to sign in to PTS first.' : `Couldn't search: ${res.message || 'no answer from Ford'}`);
  if (res.needsLogin) box.append(el('button', { class: 'primary', text: 'Sign in to PTS', onclick: async (e) => { e.target.disabled = true; await api.signIn(); retry(); } }));
  else box.append(el('button', { class: 'ghost small', text: 'Try again', onclick: retry }));
  return box;
}

async function run(q) {
  q = String(q || '').trim();
  state.query = q; state.exact = $('d-exact').checked; save();
  if (!q) { search = null; showHint(); return; }
  const { dtcs, text } = parseQuery(q);
  const my = ++token;
  search = { text, dtcs, counts: null, dtcInfo: [], lists: {} };
  $('d-tabs').hidden = true;
  $('d-body').replaceChildren(el('div', { class: 'o-msg' }, el('div', { class: 'spinner' }), `Searching Ford for ${q}…`));
  // 1) how many of each kind (Ford asks for one row of everything first)
  const res = await api.diag({ text, dtcs, rows: 1, exact: state.exact });
  if (my !== token) return;
  if (!res || res.error) { $('d-body').replaceChildren(problem(res || {}, () => run(q))); return; }
  search.counts = res.counts || {};
  search.dtcInfo = res.dtcInfo || [];
  const count = (g) => g.sources.reduce((t, s) => t + (search.counts[s] || 0), 0);
  // Stay on the chosen group if it has results; otherwise the first group that does (like Ford)
  const withResults = GROUPS.filter((g) => count(g) > 0);
  if (!withResults.find((g) => g.id === state.group)) state.group = (withResults[0] || GROUPS[0]).id;
  save();
  renderTabs();
  if (!withResults.length) {
    $('d-body').replaceChildren(dtcInfoBox(), el('div', { class: 'd-empty', text: `Nothing found for ${q} on this vehicle.` }));
    return;
  }
  loadGroup(state.group);
}

function renderTabs() {
  const count = (g) => g.sources.reduce((t, s) => t + (search.counts?.[s] || 0), 0);
  const tabs = GROUPS.filter((g) => g.id !== 'pced' || count(g) > 0).map((g) => el('button', {
    class: `d-tab${g.id === state.group ? ' on' : ''}${count(g) ? '' : ' empty'}`, type: 'button',
    onclick: () => { state.group = g.id; save(); renderTabs(); loadGroup(g.id); },
  }, g.label, el('span', { class: 'n', text: String(count(g)) })));
  $('d-tabs').replaceChildren(...tabs);
  $('d-tabs').hidden = false;
}

function dtcInfoBox() {
  const info = (search?.dtcInfo || []).filter((x) => x.description);
  if (!info.length) return null;
  return el('div', { class: 'd-dtcinfo' }, ...info.map((x) => el('div', {}, el('b', { text: x.name }), x.description)));
}

async function loadGroup(id, more = false) {
  const g = GROUPS.find((x) => x.id === id);
  if (!g || !search) return;
  const list = (search.lists[id] ||= { items: [], total: g.sources.reduce((t, s) => t + (search.counts?.[s] || 0), 0) });
  if (!more && list.items.length) return render();
  if (!list.total) return render();
  list.loading = true; list.error = null;
  render();
  const my = token;
  const res = await api.diag({ text: search.text, dtcs: search.dtcs, sources: g.sources, rows: ROWS, start: list.items.length, exact: state.exact });
  if (my !== token) return;
  list.loading = false;
  if (!res || res.error) list.error = res || {};
  else list.items.push(...(res.results || []));
  render();
}

const KIND = { tsb: 'TSB', gsb: 'GSB', ssm: 'SSM', wsmppt: 'PPT', wsmproc: 'WSM', pced: 'PC/ED', wiring: 'WIRING', tsv: 'VIDEO' };
function when(d) {
  if (!d) return '';
  const m = String(d).match(/^(\d{4})-(\d{2})-(\d{2})/); // a date without a time: that calendar day, not UTC midnight
  const t = m ? new Date(+m[1], +m[2] - 1, +m[3]) : new Date(d);
  return Number.isNaN(t.getTime()) ? String(d) : t.toLocaleDateString([], { year: 'numeric', month: 'short', day: 'numeric' });
}

function open(r) {
  if (['wsmppt', 'wsmproc', 'pced'].includes(r.source)) {
    api.openProc({ searchNumber: r.searchNumber, procUid: r.procUid, ppt: r.ppt, title: r.title });
    toast('Opening in the Workshop Manual tab…');
  } else if (['tsb', 'gsb', 'ssm'].includes(r.source) && r.article) {
    api.openTsb({ id: r.article, type: r.source, title: r.title, url: r.url });
    toast('Opening in the TSBs tab…');
  } else if (r.url) {
    api.openUrl(r.url);
  } else toast("Ford didn't give a link for this one");
}

function item(r) {
  const kind = KIND[r.source] || r.source.toUpperCase();
  const go = ['wsmppt', 'wsmproc', 'pced'].includes(r.source) ? 'Open in manual ›' : ['tsb', 'gsb', 'ssm'].includes(r.source) ? 'Open in TSBs ›' : 'Open ›';
  return el('button', { class: 'd-item', type: 'button', onclick: () => open(r), title: r.title },
    el('div', { class: 't' }, el('span', { class: `kind ${r.source === 'wsmppt' ? 'ppt' : r.source}`, text: kind }),
      el('span', { text: r.article ? `${r.article} — ${r.title}` : r.title }), el('span', { class: 'go', text: go })),
    r.description ? el('div', { class: 'desc', text: r.description }) : null,
    ((r.date && r.article) || !r.applies) ? el('div', { class: 'meta' },
      r.date && r.article ? el('span', { text: `Effective ${when(r.date)}` }) : null,
      !r.applies ? el('span', { class: 'warn', text: 'May not apply to this vehicle’s options' }) : null) : null);
}

function render() {
  const id = state.group;
  const list = search?.lists[id];
  const kids = [dtcInfoBox()];
  if (!list || (!list.items.length && list.loading)) kids.push(el('div', { class: 'o-msg' }, el('div', { class: 'spinner' }), 'Loading…'));
  else if (list.error && !list.items.length) kids.push(problem(list.error, () => loadGroup(id, true)));
  else if (!list.items.length) kids.push(el('div', { class: 'd-empty', text: 'Nothing here for this search.' }));
  else {
    kids.push(el('div', { class: 'd-list' }, ...list.items.map(item)));
    if (list.items.length < list.total) {
      kids.push(el('div', { class: 'd-more' }, list.loading
        ? el('span', { class: 'o-msg' }, el('div', { class: 'spinner' }), 'Loading more…')
        : el('button', { class: 'ghost', text: `Show more (${list.items.length} of ${list.total})`, onclick: () => loadGroup(id, true) })));
    }
  }
  $('d-body').replaceChildren(...kids.filter(Boolean));
}

// DTCs the vehicle last reported (connected vehicle data): click one to search it
async function loadVehicleDtcs() {
  const btn = $('btn-cv');
  btn.disabled = true; btn.textContent = 'Asking Ford…';
  const res = await api.cv();
  btn.disabled = false; btn.textContent = 'DTCs from the vehicle';
  const chips = $('d-chips');
  if (!res || res.error) { chips.hidden = false; chips.replaceChildren(el('span', { class: 'lbl', text: res?.needsLogin ? 'Sign in to PTS to get the vehicle\'s DTCs.' : `Couldn't get them (${res?.message || 'no answer'}).` })); return; }
  const dtcs = (res.data?.diagCurrentData?.Metadata?.dtcs || []).filter((x) => x && x.dtcCode);
  chips.hidden = false;
  if (!dtcs.length) { chips.replaceChildren(el('span', { class: 'lbl', text: 'The vehicle didn\'t report any DTCs.' })); return; }
  chips.replaceChildren(el('span', { class: 'lbl', text: 'Reported by the vehicle:' }),
    ...dtcs.map((x) => el('button', { class: 'd-chip', type: 'button', title: x.dtcDescription || x.description || 'Search this DTC',
      onclick: () => { $('d-q').value = x.dtcCode; run(x.dtcCode); } }, x.dtcCode, x.ecuName || x.ecuId ? el('small', { text: x.ecuName || x.ecuId }) : null)),
    dtcs.length > 1 ? el('button', { class: 'd-chip', type: 'button', title: 'Search all of them together',
      onclick: () => { const q = [...new Set(dtcs.map((x) => x.dtcCode))].join(' '); $('d-q').value = q; run(q); } }, 'All') : null);
}

api.onOpen?.((link) => { if (link?.query) { $('d-q').value = link.query; run(link.query); } });
$('d-form').addEventListener('submit', (e) => { e.preventDefault(); run($('d-q').value); });
$('d-exact').addEventListener('change', () => { if (state.query) run($('d-q').value); });
$('btn-cv').onclick = loadVehicleDtcs;
window.addEventListener('error', (e) => api.log(`diagnostics error: ${e.message} at ${e.lineno}`));
start();