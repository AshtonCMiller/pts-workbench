// The app's own workshop manual: Ford's contents and procedures, redrawn in the app's style.
// Open procedures, the open folders and where you were in each procedure are saved with the RO,
// so closing and reopening the app (or the RO) brings everything back.
'use strict';
const api = window.manual;
const $ = (id) => document.getElementById(id);

let state = { docs: [], active: null, open: [], query: '', tocWidth: 340, tocHidden: false };
let tree = null;          // [{ type: 'folder'|'doc'|'link', title, children, link, url, path }]
let leaves = [];          // every doc/link, for search
const docCache = new Map(); // doc key -> rendered element
const backStacks = new Map(); // doc key -> [scrollTop, ...] (this session only)

// ---------- helpers ----------
function el(tag, props = {}, ...kids) {
  const n = document.createElement(tag);
  for (const [k, v] of Object.entries(props)) {
    if (v == null || v === false) continue;
    if (k === 'class') n.className = v;
    else if (k === 'text') n.textContent = v;
    else if (k === 'style') { for (const part of String(v).split(';')) { const i = part.indexOf(':'); if (i > 0) n.style.setProperty(part.slice(0, i).trim(), part.slice(i + 1).trim()); } }
    else if (k.startsWith('on')) n.addEventListener(k.slice(2), v);
    else n.setAttribute(k, v);
  }
  for (const c of kids) if (c != null && c !== false) n.append(c);
  return n;
}
const docKey = (link) => `${link.linkType || 'Procedure'}|${link.searchNumber}|${link.procUid || ''}`;
let saveTimer = null;
function save() {
  clearTimeout(saveTimer);
  saveTimer = setTimeout(() => api.save(JSON.parse(JSON.stringify(state))), 400);
}
const activeDoc = () => state.docs.find((d) => d.key === state.active) || null;

// Ford's inline styles aren't used (and this page's security rules would flag every one), so they're
// dropped before parsing; only "hidden" is kept, as a marker.
const stripStyles = (html) => String(html).replace(/\sstyle\s*=\s*(?:"([^"]*)"|'([^']*)')/gi,
  (_m, a, b) => (/display\s*:\s*none/i.test(a || b || '') ? ' data-hidden="1"' : ''));

// The manual writes a link target either as "G123#part" or "G123:procsub2"
function splitTarget(forAttr) {
  const s = String(forAttr || '');
  const m = s.match(/^(.*?):((?:procsub|figure_|table_).*)$/);
  if (m) return { searchNumber: m[1], hash: m[2] };
  const [searchNumber, hash = ''] = s.split('#');
  return { searchNumber, hash };
}

// ---------- start ----------
async function start() {
  const init = await api.init();
  if (init?.state) state = { ...state, ...init.state };
  applyLayout();
  renderTabs();
  if (init?.open) openDoc(init.open, { newTab: true }); // sent here from the Diagnostics tab
  else showDoc();
  loadContents();
}
api.onOpen?.((link) => openDoc(link, { newTab: true }));

async function loadContents() {
  try { await loadContentsInner(); } catch (e) {
    api.log(`contents: ${e && e.stack || e}`);
    contentsError({ message: `Something went wrong showing the contents (${e?.message || e})` });
  }
}
async function loadContentsInner() {
  $('tree').replaceChildren(el('div', { class: 'note' }, el('div', { class: 'spinner' }), 'Finding the vehicle…'));
  const book = await api.book();
  if (book?.error) return contentsError(book);
  $('book-title').replaceChildren(book.title || 'Workshop Manual', ' ', el('small', { text: 'Workshop Manual' }));
  $('tree').replaceChildren(el('div', { class: 'note' }, el('div', { class: 'spinner' }), 'Loading the contents…'));
  const res = await api.tree();
  if (res?.error) return contentsError(res);
  tree = parseTree(res.html || '');
  leaves = [];
  const walk = (nodes, pathTitles) => {
    for (const n of nodes) {
      n.path = [...pathTitles, n.title];
      if (n.type === 'folder') walk(n.children, n.path); else leaves.push(n);
    }
  };
  walk(tree, []);
  const count = (n) => (n.type === 'folder' ? (n.count = n.children.reduce((a, c) => a + count(c), 0)) : 1);
  tree.forEach(count);
  api.log(`contents: ${(res.html || '').length} characters, ${tree.length} top sections, ${leaves.length} entries`);
  if (!leaves.length) {
    tree = null;
    const text = String(res.html || '').replace(/<[^>]+>/g, ' ').replace(/\s+/g, ' ').trim();
    const raw = String(res.html || '');
    const i = raw.search(/<li|<a\s/i);
    api.log(`contents were empty; ${(raw.match(/<li/gi) || []).length} <li>, ${(raw.match(/<a\s/gi) || []).length} <a>, data-for ${(raw.match(/data-for/gi) || []).length}; sample: ${raw.slice(Math.max(0, i), i + 1500)}`);
    return contentsError({ message: 'Ford sent the contents back empty.', detail: text.slice(0, 300) || '(nothing at all)' });
  }
  $('book-title').append(el('small', { text: ` · ${leaves.length.toLocaleString()} entries` }));
  $('search').value = state.query || '';
  renderTree();
  if (!state.docs.length) showWelcome();
}

function contentsError(res) {
  $('tree').replaceChildren(el('div', { class: 'note' }, res.message || "Couldn't load the manual."));
  showProblem(res, loadContents);
}

