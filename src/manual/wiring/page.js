// Wiring Diagrams, one page: each diagram's page list and drawings from Ford (fetched once and kept for the session,
// the next pages fetched ahead), opening a page, the page buttons and the message box over the drawing.
'use strict';

// ---------- one page ----------
const pageLists = new Map(); // cell → Promise<[{ page, Code, … }]>
function pagesOf(cell) {
  const key = cellNo(cell);
  if (!pageLists.has(key)) {
    const p = api.wiringPages(key).then((r) => (Array.isArray(r) ? r : []), () => []);
    pageLists.set(key, p);
    p.then((r) => { if (!r.length) pageLists.delete(key); });
  }
  return pageLists.get(key);
}

// Drawings asked for once are kept (and the app keeps them on disk too); the next page and any link you point
// at are fetched ahead, so they open straight away
const svgs = new Map(); // "cell-page" → Promise<{ svg } | { error }>
function svgOf(cell, page) {
  const key = `${cellNo(cell)}-${Number(page) || 1}`;
  if (!svgs.has(key)) {
    const p = api.wiringSvg(cellNo(cell), Number(page) || 1).catch((e) => ({ error: true, message: String(e?.message || e) }));
    svgs.set(key, p);
    p.then((r) => { if (r?.error && svgs.get(key) === p) svgs.delete(key); });
    while (svgs.size > 60) svgs.delete(svgs.keys().next().value);
  }
  return svgs.get(key);
}
function warm(cell, page) { if (cellNo(cell)) { svgOf(cell, page); pagesOf(cell); } }

let openSeq = 0;
async function openPage(at, { push = true } = {}) {
  at = { cell: cellNo(at.cell), page: Number(at.page) || 1, item: String(at.item || ''), title: clean(at.title) || titleOf(at.cell) };
  if (!at.cell) return;
  if (push && state.at && !(state.at.cell === at.cell && state.at.page === at.page && state.at.item === at.item)) backList.push(state.at);
  if (backList.length > 50) backList.shift();
  state.at = at; save();
  hidePop();
  $('w-list-view').hidden = true;
  $('w-page-view').hidden = false;
  for (const id of ['btn-contents', 'w-pager', 'w-zoom', 'btn-print']) $(id).hidden = false;
  $('btn-back').hidden = !backList.length;
  const name = titleOf(at.cell) || at.title;
  $('w-title').textContent = `${at.cell}-${at.page}  ${name}`;
  $('w-sub').textContent = `${toc?.info.title || 'Wiring'}${at.item ? ` · showing ${at.item}` : ''}`;
  const seq = ++openSeq;
  // the last drawing stays until the new one is ready; "Loading" only shows if that takes a moment
  const slow = setTimeout(() => { if (seq === openSeq) { $('w-paper').replaceChildren(); message('Loading the diagram…'); } }, 250);
  fillPager(at, null);
  pagesOf(at.cell).then((pages) => {
    if (seq !== openSeq) return;
    fillPager(at, pages);
    const next = pages.map((p) => Number(p.page)).filter((n) => n > at.page).sort((a, b) => a - b)[0];
    if (next) setTimeout(() => warm(at.cell, next), 300);
  });
  const res = await svgOf(at.cell, at.page);
  clearTimeout(slow);
  if (seq !== openSeq) return;
  if (res?.error) {
    $('w-paper').replaceChildren();
    message(res.needsLogin ? 'PTS needs you to sign in again.' : res.message || "Couldn't load the diagram",
      res.needsLogin ? el('button', { class: 'primary', text: 'Sign in to PTS', onclick: async () => { await api.signIn(); openPage(at, { push: false }); } })
        : el('button', { class: 'primary', text: 'Try again', onclick: () => openPage(at, { push: false }) }),
      el('button', { class: 'ghost', text: "Ford's page", onclick: fordPage }));
    return;
  }
  try { draw(res.svg, res.url); } catch (e) {
    api.log(`wiring: couldn't draw ${at.cell}-${at.page}: ${e.message || e}`);
    $('w-paper').replaceChildren();
    message("Couldn't draw this diagram", el('button', { class: 'ghost', text: "Ford's page", onclick: fordPage }));
    return;
  }
  message('');
  finds = []; findAt = -1;
  if ($('w-find').value.trim()) findOnPage($('w-find').value);
  if (at.item) markItem(at.item); else if (!finds.length) fit();
  $('w-stage').focus({ preventScroll: true });
}

function fillPager(at, pages) {
  const sel = $('w-page');
  const list = pages && pages.length ? pages.map((p) => ({ n: Number(p.page) || 0, ok: pageApplies(p) })).filter((p) => p.n) : [{ n: at.page, ok: true }];
  if (!list.some((p) => p.n === at.page)) list.push({ n: at.page, ok: true });
  // "Only this vehicle": pages for other options are left out (unless it's the one showing)
  if (state.only && features) for (let i = list.length - 1; i >= 0; i--) if (!list[i].ok && list[i].n !== at.page) list.splice(i, 1);
  list.sort((a, b) => a.n - b.n);
  sel.replaceChildren(...list.map((p) => el('option', { value: String(p.n), class: p.ok ? '' : 'na', text: `Page ${p.n}${p.ok ? '' : ' (not this VIN)'}` })));
  sel.value = String(at.page);
  const i = list.findIndex((p) => p.n === at.page);
  $('btn-prev').disabled = i <= 0;
  $('btn-next').disabled = i < 0 || i >= list.length - 1;
  sel.disabled = list.length < 2;
  $('btn-prev').onclick = () => { if (i > 0) openPage({ cell: at.cell, page: list[i - 1].n }, { push: false }); };
  $('btn-next').onclick = () => { if (i < list.length - 1) openPage({ cell: at.cell, page: list[i + 1].n }, { push: false }); };
  sel.onchange = () => openPage({ cell: at.cell, page: Number(sel.value) }, { push: false });
}

function message(text, ...buttons) {
  const m = $('w-msg');
  m.hidden = !text;
  m.replaceChildren(...(text ? [el('p', { text }), ...(buttons.length ? [el('div', { class: 'actions' }, ...buttons)] : [])] : []));
}
