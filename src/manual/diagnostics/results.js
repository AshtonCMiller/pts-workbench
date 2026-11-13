// Diagnostics, the results: one group's list, each result with its kind, title, date and description, and opening it
// where it belongs (pinpoint tests / procedures in Workshop Manual or PC/ED at the test, TSBs in the TSBs tab, wiring
// in Wiring Diagrams, others as Ford's page).
'use strict';

const KIND = { tsb: 'TSB', gsb: 'GSB', ssm: 'SSM', wsmppt: 'PPT', wsmproc: 'WSM', pced: 'PC/ED', wiring: 'WIRING', tsv: 'VIDEO' };
function when(d) {
  if (!d) return '';
  const m = String(d).match(/^(\d{4})-(\d{2})-(\d{2})/); // a date without a time: that calendar day, not UTC midnight
  const t = m ? new Date(+m[1], +m[2] - 1, +m[3]) : new Date(d);
  return Number.isNaN(t.getTime()) ? String(d) : t.toLocaleDateString([], { year: 'numeric', month: 'short', day: 'numeric' });
}

function open(r) {
  if (['wsmppt', 'wsmproc', 'pced'].includes(r.source)) {
    const pced = r.source === 'pced'; // PC/ED results open in the PC/ED tab (its own book)
    api.openProc({ searchNumber: r.searchNumber, procUid: r.procUid, ppt: r.ppt, title: r.title, book: pced ? 'pced' : '' });
    toast(pced ? 'Opening in the PC/ED tab…' : 'Opening in the Workshop Manual tab…');
  } else if (['tsb', 'gsb', 'ssm'].includes(r.source) && r.article) {
    api.openTsb({ id: r.article, type: r.source, title: r.title, url: r.url });
    toast('Opening in the TSBs tab…');
  } else if (r.source === 'wiring' && /^\/wiring\/page\?/i.test(r.url || '')) {
    const sp = new URL(r.url, 'https://x/').searchParams;
    api.openWiring({ cell: sp.get('cell'), page: sp.get('page'), item: sp.get('item') || '', title: r.title });
    toast('Opening in the Wiring Diagrams tab…');
  } else if (r.url) {
    api.openUrl(r.url);
  } else toast("Ford didn't give a link for this one");
}

function item(r) {
  const kind = KIND[r.source] || r.source.toUpperCase();
  const go = r.source === 'pced' ? 'Open in PC/ED ›' : ['wsmppt', 'wsmproc'].includes(r.source) ? 'Open in manual ›' : ['tsb', 'gsb', 'ssm'].includes(r.source) ? 'Open in TSBs ›' : 'Open ›';
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