// ---------- contents (from Ford's tree HTML) ----------
function parseTree(html) {
  const doc = new DOMParser().parseFromString(stripStyles(html), 'text/html');
  const text = (n) => (n?.textContent || '').replace(/\s+/g, ' ').trim();
  // Every procedure/document link, wherever it sits; the folders are the list items around it
  const anchors = [...doc.querySelectorAll('a[data-for]')].filter((a) => {
    const f = a.getAttribute('data-for') || '';
    return !a.hasAttribute('data-hidden') && !/adttestindex|spaindex_test/i.test(f) && text(a);
  });
  const isLeafLi = (li) => [...li.children].some((c) => c.tagName === 'A' && c.hasAttribute('data-for'));
  // A folder's name is the text before its first list or link (Ford sometimes wraps the name and
  // the list in one element, so taking a whole element's text would include every procedure in it)
  const folderTitle = (li) => {
    let out = '';
    let stop = false;
    const walk = (node) => {
      for (const c of node.childNodes) {
        if (stop) return;
        if (c.nodeType === 1 && /^(UL|OL|A)$/.test(c.tagName)) { if (out.trim()) { stop = true; return; } continue; }
        if (c.nodeType === 3) out += c.textContent;
        else if (c.nodeType === 1) walk(c);
      }
    };
    walk(li);
    return out.replace(/\s+/g, ' ').trim();
  };
  const roots = [];
  const folders = new Map(); // <li> -> folder node
  const seen = new Set();
  for (const a of anchors) {
    const key = `${a.getAttribute('data-for')}|${a.getAttribute('data-procuid') || ''}`;
    if (!a.closest('li') && seen.has(key)) continue; // e.g. the same link again on the manual's cover page
    seen.add(key);
    const chain = [];
    for (let li = a.closest('li')?.parentElement?.closest('li'); li; li = li.parentElement?.closest('li')) {
      if (!isLeafLi(li)) chain.unshift(li); // a procedure with sub-procedures: keep them at the same level
    }
    let list = roots;
    for (const li of chain) {
      let f = folders.get(li);
      if (!f) { f = { type: 'folder', title: folderTitle(li) || 'Section', children: [] }; folders.set(li, f); list.push(f); }
      list = f.children;
    }
    const title = text(a);
    const cls = a.getAttribute('class') || '';
    const forAttr = a.getAttribute('data-for') || '';
    if (/Quicklinkpdf-link/i.test(cls) || /^(https?:)?\//i.test(forAttr)) {
      list.push({ type: 'link', title, url: forAttr });
    } else {
      const { searchNumber, hash } = splitTarget(forAttr);
      const linkType = /publication-content/i.test(cls) ? 'PublicationContent' : 'Procedure';
      list.push({ type: 'doc', title, link: { searchNumber, procUid: a.getAttribute('data-procuid') || '', linkType, hash } });
    }
  }
  return roots;
}

function renderTree() {
  const box = $('tree');
  const q = (state.query || '').trim().toLowerCase();
  if (q) return renderSearch(box, q);
  const open = new Set(state.open);
  const frag = document.createDocumentFragment();
  const add = (nodes, depth) => {
    for (const n of nodes) {
      const id = n.path.join(' / ');
      if (n.type === 'folder') {
        const isOpen = open.has(id);
        // "303-05B Accessory Drive…" → the section number as a tag, then the name
        const m = n.title.match(/^(\d{3}-\d{2}[A-Z]?\d?)\s+(.*)$/);
        const label = m ? el('span', { class: 'label' }, el('span', { class: 'code', text: m[1] }), m[2]) : el('span', { class: 'label', text: n.title });
        frag.append(el('div', {
          class: `node folder d${Math.min(depth, 4)}${isOpen ? ' open' : ''} ${kindOf(n.title)}`, style: `--depth:${depth}`, title: n.title,
          onclick: () => { toggleFolder(id); },
        }, el('span', { class: 'tw', text: isOpen ? '▾' : '▸' }), label,
        el('span', { class: 'count', text: String(n.count || '') })));
        if (isOpen) add(n.children, depth + 1);
      } else {
        frag.append(leafNode(n, depth));
      }
    }
  };
  add(tree || [], 0);
  box.replaceChildren(frag);
}

// Colour-code the usual sub-sections so they're easy to pick out
function kindOf(title) {
  const t = title.toLowerCase();
  if (t.startsWith('description and operation')) return 'k-do';
  if (t.startsWith('diagnosis and testing')) return 'k-dt';
  if (t.startsWith('removal and installation')) return 'k-ri';
  if (t.startsWith('general procedures')) return 'k-gp';
  if (t.startsWith('disassembly') || t.startsWith('assembly') || t.startsWith('installation')) return 'k-da';
  if (t.startsWith('specifications')) return 'k-sp';
  return '';
}

function leafNode(n, depth, extra) {
  const on = n.type === 'doc' && activeDoc()?.key === docKey(n.link);
  return el('div', {
    class: `node ${n.type} d${Math.min(depth, 4)}${on ? ' on' : ''}`, style: `--depth:${depth}`, title: n.path.join(' › '),
    onclick: (e) => openLeaf(n, e),
    onauxclick: (e) => { if (e.button === 1) openLeaf(n, { ...e, newTab: true }); },
  }, el('span', { class: 'tw' }), el('span', { class: 'label' }, extra || n.title));
}

function toggleFolder(id) {
  const open = new Set(state.open);
  if (open.has(id)) open.delete(id); else open.add(id);
  state.open = [...open];
  save();
  const scroll = $('tree').scrollTop;
  renderTree();
  $('tree').scrollTop = scroll;
}

// A DTC typed in the contents search: offer Ford's diagnostics search (pinpoint tests, TSBs…) for it
const DTC_LIKE = /(?:^|\s)([PBUC][0-9A-F]{4})(?:[:|][0-9A-F]{2})?(?=\s|$)/i;
function renderSearch(box, q) {
  const words = q.split(/\s+/).filter(Boolean);
  const dtc = q.match(DTC_LIKE);
  const diag = dtc ? el('button', { class: 'diag-offer', title: "Search Ford's diagnostics (pinpoint tests, TSBs, SSMs) for this code",
    onclick: () => api.openDiag(q.trim()) }, el('b', { text: `🔎 ${dtc[1].toUpperCase()}` }), ' Find pinpoint tests & TSBs in Diagnostics ›') : null;
  const hits = [];
  for (const n of leaves) {
    const hay = n.path.join(' ').toLowerCase();
    if (words.every((w) => hay.includes(w))) hits.push(n);
    if (hits.length >= 300) break;
  }
  if (!hits.length) return box.replaceChildren(...[diag, el('div', { class: 'note', text: 'Nothing in the contents matches that.' })].filter(Boolean));
  const mark = (s) => {
    const frag = document.createDocumentFragment();
    const re = new RegExp(`(${words.map((w) => w.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')).join('|')})`, 'ig');
    let last = 0;
    for (const m of s.matchAll(re)) { frag.append(s.slice(last, m.index), el('mark', { text: m[0] })); last = m.index + m[0].length; }
    frag.append(s.slice(last));
    return frag;
  };
  box.replaceChildren(...(diag ? [diag] : []), ...hits.map((n) => leafNode(n, 0, el('span', {}, mark(n.title), el('span', { class: 'result-path', text: n.path.slice(0, -1).slice(-2).join(' › ') })))));
}

function openLeaf(n, e) {
  if (n.type === 'link') { api.openUrl(n.url); return; }
  openDoc({ ...n.link, title: n.title, section: n.path.slice(-3, -1).join(' › ') }, { newTab: e?.ctrlKey || e?.newTab });
}

