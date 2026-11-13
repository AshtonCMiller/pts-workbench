// Wiring Diagrams, links in the drawing: a page number (e.g. 151-2) opens that page in the tab; a connector, splice or
// ground shows a small box with where it is (location views) and its connector face (Ford's page, as a tab).
'use strict';

// ---------- links in the drawing ----------
function clickAt(e) {
  const a = e.target.closest?.('a.w-link');
  if (!a) { hidePop(); return; }
  const href = a.getAttribute('data-href');
  if (href && /\/wiring\/page/i.test(href)) {
    let sp;
    try { sp = new URL(href.replace(/&amp;/g, '&'), 'https://www.fordtechservice.dealerconnection.com/').searchParams; } catch { return; }
    openPage({ cell: sp.get('cell'), page: sp.get('page'), item: sp.get('item') || '', title: sp.get('title') || '' });
    return;
  }
  if (href) { api.openArticle(new URL(href.replace(/&amp;/g, '&'), 'https://www.fordtechservice.dealerconnection.com/').href, 'Wiring'); return; }
  const name = a.getAttribute('name');
  if (name) showPop(e, name, a.getAttribute('faceview') || '');
}

function hidePop() { $('w-pop').hidden = true; }
async function showPop(e, name, face) {
  const pop = $('w-pop');
  const kind = /^C/i.test(name) ? 'Connector' : /^S/i.test(name) ? 'Splice' : /^G/i.test(name) ? 'Ground' : 'Item';
  const list = el('div', { class: 'w-locs' }, el('p', { class: 'muted', text: 'Finding where it is…' }));
  pop.replaceChildren(
    el('div', { class: 'w-pop-head' }, el('b', { text: name }), el('span', { class: 'muted', text: kind }),
      el('button', { class: 'icon', text: '×', title: 'Close', onclick: hidePop })),
    face ? el('button', { class: 'ghost w-face', text: 'Connector face (pin view) ›', onclick: () => {
      api.wiringFord({ kind: 'face', cell: '150', page: '', item: face, title: `${name} face` });
      toast("Opening Ford's connector view as a tab…"); hidePop();
    } }) : (kind === 'Connector' ? el('p', { class: 'muted', text: 'No connector face for this one' }) : null),
    list);
  pop.hidden = false;
  const r = pop.getBoundingClientRect();
  pop.style.left = `${Math.max(8, Math.min(window.innerWidth - r.width - 8, e.clientX + 12))}px`;
  pop.style.top = `${Math.max(8, Math.min(window.innerHeight - r.height - 8, e.clientY + 12))}px`;
  let locs;
  try { locs = await api.wiringLocations(name); } catch (err) { locs = { error: true, message: String(err?.message || err) }; }
  if (pop.hidden) return;
  if (locs?.error || !Array.isArray(locs)) { list.replaceChildren(el('p', { class: 'muted', text: locs?.needsLogin ? 'Sign in to PTS to see where it is' : "Couldn't find where it is" })); return; }
  const rows = locs.filter((l) => l && l.Page != null && String(l.Page) !== '');
  if (!rows.length) { list.replaceChildren(el('p', { class: 'muted', text: 'No location view for this one' })); return; }
  const pages = await pagesOf('151');
  const fits = (n) => { const p = pages.find((x) => Number(x.page) === Number(n)); return p ? pageApplies(p) : true; };
  list.replaceChildren(el('div', { class: 'muted small', text: 'Where it is (location views):' }),
    ...rows.map((l) => el('button', { class: `ghost w-loc${fits(l.Page) ? '' : ' na'}`,
      onclick: () => { hidePop(); openPage({ cell: '151', page: l.Page, item: name, title: `${name} location` }); } },
    `${l.Qual ? `${clean(l.Qual)}, ` : ''}${clean(l.LocationDesc) || 'Location'} `, el('span', { class: 'muted', text: `(151-${Number(l.Page)})` }))));
}
