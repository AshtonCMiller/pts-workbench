// Connected Vehicle › DTCs: each DTC with its module, the TSBs / GSBs / SSMs Ford has for it (as PTS lists them next to
// the DTC; a click opens the article in the TSBs tab), the Recent DTCs summary (every DTC in the loaded history, how
// often and when it was seen), and picking DTCs to search together in the Diagnostics tab.
'use strict';

// ---------- Ford's TSBs / GSBs / SSMs by DTC ----------
let arts = null;        // Ford's list (see manual.js cvArticles): { "B1081": [...], "B10DA (PCM)": [...] }, or null while loading
let artsFailed = '';
async function loadArticles() {
  const res = await ask(() => api.cvArticles());
  if (res?.error) { arts = {}; artsFailed = res.needsLogin ? 'sign in to PTS' : res.message || 'no answer'; api.log(`connected vehicle articles: ${artsFailed}`); }
  else { arts = res.list || {}; artsFailed = ''; }
  if (state.view === 'home') showHome();
}
// Ford's keys: the DTC, with its failure type, with its module, or both; an article keyed with a failure type or module only
// belongs to DTCs that match it
function articlesFor(code, ft, mod) {
  if (!arts) return [];
  const keys = [code, ft ? `${code}:${ft}` : null, mod ? `${code} (${mod})` : null, ft && mod ? `${code}:${ft} (${mod})` : null].filter(Boolean);
  const seen = new Set();
  const out = [];
  for (const k of keys) for (const a of arts[k] || []) if (!seen.has(a.id)) { seen.add(a.id); out.push(a); }
  return out;
}
function articleChip(a, short) {
  return el('button', { class: `cv-art${short ? ' short' : ''}`, title: `${a.type} ${a.id}: ${a.title}\nOpen it in the TSBs tab`,
    onclick: () => { api.openTsb({ id: a.id, type: a.type, title: a.title }); toast('Opening in the TSBs tab…'); } },
  el('span', { class: `kind ${a.type.toLowerCase()}`, text: a.type }), el('b', { text: a.id }), short ? null : el('span', { class: 't', text: a.title }));
}

// ---------- what each DTC means (Ford's titles, and what its failure type and status byte say) ----------
const descs = { titles: {}, ft: {}, sb: {} };
const descAsked = new Set(); // DTCs already asked about
let descBusy = false;
// every DTC in the current report and the loaded history
function allDtcs() {
  const cur = cvRes?.data?.diagCurrentData;
  const list = [...(cur?.Metadata?.dtcs || cur?.dtcs || [])];
  for (const ev of hist.events) list.push(...(ev.dtcs || []));
  return list.filter((x) => x?.dtcCode);
}
// asks Ford about DTCs not described yet, then redraws; called whenever the history grows
async function describeNew() {
  if (descBusy) return;
  const want = [];
  for (const x of allDtcs()) {
    const d = dtcInfo(x);
    const id = `${d.code}|${d.ft}|${d.sb}`;
    if (descAsked.has(id)) continue;
    descAsked.add(id);
    want.push({ code: d.code, ecu: String(x.ecuid ?? x.ecuId ?? ''), ft: d.ft, sb: d.sb });
  }
  if (!want.length) return;
  descBusy = true;
  const res = await ask(() => api.cvDescribe(want));
  descBusy = false;
  if (res?.error) { api.log(`connected vehicle DTC descriptions: ${res.message}`); return; }
  Object.assign(descs.titles, res.titles || {}); Object.assign(descs.ft, res.ft || {}); Object.assign(descs.sb, res.sb || {});
  if (state.view === 'home') showHome();
  describeNew(); // any that arrived meanwhile
}