// ---------- documents ----------
function openDoc(link, { newTab = false, hash = link.hash } = {}) {
  const key = docKey(link);
  let d = state.docs.find((x) => x.key === key);
  if (!d) {
    d = { key, title: link.title || 'Procedure', section: link.section || '', searchNumber: link.searchNumber,
          procUid: link.procUid || '', linkType: link.linkType || 'Procedure', scroll: 0, hash: '' };
    const cur = state.docs.findIndex((x) => x.key === state.active);
    // A plain click replaces the open procedure (like a browser); Ctrl+click or links inside a procedure add a tab
    if (!newTab && cur >= 0 && !state.docs[cur].pinned) state.docs.splice(cur, 1, d);
    else state.docs.splice(cur + 1, 0, d);
  }
  if (hash) { d.hash = hash; d.scroll = 0; }
  if (link.ppt) { d.goPpt = String(link.ppt); d.scroll = 0; }
  rememberScroll();
  state.active = key;
  save();
  renderTabs();
  showDoc();
  if (!state.query) renderTree();
}

function closeDoc(key) {
  const i = state.docs.findIndex((d) => d.key === key);
  if (i < 0) return;
  state.docs.splice(i, 1);
  docCache.delete(key);
  backStacks.delete(key);
  if (state.active === key) state.active = (state.docs[i] || state.docs[i - 1])?.key || null;
  save();
  renderTabs();
  showDoc();
  if (!state.query) renderTree();
}

function renderTabs() {
  $('tabs').replaceChildren(...state.docs.map((d) => el('div', {
    class: `dtab${d.key === state.active ? ' on' : ''}`, title: [d.section, d.title].filter(Boolean).join(' › '),
    onclick: () => { if (state.active !== d.key) { rememberScroll(); state.active = d.key; save(); renderTabs(); showDoc(); if (!state.query) renderTree(); } },
    onauxclick: (e) => { if (e.button === 1) closeDoc(d.key); },
  }, el('span', { class: 't', text: d.title }), el('button', { class: 'x', title: 'Close', text: '×', onclick: (e) => { e.stopPropagation(); closeDoc(d.key); } }))));
  const on = $('tabs').querySelector('.dtab.on');
  if (on) on.scrollIntoView({ block: 'nearest', inline: 'nearest' });
}

function rememberScroll() {
  const d = activeDoc();
  if (d && docCache.has(d.key) && $('content').firstChild === docCache.get(d.key)) d.scroll = $('content').scrollTop;
}

let showToken = 0;
async function showDoc() {
  const d = activeDoc();
  const token = ++showToken;
  $('doc-bar').hidden = !d;
  if (!d) { if (tree) showWelcome(); else $('content').replaceChildren(); return; }
  $('doc-where').textContent = [d.section, d.title].filter(Boolean).join(' › ');
  updateBack();
  let node = docCache.get(d.key);
  if (!node) {
    $('content').replaceChildren(el('div', { class: 'state' }, el('div', { class: 'spinner' }), `Loading ${d.title}…`));
    const res = await api.proc({ searchNumber: d.searchNumber, procUid: d.procUid, linkType: d.linkType });
    if (token !== showToken) return;
    if (res?.error) return showProblem(res, showDoc);
    try {
      node = renderProcedure(res.html, res.base, d);
    } catch (e) {
      api.log(`procedure ${d.searchNumber}/${d.procUid}: ${e && e.stack || e}`);
      return showProblem({ message: `Couldn't show this procedure (${e?.message || e})` }, showDoc);
    }
    if (!(node.textContent || '').replace(d.title, '').trim()) {
      api.log(`procedure ${d.searchNumber}/${d.procUid} came back empty: ${String(res.html).slice(0, 300)}`);
    }
    docCache.set(d.key, node);
    if (d.title === 'Procedure' && node.dataset.title) { d.title = node.dataset.title; renderTabs(); save(); }
  }
  $('content').replaceChildren(node);
  if (node._ppts) for (const p of node._ppts.values()) renderPpt(node, d, p); // the view may have changed in another procedure
  updateModeButton(node);
  if (d.goPpt) {
    const id = d.goPpt; d.goPpt = ''; save();
    requestAnimationFrame(() => goToPpt(node, d, id));
  } else if (d.hash) {
    const h = d.hash; d.hash = ''; save();
    requestAnimationFrame(() => jumpTo(node, h, false));
  } else {
    $('content').scrollTop = d.scroll || 0;
  }
}

function showWelcome() {
  $('doc-bar').hidden = true;
  $('content').replaceChildren(el('div', { class: 'state' },
    el('h2', { text: 'Pick a procedure' }),
    el('p', { text: 'Open a section on the left, or search the contents. Ctrl+click opens a procedure in a new tab; links inside a procedure always do.' })));
}

function showProblem(res, retry) {
  $('doc-bar').hidden = true;
  const actions = el('div', { class: 'actions' });
  if (res.needsLogin) {
    actions.append(el('button', { class: 'primary', text: 'Sign in to PTS', onclick: async (e) => {
      e.target.disabled = true; e.target.textContent = 'Waiting for sign-in…';
      await api.signIn();
      retry();
    } }));
  } else {
    actions.append(el('button', { class: 'primary', text: 'Try again', onclick: () => retry() }));
  }
  actions.append(el('button', { class: 'ghost', text: 'Open in PTS instead', onclick: () => api.openPts() }));
  if (!res.needsLogin) actions.append(el('button', { class: 'ghost', text: 'Open log', title: 'What the app asked Ford for, and what came back', onclick: () => api.openLog() }));
  $('content').replaceChildren(el('div', { class: 'state' },
    el('h2', { text: res.needsLogin ? 'Sign in to PTS' : "Couldn't load this" }),
    el('p', { text: res.needsLogin ? 'The manual needs your PTS login. Sign in once and it opens straight away from then on.' : (res.message || 'Something went wrong talking to Ford.') }),
    res.detail ? el('p', { class: 'detail', text: res.detail }) : null,
    actions));
}

// ---------- turning Ford's procedure HTML into ours ----------
const UI_IMAGES = /(Caret(Right|Down)|bars-black|minus-black|print|arrowback|ppt-(minus|plus)|FordEcat|loading|spigot)/i;
const asset = (src, base) => {
  try { return `ptsasset://f/?u=${encodeURIComponent(new URL(src, base).href)}`; } catch { return ''; }
};

