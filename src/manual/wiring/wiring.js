// Wiring Diagrams: the vehicle's wiring book from Ford (the same lists and drawings PTS's /wiring pages use).
// Contents: Ford's diagrams, gray when they don't apply to this VIN (from the vehicle's option codes).
// A page: Ford's drawing, shown here with zoom / drag, page buttons, and its links working inside the tab:
// blue page numbers (e.g. 151-2) open that page, connectors / splices / grounds show where they are.
// Connector faces and Ford's other wiring pages open as a tab of Ford's own page.
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
const clean = (t) => String(t || '').replace(/ /g, ' ').replace(/\s+/g, ' ').trim();
const cellNo = (c) => String(Number(String(c || '').replace(/\D/g, '')) || '');
function toast(text) {
  const t = $('o-toast');
  t.textContent = text; t.hidden = false;
  clearTimeout(toast.timer);
  toast.timer = setTimeout(() => { t.hidden = true; }, 1800);
}

let state = { q: '', sort: 'cell', only: false, at: null }; // at: { cell, page, item, title } — the page showing
let backList = [];
function save() { api.save({ q: state.q, sort: state.sort, only: state.only, at: state.at }); }

// ---------- does it apply to this VIN? (as Ford's wiring pages work it out) ----------
let features = null; // { pfc, mfc, vehicleLine, buildDate } or null (then everything shows as applying)
function cellApplies(it) {
  if (!features || !Array.isArray(it.Code) || !it.Code.length) return true;
  return it.Code.some((c) => {
    const code = String(c).replace('-', '').replace('_', '0');
    return features.pfc.includes(code) || features.mfc.includes(code);
  });
}
function pageApplies(pg) {
  if (!features) return true;
  const build = parseInt(features.buildDate, 10);
  const day = (v) => (v && String(v).length ? parseInt(String(v).replace(/-/g, ''), 10) : NaN);
  const sd = day(pg.Startdate), ed = day(pg.Enddate);
  if (build && !Number.isNaN(sd) && !(sd <= build)) return false;
  if (build && !Number.isNaN(ed) && !(ed >= build)) return false;
  const groups = Array.isArray(pg.Code) && pg.Code.length ? String(pg.Code).replace(/[()]/g, '') : '';
  if (groups) {
    const all = [...features.pfc, ...features.mfc, ...features.vehicleLine];
    for (const g of groups.split('+')) {
      const quals = g.toUpperCase().split('|');
      if (!quals.some((q) => (q.startsWith('!') ? !all.includes(q.slice(1)) : all.includes(q)))) return false;
    }
  }
  return true;
}

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
  if (!list.length) ul.append(el('p', { class: 'muted', text: 'Nothing matches.' }));
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
  try { draw(res.svg); } catch (e) {
    api.log(`wiring: couldn't draw ${at.cell}-${at.page}: ${e.message || e}`);
    $('w-paper').replaceChildren();
    message("Couldn't draw this diagram", el('button', { class: 'ghost', text: "Ford's page", onclick: fordPage }));
    return;
  }
  message('');
  if (at.item) markItem(at.item); else fit();
  $('w-stage').focus({ preventScroll: true });
}