// ---------- one DTC ----------
const moduleName = (ecu) => {
  const id = String(ecu ?? '').toUpperCase();
  return hist.modules[id]?.name || hist.modules[id.slice(-3)]?.name || (id ? id : '');
};
// what's needed about a DTC from Ford's report
function dtcInfo(x) {
  const mod = moduleName(x.ecuid ?? x.ecuId) || x.ecuName || '';
  const ft = x.failureType != null && x.failureType !== '' ? String(x.failureType).toUpperCase() : '';
  const sb = x.dtcsStatus ?? x.statusByte ?? x.dtcStatus ?? x.status;
  const code = String(x.dtcCode).toUpperCase();
  const ftText = ft ? descs.ft[ft] || '' : '';
  const sbText = sb != null && sb !== '' ? descs.sb[String(sb).toUpperCase()] || '' : '';
  return {
    code: String(x.dtcCode).toUpperCase(), ft, mod, sb: sb != null && sb !== '' ? String(sb) : '',
    key: ft ? `${String(x.dtcCode).toUpperCase()}:${ft}` : String(x.dtcCode).toUpperCase(), // what Diagnostics searches
    desc: x.dtcTitle || x.dtcDescription || x.description || descs.titles[code] || '',
    // what the failure type and status byte mean, for the DTC's tooltip
    detail: [ft ? `Failure type ${ft}${ftText ? `: ${ftText}` : ''}` : '', sb != null && sb !== '' ? `Status byte ${String(sb).toUpperCase()}${sbText ? `: ${sbText}` : ''}` : ''].filter(Boolean).join('\n'),
    modDesc: hist.modules[String(x.ecuid ?? x.ecuId ?? '').toUpperCase()]?.desc || (x.ecuid != null ? `Module ${x.ecuid}` : ''),
  };
}
function dtcRow(x) {
  const d = dtcInfo(x);
  const full = [d.code, d.ft, d.sb].filter(Boolean).join(':');
  const list = articlesFor(d.code, d.ft, d.mod);
  return el('div', { class: 'cv-code' },
    pickBox(d.key),
    el('span', { class: 'mod', text: d.mod, title: d.modDesc }),
    el('button', { class: 'dtc', title: `${d.detail ? `${d.detail}\n\n` : ''}Search this DTC in Diagnostics`, onclick: () => api.openDiag(d.key) }, full),
    d.desc ? el('span', { class: 'title', text: d.desc }) : null,
    list.length ? el('span', { class: 'cv-arts' }, ...list.map((a) => articleChip(a, true))) : null,
    el('button', { class: 'ghost small copy', text: 'Copy', title: 'Copy the DTC', onclick: () => copy(full) }));
}

// ---------- picking DTCs for Diagnostics ----------
const picked = new Set(); // "U2100:00"
function pickBox(key) {
  return el('input', { type: 'checkbox', class: 'cv-pick', 'data-dtc': key, title: 'Pick to search together in Diagnostics', checked: picked.has(key) ? '' : null,
    onchange: (e) => setPicked([key], e.target.checked) });
}
function setPicked(keys, on) {
  for (const k of keys) { if (on) picked.add(k); else picked.delete(k); }
  for (const box of document.querySelectorAll('input.cv-pick[data-dtc]')) box.checked = picked.has(box.dataset.dtc);
  const all = $('cv-pick-all');
  if (all) { const n = recentList().filter((r) => picked.has(r.key)).length; all.checked = n > 0 && n === recentList().length; all.indeterminate = n > 0 && !all.checked; }
  drawPickBar();
}
const DIAG_MAX = 20; // Diagnostics searches up to 20 DTCs at once
function drawPickBar() {
  let bar = $('cv-pickbar');
  if (!picked.size || state.view !== 'home') { if (bar) bar.remove(); return; }
  if (!bar) { bar = el('div', { id: 'cv-pickbar', class: 'cv-pickbar' }); document.body.append(bar); }
  const over = picked.size > DIAG_MAX;
  bar.replaceChildren(...[
    el('span', { class: 'n', text: `${picked.size} DTC${picked.size === 1 ? '' : 's'} picked` }),
    el('span', { class: 'list', text: [...picked].join(', ') }),
    over ? el('span', { class: 'over', text: `Diagnostics searches the first ${DIAG_MAX}` }) : null,
    el('button', { class: 'primary small', text: 'Search in Diagnostics', title: 'TSBs, GSBs, SSMs, pinpoint tests and procedures for these DTCs',
      onclick: () => { api.openDiag([...picked].slice(0, DIAG_MAX).join(' ')); } }),
    el('button', { class: 'ghost small', text: 'Copy', onclick: () => copy([...picked].join(', ')) }),
    el('button', { class: 'ghost small', text: 'Clear', onclick: () => setPicked([...picked], false) }),
  ].filter(Boolean));
}