// Ford's server sends some pinpoint tests as one table with "PINPOINT TEST A: …" as its caption and each
// step as a title row ("A1 CHECK …") followed by its content. PTS rearranges that in the page; this does the
// same, into the layout the single-step view works with (a heading, then one row per step).
function normalizeCaptionPinpoints(root) {
  const doc = root.ownerDocument;
  const make = (tag, attrs = {}) => { const n = doc.createElement(tag); for (const [k, v] of Object.entries(attrs)) n.setAttribute(k, v); return n; };
  root.querySelectorAll('table').forEach((table) => {
    const caption = table.querySelector(':scope > caption');
    const title = (caption?.textContent || '').replace(/\s+/g, ' ').trim();
    const m = title.match(/^PINPOINT TEST\s+([A-Z]{1,3}\d{0,3})\s*:/i);
    if (!m) return;
    const name = m[1].toUpperCase();
    const rows = [...table.querySelectorAll(':scope > tbody > tr, :scope > tr')];
    const isHead = (tr) => {
      const td = tr.children.length === 1 && tr.children[0].matches('td.gray') ? tr.children[0] : null;
      const b = td?.querySelector(':scope > b');
      return td && td.querySelector(':scope > a[name]') && b && new RegExp(`^${name}\\d+\\b`, 'i').test((b.textContent || '').trim());
    };
    if (!rows.some(isHead)) return;
    let anchor = table.previousElementSibling;
    if (!(anchor && anchor.tagName === 'A' && anchor.getAttribute('name'))) anchor = null;
    const h = make('h3', { class: 'ppt', 'data-ppt': name, 'data-legacy': '1' }); // Ford shows these all at once and doesn't record the steps
    h.textContent = title;
    if (anchor) h.id = `n-${anchor.getAttribute('name')}`;
    const body = make('div', { id: `PPT${name}` });
    const tbl = make('table');
    const tbody = make('tbody');
    tbl.append(tbody); body.append(tbl);
    let intro = null;
    let step = null;
    let n = 0;
    for (const tr of rows) {
      if (isHead(tr)) {
        n += 1;
        const id = tr.querySelector('a[name]').getAttribute('name');
        step = make('tr', { 'data-content-type': 'step', 'data-pptid': name, 'data-pptstepid': id, 'data-ppttypeandstep': `${name}STEP${n}` });
        const td = make('td');
        const inner = make('table');
        const innerBody = make('tbody');
        inner.append(innerBody); td.append(inner); step.append(td);
        tbody.append(step);
        step._rows = innerBody;
        innerBody.append(tr);
      } else if (step) {
        step._rows.append(tr);
      } else {
        if (!intro) { intro = make('tr', { 'data-content-type': 'wrapper' }); intro.append(make('td')); tbody.append(intro); }
        intro.firstChild.append(...tr.childNodes);
      }
    }
    table.replaceWith(h, body);
  });
}

