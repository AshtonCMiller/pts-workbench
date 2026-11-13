// Diagnostics, the search: what was typed split into DTCs and words, Ford's search run for each group (TSB/GSB/SSM,
// pinpoint tests, procedures, PC/ED, wiring, videos), the group tabs with their counts, and "Show more".
'use strict';

function parseQuery(q) {
  const dtcs = [];
  let text = ` ${q} `.replace(DTC_RE, (m) => { dtcs.push(m.toUpperCase().replace(/\|/g, ':')); return ' '; });
  text = text.replace(/[,;]/g, ' ').replace(/\s+/g, ' ').trim();
  return { dtcs: [...new Set(dtcs)], text };
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
