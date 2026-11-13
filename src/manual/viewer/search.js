// Workshop Manual, searching the contents (the box at the top, Ctrl+K). A DTC typed there also offers Ford's
// Diagnostics search; in PC/ED it lists that DTC's pinpoint tests and charts from the PC/ED book itself.
'use strict';

const DTC_LIKE = /(?:^|\s)([PBUC][0-9A-F]{4})(?:[:|][0-9A-F]{2})?(?=\s|$)/i;
function renderSearch(box, q) {
  const words = q.split(/\s+/).filter(Boolean);
  const dtc = q.match(DTC_LIKE);
  // PC/ED: a DTC finds its pinpoint tests and charts in this book (Ford's search), listed first
  const pcedBox = bookKind === 'pced' && dtc ? pcedDtcResults(q.trim()) : null;
  const diag = dtc ? el('button', { class: 'diag-offer', title: "Search Ford's diagnostics (pinpoint tests, TSBs, SSMs) for this code",
    onclick: () => api.openDiag(q.trim()) }, el('b', { text: `🔎 ${dtc[1].toUpperCase()}` }), ' Find pinpoint tests & TSBs in Diagnostics ›') : null;
  const hits = [];
  for (const n of leaves) {
    const hay = n.path.join(' ').toLowerCase();
    if (words.every((w) => hay.includes(w))) hits.push(n);
    if (hits.length >= 300) break;
  }
  if (!hits.length) return box.replaceChildren(...[pcedBox, diag, pcedBox ? null : el('div', { class: 'note', text: 'Nothing in the contents matches that.' })].filter(Boolean));
  const mark = (s) => {
    const frag = document.createDocumentFragment();
    const re = new RegExp(`(${words.map((w) => w.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')).join('|')})`, 'ig');
    let last = 0;
    for (const m of s.matchAll(re)) { frag.append(s.slice(last, m.index), el('mark', { text: m[0] })); last = m.index + m[0].length; }
    frag.append(s.slice(last));
    return frag;
  };
  box.replaceChildren(...[pcedBox, diag].filter(Boolean), ...hits.map((n) => leafNode(n, 0, el('span', {}, mark(n.title), el('span', { class: 'result-path', text: n.path.slice(0, -1).slice(-2).join(' › ') })))));
}

// A DTC in the PC/ED search: Ford's diagnostics search, PC/ED only (the same results its Diagnostics tab lists
// under PC/ED). Kept for the session, so typing doesn't search again.
const pcedSearches = new Map(); // DTC -> Promise<result>
function pcedDtcResults(q) {
  const m = q.match(/([PBUC][0-9A-F]{4})(?:[:|]([0-9A-F]{2}))?/i);
  const code = m[2] ? `${m[1]}:${m[2]}`.toUpperCase() : m[1].toUpperCase();
  const box = el('div', { class: 'pced-dtc' }, el('div', { class: 'note' }, el('div', { class: 'spinner' }), `Finding ${code} in the PC/ED…`));
  if (!pcedSearches.has(code)) {
    const p = api.diag({ dtcs: [code], sources: ['pced'], rows: 50 });
    pcedSearches.set(code, p);
    p.then((r) => { if (!r || r.error) pcedSearches.delete(code); }, () => pcedSearches.delete(code));
  }
  pcedSearches.get(code).then((res) => {
    if (res?.error) {
      box.replaceChildren(el('div', { class: 'note', text: res.needsLogin ? 'Sign in to PTS to search the PC/ED for DTCs.' : `Couldn't search the PC/ED: ${res.message || 'no answer'}` }));
      return;
    }
    const found = (res?.results || []).filter((r) => r.source === 'pced' && (r.searchNumber || r.procUid));
    api.log(`PC/ED ${code}: ${found.length} results${found.length ? `, e.g. ${found.slice(0, 3).map((r) => `${r.searchNumber}/${r.procUid} ppt ${r.ppt || '-'}`).join('; ')}` : ''}`);
    const info = (res?.dtcInfo || []).find((x) => String(x.name || '').toUpperCase().startsWith(m[1].toUpperCase()));
    const head = el('div', { class: 'pced-dtc-head' }, el('b', { text: code }), info?.description ? ` ${info.description}` : '',
      el('span', { class: 'muted', text: ` · ${found.length ? `${found.length} in the PC/ED` : 'nothing in the PC/ED'}` }));
    box.replaceChildren(head, ...found.map((r) => {
      const link = { searchNumber: r.searchNumber, procUid: r.procUid, linkType: 'Procedure', ppt: r.ppt || '', title: r.title, section: `PC/ED › ${code}` };
      return el('div', {
        class: `node doc d0${r.applies === false ? ' na' : ''}`, style: '--depth:0',
        title: [r.title, r.description].filter(Boolean).join('\n') + (r.applies === false ? "\n(may not apply to this vehicle)" : ''),
        onclick: (e) => openDoc(link, { newTab: e.ctrlKey }),
        onauxclick: (e) => { if (e.button === 1) openDoc(link, { newTab: true }); },
      }, el('span', { class: 'tw' }), el('span', { class: 'label' }, r.title || r.searchNumber,
        r.description ? el('span', { class: 'result-path', text: r.description.slice(0, 140) }) : null));
    }));
  });
  return box;
}