// ---------- Recent DTCs: each DTC in the loaded history once ----------
function recentList() {
  const map = new Map();
  const add = (x, when, odo, now) => {
    if (!x?.dtcCode) return;
    const d = dtcInfo(x);
    const id = `${d.mod}|${d.key}`;
    let r = map.get(id);
    if (!r) { r = { ...d, count: 0, first: null, last: null, lastOdo: null, now: false }; map.set(id, r); }
    r.count++;
    if (now) r.now = true;
    const t = toDate(when);
    if (t && (!r.last || t > r.last)) { r.last = t; r.lastOdo = odo; }
    if (t && (!r.first || t < r.first)) r.first = t;
    if (!r.desc && d.desc) r.desc = d.desc;
  };
  const cur = cvRes?.data?.diagCurrentData;
  for (const x of cur?.Metadata?.dtcs || cur?.dtcs || []) add(x, cur.eventTime, null, true);
  for (const ev of hist.events) for (const x of ev.dtcs || []) add(x, ev.eventTime || ev.eventDate, ev.odometer, false);
  return [...map.values()].sort((a, b) => (b.last?.getTime() || 0) - (a.last?.getTime() || 0));
}

function drawRecent() {
  const box = $('cv-recent');
  if (!box) return;
  const list = recentList();
  const back = hist.oldest ? Math.round((Date.now() - hist.oldest) / 86400000) : null;
  const allOn = list.length > 0 && list.every((r) => picked.has(r.key));
  const head = el('h2', {}, 'Recent DTCs', back ? el('span', { class: 'cv-when', text: `last ${back} days` }) : null,
    arts == null ? el('span', { class: 'cv-when', text: '· checking TSBs…' }) : artsFailed ? el('span', { class: 'cv-when', text: `· couldn't check TSBs (${artsFailed})` }) : null);
  if (!list.length) { box.replaceChildren(head, msg(hist.loading ? 'Loading the history…' : 'No DTCs reported in this time.', 'ok')); return; }
  const rows = list.map((r) => {
    const list2 = articlesFor(r.code, r.ft, r.mod);
    return el('tr', {},
      el('td', {}, pickBox(r.key)),
      el('td', { class: 'mod', text: r.mod, title: r.modDesc }),
      el('td', {}, el('button', { class: 'dtc', title: `${r.detail ? `${r.detail}\n\n` : ''}Search this DTC in Diagnostics`, onclick: () => api.openDiag(r.key) }, r.key),
        r.desc ? el('div', { class: 'title', text: r.desc }) : null),
      el('td', { class: 'seen' }, `${r.count}×`, r.now ? el('span', { class: 'now', text: 'now' }) : null),
      el('td', { class: 'seen' }, r.last ? fmtDate(r.last) : '—', r.lastOdo != null && r.lastOdo !== '' ? el('div', { class: 'desc', text: dist(Number(r.lastOdo)) }) : null),
      el('td', { class: 'seen' }, r.first ? fmtDate(r.first) : '—'),
      el('td', {}, list2.length ? el('div', { class: 'cv-arts col' }, ...list2.map((a) => articleChip(a, false))) : el('span', { class: 'desc', text: arts ? 'None' : '' })));
  });
  box.replaceChildren(head,
    el('div', { class: 'o-card cv-recent' }, el('table', { class: 'cv-table' },
      el('thead', {}, el('tr', {},
        el('th', {}, el('input', { type: 'checkbox', id: 'cv-pick-all', title: 'Pick them all', checked: allOn ? '' : null, onchange: (e) => setPicked(list.map((r) => r.key), e.target.checked) })),
        ...['Module', 'DTC', 'Seen', 'Last seen', 'First seen', 'TSBs / GSBs / SSMs'].map((h) => el('th', { text: h })))),
      el('tbody', {}, ...rows))));
  const all = $('cv-pick-all');
  if (all) all.indeterminate = !allOn && list.some((r) => picked.has(r.key));
}