// Ford's HTML → ours: used for a whole procedure, and for pinpoint test steps fetched later
function cleanContent(root, base, d) {
  normalizeCaptionPinpoints(root);
  // Ford's training banners next to pictures ("Workshop Manual Graphics Training", "Click here to learn
  // about symbols…") don't load outside PTS and get in the way: remove them, and the empty space they leave
  const BANNER = /graphics training|learn about symbols|symbols, color coding/i;
  const touched = new Set();
  root.querySelectorAll('a, img').forEach((n) => {
    if (!n.isConnected) return;
    const t = `${n.textContent || ''} ${n.getAttribute('alt') || ''} ${n.getAttribute('title') || ''} ${n.getAttribute('href') || ''} ${n.getAttribute('src') || ''}`;
    if (!BANNER.test(t)) return;
    const box = n.closest('a') || n;
    const table = box.closest('table');
    if (table) touched.add(table);
    box.remove();
  });
  const empty = (n) => !(n.textContent || '').trim() && !n.querySelector('img, svg, object, embed, iframe');
  for (const table of touched) {
    if (!table.isConnected) continue;
    table.querySelectorAll('tr').forEach((tr) => { if (empty(tr)) tr.remove(); });
    // a column left with nothing in it (beside the picture): drop it so the picture has the room
    table.querySelectorAll('td, th').forEach((cell) => {
      if (empty(cell) && cell.parentElement && cell.parentElement.children.length > 1) cell.remove();
    });
    if (empty(table)) table.remove();
  }
  root.querySelectorAll('img').forEach((img) => {
    const src = img.getAttribute('src') || '';
    if (!src || UI_IMAGES.test(src)) { img.remove(); return; }
    img.setAttribute('src', asset(src, base));
    img.className = 'fig';
    img.removeAttribute('width'); img.removeAttribute('height');
  });

  // Pinpoint test titles become headings
  root.querySelectorAll('span.isipppt, span.spappt, span[id^="Span"][onclick*="Pinpoint"]').forEach((s) => {
    const title = (s.querySelector('#spappttitle, #ppttitle')?.textContent || s.textContent || '').trim();
    const anchor = s.previousElementSibling?.tagName === 'A' && s.previousElementSibling.getAttribute('name');
    const h = el('h3', { class: 'ppt', text: title });
    if (anchor) h.id = `n-${anchor}`;
    const name = (s.getAttribute('id') || '').replace(/^Span/, '');
    if (name) h.setAttribute('data-ppt', name);
    const spa = s.getAttribute('data-spapptfile'); // newer tests: steps are fetched when the test is opened
    if (spa) h.setAttribute('data-spa', spa);
    s.replaceWith(h);
  });
  // "Click for details" just reveals the text next to it
  root.querySelectorAll('a[data-pptlinktype="clickfordetails"]').forEach((a) => {
    a.closest('tr')?.classList.add('ppt-outcome'); // an answer that ends with an action rather than another step
    const hidden = a.nextElementSibling;
    a.remove();
    if (hidden) hidden.removeAttribute('style');
  });
  root.querySelectorAll('[data-content-type="intro"], #lnkPptStepIntro img').forEach((n) => n.removeAttribute('style'));

  // Attributes: no scripts, no Ford styling, but keep what tables and anchors need
  const KEEP = new Set(['href', 'name', 'id', 'colspan', 'rowspan', 'src', 'alt', 'title', 'class', 'type', 'start',
    'data-for', 'data-procuid', 'data-show-ppt', 'data-content-type', 'data-pptid', 'data-pptstepid', 'data-ppttypeandstep', 'data-ppt', 'data-spa', 'data-legacy', 'data-hash-type', 'data-linkid', 'valign']);
  root.querySelectorAll('*').forEach((n) => {
    for (const a of [...n.attributes]) if (!KEEP.has(a.name)) n.removeAttribute(a.name);
    if (n.tagName === 'A' && n.getAttribute('name')) n.id = `n-${n.getAttribute('name')}`;
  });
  root.querySelectorAll('center').forEach((c) => { const div = el('div', { class: 'center' }); div.append(...c.childNodes); c.replaceWith(div); });
  root.querySelectorAll('table').forEach((t) => {
    if (t.querySelector(':scope > tbody > tr > td, :scope > thead > tr > th, :scope > tr > td')) t.classList.add('grid');
  });
  // Ford spaces things out with runs of <br>; one is enough, and none next to a block
  root.querySelectorAll('br').forEach((br) => {
    let prev = br.previousSibling;
    while (prev && prev.nodeType === 3 && !prev.textContent.trim()) prev = prev.previousSibling;
    let next = br.nextSibling;
    while (next && next.nodeType === 3 && !next.textContent.trim()) next = next.nextSibling;
    const block = (n) => n && n.nodeType === 1 && /^(BR|TABLE|DIV|P|UL|OL|H1|H2|H3|H4)$/.test(n.tagName);
    if (!prev || block(prev) || block(next) || !next) br.remove();
  });
  root.querySelectorAll('b[data-content-type="stepresult"]').forEach((b) => b.classList.add('stepresult'));

  // Safety notes stand out
  root.querySelectorAll('p, li').forEach((p) => {
    const t = (p.textContent || '').trim();
    const m = t.match(/^(WARNING|CAUTION|NOTICE|NOTE)\b/);
    if (m && p.tagName === 'P') p.classList.add('callout', m[1].toLowerCase());
  });

  // Links: other procedures open as tabs here, steps jump within the page, the rest open in a window
  root.querySelectorAll('a').forEach((a) => {
    const href = a.getAttribute('href') || '';
    const forAttr = a.getAttribute('data-for');
    const showPpt = a.getAttribute('data-show-ppt');
    if (forAttr && showPpt) {
      // Ford's DTC charts: "GO to Pinpoint Test X" in this or another procedure
      const { searchNumber } = splitTarget(forAttr);
      const procUid = a.getAttribute('data-procuid') || '';
      if (searchNumber === d.searchNumber || (procUid && procUid === d.procUid)) a.dataset.ppt = showPpt;
      else a.dataset.open = JSON.stringify({ searchNumber, procUid, linkType: 'Procedure', ppt: showPpt, title: (a.textContent || '').trim() });
      a.removeAttribute('href');
    } else if (forAttr && /expand-link|procedure-link|publication-content/.test(a.className)) {
      const { searchNumber, hash } = splitTarget(forAttr);
      a.dataset.open = JSON.stringify({ searchNumber, procUid: a.getAttribute('data-procuid') || '',
        linkType: /publication-content/.test(a.className) ? 'PublicationContent' : 'Procedure', hash, title: (a.textContent || '').trim() });
      a.removeAttribute('href');
    } else if (/\/publication\/proc\?/i.test(href)) {
      let u = null;
      try { u = new URL(href, base); } catch {}
      const searchNumber = u?.searchParams.get('searchNumber') || '';
      const procUid = u?.searchParams.get('procUid') || '';
      const hash = (u?.hash || '').replace(/^#/, '');
      if (procUid && procUid === d.procUid) {
        if (hash) { a.dataset.jump = hash; a.classList.add('jump'); }
      } else if (searchNumber) {
        a.dataset.open = JSON.stringify({ searchNumber, procUid, linkType: u.searchParams.get('linkType') || 'Procedure', hash, title: (a.textContent || '').trim() });
      }
      a.removeAttribute('href');
    } else if (/^javascript:/i.test(href) || !href) {
      if (!a.getAttribute('name')) { a.replaceWith(...a.childNodes); }
      else a.removeAttribute('href');
    } else if (href.startsWith('#')) {
      a.dataset.jump = href.slice(1);
      a.removeAttribute('href');
    } else {
      try { a.dataset.url = new URL(href, base).href; } catch {}
      a.removeAttribute('href');
    }
  });

}

function renderProcedure(html, base, d) {
  const parsed = new DOMParser().parseFromString(`<div id="wrap">${stripStyles(html)}</div>`, 'text/html');
  const root = parsed.getElementById('contentDiv') || parsed.getElementById('wrap');
  const meta = (name) => root.querySelector(`meta[name="${name}"]`)?.getAttribute('content') || '';
  const info = {
    sectionNumber: meta('tps_sectionnumber'), sectionTitle: meta('tps_section_title'),
    subsection: meta('tps_ss_title'), title: meta('tps_proctitle'),
  };
  const rev = (root.querySelector('a.bookinfosmall')?.textContent || '').replace(/\s+/g, ' ').trim();

  // Ford's own page furniture
  root.querySelectorAll('script, style, link, meta, #taginfo, #rapDiv, .show-vin, #goBackStepRow, #copyright').forEach((n) => n.remove());
  const headTable = root.querySelector('a.bookinfosmall')?.closest('table');
  if (headTable) {
    let next = headTable.nextElementSibling;
    headTable.remove();
    if (next?.tagName === 'HR') next.remove();
  }
  const h1 = root.querySelector('h1');
  if (h1 && (h1.textContent || '').trim() === info.title) h1.remove();
  cleanContent(root, base, d);

  const head = el('div', { class: 'doc-head' },
    el('div', { class: 'sec', text: [[info.sectionNumber, info.sectionTitle].filter(Boolean).join(' '), info.subsection].filter(Boolean).join(' · ') }),
    el('h1', { text: info.title || d.title }),
    rev ? el('div', { class: 'rev', text: rev }) : null);
  const docEl = el('article', { class: 'doc' }, head);
  docEl.append(...root.childNodes);
  docEl.dataset.title = info.title || '';

  setupPinpoints(docEl, d);

  docEl.addEventListener('click', (e) => {
    const img = e.target.closest('img.fig');
    if (img) { $('zoom').querySelector('img').src = img.src; $('zoom').hidden = false; return; }
    if (pinpointClick(docEl, d, e)) return;
    const a = e.target.closest('a');
    if (!a) return;
    if (a.dataset.open) {
      e.preventDefault();
      const link = JSON.parse(a.dataset.open);
      openDoc({ ...link, section: '' }, { newTab: true, hash: link.hash });
    } else if (a.dataset.ppt) {
      e.preventDefault();
      pushBack();
      goToPpt(docEl, d, a.dataset.ppt);
    } else if (a.dataset.jump) {
      e.preventDefault();
      pushBack();
      jumpTo(docEl, a.dataset.jump, true);
    } else if (a.dataset.url) {
      e.preventDefault();
      api.openUrl(a.dataset.url);
    }
  });
  return docEl;
}

function jumpTo(docEl, hash, flash) {
  const target = docEl.querySelector(`#n-${CSS.escape(hash)}`) || docEl.querySelector(`[name="${CSS.escape(hash)}"]`);
  if (!target) return;
  if (pinpointJump(docEl, target)) return;
  const step = target.closest('tr[data-content-type="step"]');
  const box = step ? (step.querySelector('td.gray') || step) : (target.closest('h3, p, li, td') || target);
  (step || box).scrollIntoView({ block: 'start' });
  if (flash) { box.classList.remove('flash'); void box.offsetWidth; box.classList.add('flash'); }
}

// Back (after jumping to another pinpoint step)
function pushBack() {
  const d = activeDoc();
  if (!d) return;
  const s = backStacks.get(d.key) || [];
  s.push($('content').scrollTop);
  backStacks.set(d.key, s.slice(-50));
  updateBack();
}
function updateBack() {
  const d = activeDoc();
  $('btn-back').disabled = !(d && backStacks.get(d.key)?.length);
}
$('btn-back').onclick = () => {
  const d = activeDoc();
  const s = d && backStacks.get(d.key);
  if (!s?.length) return;
  $('content').scrollTop = s.pop();
  updateBack();
};
$('btn-print').onclick = () => window.print();
// ---------- pinpoint tests: single-step view, and reporting to Vehicle History ----------
// Ford's single-step view: one step at a time, answering Yes/No moves to the next step, Back goes to
// the step before. Opening a test and every answer is reported to Vehicle History the same way Ford's
// manual does it, so warranty can see the test was run and what the results were.
const pptMode = () => (state.pptMode === 'all' ? 'all' : 'single');
const pptState = (d, name) => ((d.ppt ||= {})[name] ||= { open: false, step: '', history: [], answers: {} });
const stepLabel = (row) => {
  const t = row.getAttribute('data-ppttypeandstep') || '';
  const m = t.match(/^([A-Z]+[0-9]*)STEP(\d+)/i);
  if (m) return `${m[1]}${m[2]}`;
  const head = (row.querySelector('td.gray b')?.textContent || '').trim();
  return head.split(/\s+/)[0] || '';
};

// Answer rows (Yes / No …) become buttons; one that doesn't go on to another step ends the test with an action
function markAnswers(row) {
  row.querySelectorAll('b.stepresult').forEach((b) => {
    const tr = b.closest('tr');
    if (!tr) return;
    tr.classList.add('ppt-answer');
    if (!tr.querySelector('a[data-jump]')) tr.classList.add('ppt-outcome');
  });
}

function setupPinpoints(docEl, d) {
  const ppts = new Map();
  docEl.querySelectorAll('h3.ppt[data-ppt]').forEach((h) => {
    const name = h.getAttribute('data-ppt');
    const body = docEl.querySelector(`#PPT${CSS.escape(name)}`);
    if (!body) return;
    const rows = [...body.querySelectorAll('tr[data-content-type="step"]')].filter((r) => r.getAttribute('data-pptid') === name);
    const spa = h.getAttribute('data-spa') || '';
    if (!rows.length && !spa) return;
    const p = { name, title: h.textContent.trim(), h, body, rows, spa, legacy: h.hasAttribute('data-legacy'), intro: body.querySelector('tr[data-content-type="wrapper"]') };
    p.bar = el('div', { class: 'ppt-bar' });
    body.before(p.bar);
    h.classList.add('toggle');
    // every answer row becomes one big button
    for (const row of rows) markAnswers(row);
    ppts.set(name, p);
  });
  docEl._ppts = ppts;
  docEl.classList.toggle('has-ppt', ppts.size > 0);
  for (const p of ppts.values()) {
    renderPpt(docEl, d, p);
    if (p.spa && !p.rows.length && (pptState(d, p.name).open || pptMode() === 'all')) loadSpaSteps(docEl, d, p);
  }
  // A procedure that mentions pinpoint tests the viewer didn't recognise: note its layout for working it out
  if (!ppts.size && /PINPOINT TEST/i.test(docEl.textContent || '')) {
    const counts = ['span.isipppt', 'span.spappt', 'span[id^="Span"]', 'tr[data-content-type="step"]', '[data-spa]', 'h3.ppt']
      .map((q) => `${q}=${docEl.querySelectorAll(q).length}`).join(' ');
    api.log(`pinpoint tests not recognised in ${d.searchNumber}/${d.procUid}: ${counts}`);
  }
}

function renderPpt(docEl, d, p) {
  const st = pptState(d, p.name);
  const single = pptMode() === 'single';
  if (!p.rows.length) { // a newer test whose steps haven't been fetched yet
    const open = st.open || !single;
    p.h.classList.toggle('open', open);
    p.body.classList.toggle('hide', !open);
    p.bar.classList.toggle('hide', !open);
    if (open) p.bar.replaceChildren(el('span', { class: 'where', text: p.loading ? 'Loading the test steps…' : (p.loadError || 'No steps for this test') }),
      el('span', { class: `rec ${p.rec?.state || ''}`, text: p.rec?.text || '' }));
    return;
  }
  const current = p.rows.find((r) => r.getAttribute('data-pptstepid') === st.step) || p.rows[0];
  const open = !single || st.open;
  p.h.classList.toggle('open', open);
  p.body.classList.toggle('hide', !open);
  p.bar.classList.toggle('hide', !single || !open);
  const first = current === p.rows[0];
  if (p.intro) p.intro.classList.toggle('hide', single && !(first && !st.history.length));
  for (const r of p.rows) {
    r.classList.toggle('hide', single && r !== current);
    const answer = st.answers[r.getAttribute('data-pptstepid')];
    r.querySelectorAll('tr.ppt-answer').forEach((a) => {
      const b = a.querySelector('b.stepresult');
      a.classList.toggle('chosen', !!answer && b?.id === answer);
    });
  }
  if (!single || !open) return;
  const n = p.rows.indexOf(current) + 1;
  p.bar.replaceChildren(
    el('button', { class: 'ghost small', text: '‹ Back', disabled: st.history.length ? null : 'disabled', title: 'Go back to the step before',
      'data-ppt-action': 'back' }),
    el('span', { class: 'where', text: `Step ${stepLabel(current)} · ${n} of ${p.rows.length}` }),
    el('span', { class: `rec ${p.rec?.state || ''}`, text: p.rec?.text || (p.legacy ? "Ford only records the start of this kind of test, not each step" : 'Answers are recorded to Vehicle History') }),
    el('button', { class: 'ghost small', text: 'Start over', title: 'Back to the first step', 'data-ppt-action': 'restart' }));
}

async function report(p, d, payload, okText) {
  p.rec = { state: 'busy', text: 'Recording…' };
  renderPptBar(p, d);
  const res = await api.report({ procUid: d.procUid, searchNumber: d.searchNumber, pptName: p.name, title: p.title, ...payload });
  p.rec = res?.ok ? { state: 'ok', text: `✓ ${okText}` }
    : { state: 'bad', text: res?.needsLogin ? '⚠ Not recorded: sign in to PTS' : `⚠ Not recorded (${res?.message || 'error'})` };
  renderPptBar(p, d);
}
function renderPptBar(p, d) {
  const docEl = p.h.closest('.doc');
  if (docEl) renderPpt(docEl, d, p);
}

async function loadSpaSteps(docEl, d, p) {
  if (p.loading || p.rows.length) return;
  p.loading = true; p.loadError = '';
  renderPpt(docEl, d, p);
  const res = await api.spa({ file: p.spa, procUid: d.procUid });
  p.loading = false;
  if (res?.error) { p.loadError = res.needsLogin ? 'Sign in to PTS to load the steps' : `Couldn't load the steps (${res.message})`; renderPpt(docEl, d, p); return; }
  const html = String(res.html || '');
  const parsed = new DOMParser().parseFromString(`<table><tbody id="spa-rows">${stripStyles(html)}</tbody></table>`, 'text/html');
  const holder = parsed.getElementById('spa-rows') || parsed.body;
  if (!holder.querySelector('tr')) { // not table rows: put whatever came back in a wrapper
    const div = parsed.createElement('div'); div.innerHTML = stripStyles(html);
    cleanContent(div, res.base, d);
    p.body.prepend(...div.childNodes);
  } else {
    cleanContent(holder, res.base, d);
    let tbody = p.body.querySelector(':scope > table > tbody');
    if (!tbody) { const t = el('table', { class: 'grid' }); tbody = el('tbody'); t.append(tbody); p.body.append(t); }
    tbody.querySelectorAll(':scope > tr[data-content-type="step"]').forEach((r) => r.remove());
    tbody.append(...holder.childNodes);
  }
  p.rows = [...p.body.querySelectorAll('tr[data-content-type="step"]')].filter((r) => !r.getAttribute('data-pptid') || r.getAttribute('data-pptid') === p.name);
  p.rows.forEach(markAnswers);
  api.log(`test ${p.name} (${p.spa}): ${html.length} characters, ${p.rows.length} steps`);
  if (!p.rows.length) p.loadError = 'This test came back without steps';
  const st = pptState(d, p.name);
  if (p.rows.length && !p.rows.some((r) => r.getAttribute('data-pptstepid') === st.step)) st.step = p.rows[0].getAttribute('data-pptstepid');
  renderPpt(docEl, d, p);
}

function openPpt(docEl, d, p, { step, scroll = true } = {}) {
  const st = pptState(d, p.name);
  const wasOpen = st.open;
  st.open = true;
  if (step) st.step = step;
  else if (!st.step && p.rows.length) st.step = p.rows[0].getAttribute('data-pptstepid');
  save();
  renderPpt(docEl, d, p);
  if (p.spa && !p.rows.length) loadSpaSteps(docEl, d, p);
  if (!wasOpen) report(p, d, {}, 'Test start recorded to Vehicle History');
  if (scroll) p.h.scrollIntoView({ block: 'start' });
}

// Open a pinpoint test by Ford's id for it (a letter like "A", or an id like "G2198756p1"), e.g. from a
// Diagnostics search result or a DTC chart
function findPpt(docEl, id) {
  const ppts = docEl._ppts;
  if (!ppts || !id) return null;
  const want = String(id).replace(/^PPT/i, '');
  if (ppts.has(want)) return ppts.get(want);
  for (const p of ppts.values()) {
    if (p.spa === want || p.rows.some((r) => r.getAttribute('data-pptid') === want)) return p;
    if (p.name.toLowerCase() === want.toLowerCase()) return p;
  }
  return null;
}
function goToPpt(docEl, d, id) {
  const p = findPpt(docEl, id);
  if (p) { openPpt(docEl, d, p); p.h.classList.remove('flash'); void p.h.offsetWidth; p.h.classList.add('flash'); return true; }
  const target = docEl.querySelector(`#PPT${CSS.escape(String(id))}`) || docEl.querySelector(`#n-${CSS.escape(String(id))}`) || docEl.querySelector(`[name="${CSS.escape(String(id))}"]`);
  if (target) { target.scrollIntoView({ block: 'start' }); return true; }
  api.log(`pinpoint test ${id} not found in ${d.searchNumber}/${d.procUid}`);
  return false;
}

// A jump to a pinpoint test or one of its steps (from the symptom chart, a link, or a reopened tab)
function pinpointJump(docEl, target) {
  if (pptMode() !== 'single' || !docEl._ppts) return false;
  const d = activeDoc();
  if (!d) return false;
  const next = target.nextElementSibling;
  const h = target.closest('h3.ppt[data-ppt]') || (next?.matches('h3.ppt[data-ppt]') ? next : null);
  if (h && docEl._ppts.has(h.getAttribute('data-ppt'))) { openPpt(docEl, d, docEl._ppts.get(h.getAttribute('data-ppt'))); return true; }
  const row = target.closest('tr[data-content-type="step"]');
  const p = row && docEl._ppts.get(row.getAttribute('data-pptid'));
  if (!p) return false;
  openPpt(docEl, d, p, { step: row.getAttribute('data-pptstepid'), scroll: false });
  p.bar.scrollIntoView({ block: 'start' });
  return true;
}

function stepInfo(row, answerRow, answer, mode) {
  const id = stepLabel(row);
  let title = (row.querySelector(`a[name="${CSS.escape(row.getAttribute('data-pptstepid') || '')}"]`)?.nextElementSibling?.textContent
    || row.querySelector('td.gray b')?.textContent || '').replace(/\s+/g, ' ').trim();
  if (id && title.startsWith(id)) title = title.slice(id.length).trim();
  const question = (row.querySelector('b[data-content-type="stepquestion"]')?.textContent || '').trim();
  let outcome = '';
  if (answerRow?.classList.contains('ppt-outcome')) {
    const cells = [...answerRow.children];
    outcome = (cells[cells.length - 1]?.textContent || '').replace(/\s+/g, ' ').trim();
  }
  return { id, title, question, answer, outcome, mode };
}

function pinpointClick(docEl, d, e) {
  const ppts = docEl._ppts;
  if (!ppts || !ppts.size) return false;
  const single = pptMode() === 'single';
  // the test's title opens/closes it
  const h = e.target.closest('h3.ppt.toggle');
  if (h && single) {
    const p = ppts.get(h.getAttribute('data-ppt'));
    const st = pptState(d, p.name);
    if (st.open) { st.open = false; save(); renderPpt(docEl, d, p); } else openPpt(docEl, d, p, { scroll: false });
    return true;
  }
  // Back / Start over
  const btn = e.target.closest('[data-ppt-action]');
  if (btn) {
    const p = [...ppts.values()].find((x) => x.bar.contains(btn));
    const st = p && pptState(d, p.name);
    if (!p) return true;
    if (btn.getAttribute('data-ppt-action') === 'back' && st.history.length) st.step = st.history.pop().step;
    if (btn.getAttribute('data-ppt-action') === 'restart') { st.history = []; st.answers = {}; st.step = p.rows[0].getAttribute('data-pptstepid'); }
    save(); renderPpt(docEl, d, p);
    p.bar.scrollIntoView({ block: 'nearest' });
    return true;
  }
  // an answer (Yes / No …)
  const answerRow = e.target.closest('tr.ppt-answer');
  const row = answerRow?.closest('tr[data-content-type="step"]');
  const p = row && ppts.get(row.getAttribute('data-pptid'));
  if (!p) return false;
  const answer = answerRow.querySelector('b.stepresult')?.id || '';
  const isOutcome = answerRow.classList.contains('ppt-outcome');
  // "GO to A2" answers move on by themselves; for an action ("INSTALL… REFER to: …") only a link you click opens
  const clicked = e.target.closest('a[data-jump], a[data-open], a[data-url]');
  const jumps = answerRow.querySelectorAll('a[data-jump]');
  // one "GO to" goes there; with a choice ("…built before May 12, GO to A3 / after, GO to A4") you click the one that applies
  const link = isOutcome ? clicked : (clicked || (jumps.length === 1 ? jumps[0] : null) || answerRow.querySelector('a[data-open]'));
  const choose = !isOutcome && !link && jumps.length > 1;
  if (!single) {
    // all-steps view: Ford still records the step when you follow its link
    if (link && !p.legacy) report(p, d, { step: stepInfo(row, answerRow, answer, 'MULTI') }, `${stepLabel(row)} ${answer} recorded`);
    return false; // the normal link handling carries on
  }
  e.preventDefault();
  const st = pptState(d, p.name);
  const stepId = row.getAttribute('data-pptstepid');
  const again = st.answers[stepId] === answer && (isOutcome || st.choosing === stepId);
  st.answers[stepId] = answer;
  st.choosing = choose ? stepId : '';
  if (p.legacy) {
    p.rec = choose ? { state: '', text: 'Click the step that applies to this vehicle' } : null;
  } else if (!again) report(p, d, { step: stepInfo(row, answerRow, answer, 'SINGLE') },
    choose ? `${stepLabel(row)}: ${answer} recorded. Now click the step that applies to this vehicle` : `${stepLabel(row)}: ${answer} recorded to Vehicle History`);
  const hash = link?.dataset.jump;
  const next = hash && p.rows.find((r) => r.getAttribute('data-pptstepid') === hash);
  if (next) {
    st.history.push({ step: stepId, answer });
    st.step = hash;
    save(); renderPpt(docEl, d, p);
    p.bar.scrollIntoView({ block: 'start' });
  } else if (hash) {
    save(); renderPpt(docEl, d, p);
    jumpTo(docEl, hash, true); // another pinpoint test on this page
  } else if (link?.dataset.open) {
    save(); renderPpt(docEl, d, p);
    const target = JSON.parse(link.dataset.open);
    openDoc({ ...target, section: '' }, { newTab: true, hash: target.hash });
  } else if (link?.dataset.url) {
    save(); renderPpt(docEl, d, p);
    api.openUrl(link.dataset.url);
  } else if (choose) {
    save(); renderPpt(docEl, d, p);
  } else {
    save(); renderPpt(docEl, d, p); // an action to do (INSTALL…, REPAIR…): the test ends here
  }
  return true;
}

$('btn-expand').onclick = () => {
  state.pptMode = pptMode() === 'single' ? 'all' : 'single';
  save();
  const node = $('content').querySelector('.doc');
  const d = activeDoc();
  if (node?._ppts && d) for (const p of node._ppts.values()) {
    renderPpt(node, d, p);
    if (p.spa && !p.rows.length && pptMode() === 'all') loadSpaSteps(node, d, p);
  }
  updateModeButton(node);
};
function updateModeButton(node) {
  const has = !!node?._ppts?.size;
  $('btn-expand').hidden = !has;
  $('btn-expand').textContent = pptMode() === 'single' ? 'Show all steps' : 'Single-step view';
  $('btn-expand').title = pptMode() === 'single'
    ? 'Show every step at once (Ford only treats single-step runs as a recorded test)'
    : "Back to Ford's single-step view, where each answer is recorded to Vehicle History";
}

// ---------- layout ----------
function applyLayout() {
  document.documentElement.style.setProperty('--toc', `${Math.max(220, Math.min(700, state.tocWidth || 340))}px`);
  document.body.classList.toggle('toc-hidden', !!state.tocHidden);
}
$('btn-toc').onclick = () => { state.tocHidden = !state.tocHidden; applyLayout(); save(); };
$('toc-resize').addEventListener('mousedown', (e) => {
  e.preventDefault();
  const move = (ev) => { state.tocWidth = Math.max(220, Math.min(700, ev.clientX)); applyLayout(); };
  const up = () => { window.removeEventListener('mousemove', move); window.removeEventListener('mouseup', up); save(); };
  window.addEventListener('mousemove', move);
  window.addEventListener('mouseup', up);
});
let searchTimer = null;
$('search').addEventListener('input', (e) => {
  clearTimeout(searchTimer);
  searchTimer = setTimeout(() => { state.query = e.target.value; save(); if (tree) renderTree(); }, 150);
});
$('zoom').onclick = () => { $('zoom').hidden = true; };
let scrollTimer = null;
$('content').addEventListener('scroll', () => {
  clearTimeout(scrollTimer);
  scrollTimer = setTimeout(() => { rememberScroll(); save(); }, 300);
});
window.addEventListener('keydown', (e) => {
  if (e.key === 'Escape') $('zoom').hidden = true;
  if (e.ctrlKey && e.key.toLowerCase() === 'f') { e.preventDefault(); if (state.tocHidden) { state.tocHidden = false; applyLayout(); } $('search').focus(); $('search').select(); }
  if (e.ctrlKey && e.key.toLowerCase() === 'w' && state.active) { e.preventDefault(); closeDoc(state.active); }
  if (e.altKey && e.key === 'ArrowLeft') $('btn-back').click();
});
window.addEventListener('beforeunload', () => { rememberScroll(); api.save(JSON.parse(JSON.stringify(state))); });

window.addEventListener('error', (e) => api.log(`error: ${e.message} at ${e.lineno}`));
window.addEventListener('unhandledrejection', (e) => api.log(`error: ${e.reason && e.reason.stack || e.reason}`));
start();