function fillPager(at, pages) {
  const sel = $('w-page');
  const list = pages && pages.length ? pages.map((p) => ({ n: Number(p.page) || 0, ok: pageApplies(p) })).filter((p) => p.n) : [{ n: at.page, ok: true }];
  if (!list.some((p) => p.n === at.page)) list.push({ n: at.page, ok: true });
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

// Ford's drawing, without its scripts, click handlers or inline styles (this page's rules don't allow them)
let size = { w: 1100, h: 850 };
function draw(text) {
  let src = String(text).replace(/<\?xml[\s\S]*?\?>/i, '').replace(/<!DOCTYPE[\s\S]*?>/i, '').replace(/<script\b[\s\S]*?<\/script>/gi, '');
  // inline styles are dropped: Ford's file starts with its sheet hidden ("visibility:hidden") and its page script shows it
  src = src.replace(/\sstyle\s*=\s*("[^"]*"|'[^']*')/gi, '');
  // Ford's notes about the drawing (<desc>, <metadata>) aren't drawn, and read as a web page they can swallow the
  // whole drawing (an empty <Qualifier/> there isn't closed in a web page), so they're taken out first
  src = src.replace(/<(desc|metadata)\b[\s\S]*?<\/\1\s*>/gi, '').replace(/<(desc|metadata)\b[^>]*\/>/gi, '');
  // read as a drawing (SVG / XML) first; if Ford's file isn't tidy enough for that, as a web page
  let svg = null;
  const xml = new DOMParser().parseFromString(/xmlns\s*=\s*["']http:\/\/www\.w3\.org\/2000\/svg/i.test(src) ? src
    : src.replace(/<svg\b/i, '<svg xmlns="http://www.w3.org/2000/svg"'), 'image/svg+xml');
  if (!xml.querySelector('parsererror') && xml.documentElement?.localName === 'svg') svg = xml.documentElement;
  if (!svg) {
    const doc = new DOMParser().parseFromString(`<!doctype html><body>${src}</body>`, 'text/html');
    svg = doc.querySelector('svg');
  }
  if (!svg) throw new Error('no drawing in the reply');
  svg.querySelectorAll('script, foreignObject, iframe, object, embed').forEach((n) => n.remove());
  const hidden = [];
  for (const n of [svg, ...svg.querySelectorAll('*')]) {
    const tag = n.localName.toLowerCase();
    for (const a of [...n.attributes]) {
      const name = a.name.toLowerCase();
      if (name.startsWith('on')) n.removeAttribute(a.name);
      else if (name === 'style') n.removeAttribute(a.name);
      else if (name === 'visibility' && /hidden/i.test(a.value)) { hidden.push(`${tag}#${n.id || '?'} visibility`); n.removeAttribute(a.name); } // Ford's page script shows these
      else if (name === 'display' && /none/i.test(a.value)) hidden.push(`${tag}#${n.id || '?'} display:none`);
      else if (name === 'href' || name === 'xlink:href') {
        const v = a.value.trim();
        if (tag === 'a') { n.setAttribute('data-href', v); n.removeAttribute(a.name); }
        else if (v.startsWith('#')) { /* a shape drawn from elsewhere in the drawing (<use>): kept */ }
        else if (tag === 'image' && /^data:image\//i.test(v)) { /* a picture inside the file: kept */ }
        else if (tag === 'image' && /^(https?:)?\/\//i.test(v) || (tag === 'image' && /^\//.test(v))) {
          let abs = '';
          try { abs = new URL(v, 'https://www.fordservicecontent.com/').href; } catch { /* bad address */ }
          if (abs) n.setAttribute(a.name, `ptsasset://f/?u=${encodeURIComponent(abs)}`); else n.removeAttribute(a.name);
        } else n.removeAttribute(a.name);
      }
    }
  }
  if (hidden.length) api.log(`wiring: hidden in Ford's file: ${hidden.slice(0, 20).join(', ')}`);
  const vb = (svg.getAttribute('viewBox') || '').trim().split(/[\s,]+/).map(Number);
  size = vb.length === 4 && vb[2] > 0 && vb[3] > 0 ? { x: vb[0], y: vb[1], w: vb[2], h: vb[3] }
    : { x: 0, y: 0, w: parseFloat(svg.getAttribute('width')) || 1100, h: parseFloat(svg.getAttribute('height')) || 850 };
  if (!(vb.length === 4)) svg.setAttribute('viewBox', `0 0 ${size.w} ${size.h}`);
  svg.setAttribute('width', '100%'); svg.setAttribute('height', '100%');
  svg.setAttribute('preserveAspectRatio', 'xMidYMid meet');
  svg.removeAttribute('id');
  for (const a of svg.querySelectorAll('a[data-href], a[name]')) a.classList.add('w-link');
  themeColors(svg);
  // the drawing's sheet(s): shown, as Ford's page does
  for (const g of svg.querySelectorAll('g[id^="Sheet"]')) { g.removeAttribute('visibility'); g.removeAttribute('display'); }
  $('w-paper').replaceChildren(document.importNode(svg, true));
  zoomed = false;
  // a record of what was drawn, for working out problems
  const shown = $('w-paper').querySelector('svg');
  let box = null;
  try { const b = shown.getBBox(); box = `${Math.round(b.x)},${Math.round(b.y)} ${Math.round(b.width)}×${Math.round(b.height)}`; } catch { /* not laid out */ }
  const tags = {};
  for (const n of shown.querySelectorAll('*')) tags[n.localName] = (tags[n.localName] || 0) + 1;
  const kids = [...shown.children].slice(0, 12).map((k) => { let b = ''; try { const r = k.getBBox(); b = ` ${Math.round(r.width)}×${Math.round(r.height)}`; } catch {} return `${k.localName}${k.id ? `#${k.id}` : ''}${b}`; });
  api.log(`wiring: elements ${Object.entries(tags).map(([k, v]) => `${k} ${v}`).join(', ')} | top: ${kids.join(', ')} | namespace ${shown.namespaceURI}`);
  api.log(`wiring: drew ${String(text).length} characters, ${shown.querySelectorAll('line, path, polyline, polygon, rect, text').length} shapes, ` +
    `view ${size.w}×${size.h}, drawing at ${box || '?'}; starts: ${String(text).replace(/\s+/g, ' ').slice(0, 300)}`);
}

// The app's colors: Ford's black lines and text, white paper, light gray boxes and blue links are tagged so the
// page's style can draw them in the app's light / dark colors. Wire colors (red, pink, blue wires…) stay as Ford drew them.
function colorKind(v, isText) {
  const c = String(v || '').trim().toLowerCase().replace(/\s+/g, '');
  if (!c || c === 'none' || c.startsWith('url(')) return '';
  let rgb = null;
  const named = { black: [0, 0, 0], white: [255, 255, 255], blue: [0, 0, 255], gray: [128, 128, 128], grey: [128, 128, 128], purple: [128, 0, 128] };
  if (named[c]) rgb = named[c];
  else if (/^#[0-9a-f]{3}$/.test(c)) rgb = [1, 2, 3].map((i) => parseInt(c[i] + c[i], 16));
  else if (/^#[0-9a-f]{6}$/.test(c)) rgb = [1, 3, 5].map((i) => parseInt(c.slice(i, i + 2), 16));
  else { const m = c.match(/^rgb\((\d+),(\d+),(\d+)\)$/); if (m) rgb = [m[1], m[2], m[3]].map(Number); }
  if (!rgb) return '';
  const [r, g, b] = rgb;
  const grayish = Math.max(r, g, b) - Math.min(r, g, b) < 12;
  if (grayish && r < 60) return 'ink';
  if (grayish && r > 245) return 'paper';
  if (grayish && r >= 200) return 'tint';                       // light gray boxes
  if (b > 180 && r < 90 && g < 90) return 'link';              // Ford's blue: page numbers, connector names, component outlines
  return '';
}
function themeColors(svg) {
  for (const n of svg.querySelectorAll('[fill], [stroke]')) {
    const isText = n.localName === 'text' || n.localName === 'tspan';
    const f = colorKind(n.getAttribute('fill'), isText), st = colorKind(n.getAttribute('stroke'), false);
    if (f) n.classList.add(`f-${f}`);
    if (st && !(st === 'link' && n.closest('[id^="CIRC"]'))) n.classList.add(`s-${st}`); // a blue wire stays Ford's blue
  }
  // text with no fill of its own is black
  for (const t of svg.querySelectorAll('text:not([fill])')) if (!t.closest('[fill]')) t.classList.add('f-ink');
}

// ---------- zoom and drag ----------
const view = { s: 1, x: 0, y: 0 };
let zoomed = false;
function fitScale() {
  const st = $('w-stage');
  return Math.min(st.clientWidth / size.w, st.clientHeight / size.h) * 0.98 || 1;
}
function apply() {
  const p = $('w-paper');
  p.style.width = `${size.w * view.s}px`;
  p.style.height = `${size.h * view.s}px`;
  p.style.transform = `translate(${view.x}px, ${view.y}px)`;
}
function fit() {
  const st = $('w-stage');
  view.s = fitScale();
  view.x = (st.clientWidth - size.w * view.s) / 2;
  view.y = (st.clientHeight - size.h * view.s) / 2;
  zoomed = false;
  apply();
}
function zoomAt(factor, mx, my) {
  const st = $('w-stage');
  if (mx == null) { mx = st.clientWidth / 2; my = st.clientHeight / 2; }
  const min = fitScale() * 0.5, max = fitScale() * 14;
  const ns = Math.min(max, Math.max(min, view.s * factor));
  view.x = mx - (mx - view.x) * (ns / view.s);
  view.y = my - (my - view.y) * (ns / view.s);
  view.s = ns;
  zoomed = true;
  apply();
}
// Centre on part of the drawing (drawing units), zoomed in
function zoomTo(box) {
  const st = $('w-stage');
  view.s = Math.min(fitScale() * 3, Math.max(fitScale(), Math.min(st.clientWidth / (box.width + 80), st.clientHeight / (box.height + 80))));
  const cx = (box.x + box.width / 2 - size.x) * view.s, cy = (box.y + box.height / 2 - size.y) * view.s;
  view.x = st.clientWidth / 2 - cx;
  view.y = st.clientHeight / 2 - cy;
  zoomed = true;
  apply();
}

// A connector / splice / ground the page was opened for (from a location chart): marked and zoomed to
function markItem(item) {
  fit();
  const svg = $('w-paper').querySelector('svg');
  const want = item.toUpperCase();
  const hits = [...svg.querySelectorAll('a[name]')].filter((a) => a.getAttribute('name').toUpperCase() === want);
  if (!hits.length) for (const t of svg.querySelectorAll('text')) if (clean(t.textContent).toUpperCase() === want) hits.push(t);
  if (!hits.length) { toast(`${item} isn't labeled on this page`); return; }
  let first = null;
  for (const h of hits) {
    let b;
    try { b = h.getBBox(); } catch { continue; }
    if (!b || (!b.width && !b.height)) continue;
    // the box in the drawing's units (the element may sit inside transformed groups)
    let m = null;
    try { m = svg.getScreenCTM().inverse().multiply(h.getScreenCTM()); } catch { /* use the box as it is */ }
    const pt = (x, y) => { if (!m) return { x, y }; const p = new DOMPoint(x, y).matrixTransform(m); return { x: p.x, y: p.y }; };
    const a = pt(b.x, b.y), c = pt(b.x + b.width, b.y + b.height);
    const box = { x: Math.min(a.x, c.x), y: Math.min(a.y, c.y), width: Math.abs(c.x - a.x), height: Math.abs(c.y - a.y) };
    const r = document.createElementNS('http://www.w3.org/2000/svg', 'rect');
    r.setAttribute('x', box.x - 4); r.setAttribute('y', box.y - 4);
    r.setAttribute('width', box.width + 8); r.setAttribute('height', box.height + 8);
    r.setAttribute('rx', 3); r.setAttribute('class', 'w-mark');
    svg.append(r); // on top, see-through (Ford draws wires as white lines over black ones)
    first ||= box;
  }
  if (first) zoomTo(first);
}

let drag = null;
function linkTarget(a) {
  const href = a?.getAttribute('data-href');
  if (!href || !/\/wiring\/page/i.test(href)) return null;
  try { return new URL(href.replace(/&amp;/g, '&'), 'https://www.fordtechservice.dealerconnection.com/').searchParams; } catch { return null; }
}
function stageEvents() {
  const st = $('w-stage');
  st.addEventListener('pointerover', (e) => {
    if (drag) return;
    const sp = linkTarget(e.target.closest?.('a.w-link'));
    if (sp) warm(sp.get('cell'), sp.get('page'));
  });
  st.addEventListener('wheel', (e) => {
    e.preventDefault();
    const r = st.getBoundingClientRect();
    zoomAt(Math.exp(-e.deltaY * (e.ctrlKey ? 0.01 : 0.0015)), e.clientX - r.left, e.clientY - r.top);
  }, { passive: false });
  st.addEventListener('pointerdown', (e) => {
    if (e.button !== 0) return;
    drag = { x: e.clientX, y: e.clientY, vx: view.x, vy: view.y, moved: false, id: e.pointerId };
  });
  st.addEventListener('pointermove', (e) => {
    if (!drag || drag.id !== e.pointerId) return;
    const dx = e.clientX - drag.x, dy = e.clientY - drag.y;
    if (!drag.moved && Math.hypot(dx, dy) < 4) return;
    if (!drag.moved) { drag.moved = true; st.setPointerCapture(e.pointerId); st.classList.add('dragging'); hidePop(); }
    view.x = drag.vx + dx; view.y = drag.vy + dy; zoomed = true;
    apply();
  });
  const end = (e) => {
    if (!drag || drag.id !== e.pointerId) return;
    const moved = drag.moved;
    drag = null;
    st.classList.remove('dragging');
    if (!moved && e.type === 'pointerup') clickAt(e);
  };
  st.addEventListener('pointerup', end);
  st.addEventListener('pointercancel', end);
  st.addEventListener('dblclick', (e) => {
    if (e.target.closest?.('a.w-link')) return;
    const r = st.getBoundingClientRect();
    zoomAt(2, e.clientX - r.left, e.clientY - r.top);
  });
  new ResizeObserver(() => { if (!$('w-page-view').hidden && !zoomed) fit(); }).observe(st);
}

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

// ---------- buttons ----------
function fordPage() {
  if (state.at) api.wiringFord({ kind: 'page', cell: state.at.cell, page: state.at.page, item: state.at.item, title: titleOf(state.at.cell) || state.at.title });
  else api.wiringFord({ kind: 'TableOfContents', title: 'Wiring' });
  toast("Opening Ford's page as a tab…");
}
function goBack() {
  const prev = backList.pop();
  if (prev) openPage(prev, { push: false }); else showList();
}

$('btn-contents').addEventListener('click', () => { backList = []; showList(); });
$('btn-back').addEventListener('click', goBack);
$('btn-ford').addEventListener('click', fordPage);
$('btn-print').addEventListener('click', () => window.print());
$('btn-in').addEventListener('click', () => zoomAt(1.4));
$('btn-out').addEventListener('click', () => zoomAt(1 / 1.4));
$('btn-fit').addEventListener('click', fit);
$('w-form').addEventListener('submit', (e) => e.preventDefault());
$('w-q').addEventListener('input', () => { state.q = $('w-q').value.trim(); save(); drawList(); });
$('w-sort').addEventListener('change', () => { state.sort = $('w-sort').value; save(); drawList(); });
$('w-only').addEventListener('change', () => { state.only = $('w-only').checked; save(); drawList(); });
document.addEventListener('keydown', (e) => {
  if ($('w-page-view').hidden || e.target.closest?.('input, select, textarea')) return;
  if (e.key === '+' || e.key === '=') zoomAt(1.4);
  else if (e.key === '-' || e.key === '_') zoomAt(1 / 1.4);
  else if (e.key === '0') fit();
  else if (e.key === 'Escape') hidePop();
  else if (e.key === 'Backspace' || (e.altKey && e.key === 'ArrowLeft')) goBack();
  else if (e.key === 'PageDown') $('btn-next').click();
  else if (e.key === 'PageUp') $('btn-prev').click();
  else return;
  e.preventDefault();
});
document.addEventListener('pointerdown', (e) => { if (!$('w-pop').hidden && !e.target.closest('#w-pop, #w-stage')) hidePop(); });

// Diagnostics sends wiring results here
api.onOpen?.((o) => { if (o?.wiring) { backList = []; openPage(o.wiring); } });

(async () => {
  stageEvents();
  const info = (await api.init().catch(() => null)) || {};
  const s = info.state || {};
  state = { q: String(s.q || ''), sort: s.sort === 'title' ? 'title' : 'cell', only: !!s.only, at: s.at && s.at.cell ? s.at : null };
  $('w-q').value = state.q; $('w-sort').value = state.sort; $('w-only').checked = state.only;
  const go = info.open?.wiring || state.at;
  if (go) {
    openPage(go, { push: false });
    loadToc().then((ok) => { if (ok && state.at) $('w-title').textContent = `${state.at.cell}-${state.at.page}  ${titleOf(state.at.cell) || state.at.title}`; });
  } else {
    await loadToc();
  }
})();
