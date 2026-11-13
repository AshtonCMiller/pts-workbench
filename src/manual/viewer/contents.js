// Workshop Manual, the contents: loading Ford's table of contents for this vehicle, reading it into folders and
// procedures (a procedure's parts listed under it, PC/ED's current-format sections only), and drawing it.
'use strict';

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
  $('book-title').replaceChildren(book.title || bookName(), ' ', el('small', { text: bookName() }));
  $('tree').replaceChildren(el('div', { class: 'note' }, el('div', { class: 'spinner' }), 'Loading the contents…'));
  const res = await api.tree();
  if (res?.error) return contentsError(res);
  const fit = vehicleFit(res.html || '', res.features);
  fullTree = parseTree(res.html || '', fit);
  filterable = !!fit;
  buildShown();
  const naCount = filterable ? countLeaves(fullTree, (n) => n.na) : 0;
  api.log(`contents: ${(res.html || '').length} characters, ${tree.length} top sections, ${leaves.length} entries` +
    (fit ? `, ${naCount} don't apply to this vehicle (options ${fit.matches.length} of ${fit.codes})` : ', not filtered for this vehicle'));
  if (!leaves.length) {
    tree = null;
    const text = String(res.html || '').replace(/<[^>]+>/g, ' ').replace(/\s+/g, ' ').trim();
    const raw = String(res.html || '');
    const i = raw.search(/<li|<a\s/i);
    api.log(`contents were empty; ${(raw.match(/<li/gi) || []).length} <li>, ${(raw.match(/<a\s/gi) || []).length} <a>, data-for ${(raw.match(/data-for/gi) || []).length}; sample: ${raw.slice(Math.max(0, i), i + 1500)}`);
    return contentsError({ message: 'Ford sent the contents back empty.', detail: text.slice(0, 300) || '(nothing at all)' });
  }
  $('book-title').append(el('small', { id: 'book-count' }));
  drawFilter();
  $('search').value = state.query || '';
  renderTree();
  if (!state.docs.length) showWelcome();
}

function contentsError(res) {
  $('tree').replaceChildren(el('div', { class: 'note' }, res.message || "Couldn't load the manual."));
  showProblem(res, loadContents);
}

// ---------- contents (from Ford's tree HTML) ----------
function parseTree(html, fit = null) {
  const doc = new DOMParser().parseFromString(stripStyles(html), 'text/html');
  const text = (n) => (n?.textContent || '').replace(/\s+/g, ' ').trim();
  // PC/ED books list their DTC charts and pinpoint tests twice: the current version (data-spa-file="Y") and an older one
  // for old browsers ("N"). Like Ford's page, only the current one is shown.
  const hasSpa = !!doc.querySelector('li[data-spa-file="Y"], li[data-spa-file="y"]');
  // Every procedure/document link, wherever it sits; the folders are the list items around it
  const anchors = [...doc.querySelectorAll('a[data-for]')].filter((a) => {
    const f = a.getAttribute('data-for') || '';
    if (hasSpa && a.closest('li[data-spa-file="N"], li[data-spa-file="n"]')) return false;
    return !a.hasAttribute('data-hidden') && !/adttestindex|spaindex_test/i.test(f) && text(a);
  });
  // a list item that is itself a procedure (its own link, not one in a list inside it): what's listed under it are its parts
  const ownLink = (li) => [...li.querySelectorAll('a[data-for]')].find((a) => a.closest('li') === li) || null;
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
  const docNodes = new Map(); // <li> -> the procedure it is (parts listed under it go in its children)
  const whole = new Map();    // "VF2s2p005" -> that procedure's entry, for its parts ("VF2s2p005:procsub1_2")
  const seen = new Set();
  for (const a of anchors) {
    const key = `${a.getAttribute('data-for')}|${a.getAttribute('data-procuid') || ''}`;
    if (!a.closest('li') && seen.has(key)) continue; // e.g. the same link again on the manual's cover page
    seen.add(key);
    // A part of a procedure listed in the contents (Quick Test Description › Quick Test, KOEO…) goes under that
    // procedure, however Ford's list happens to be laid out around it
    const partOf = splitTarget(a.getAttribute('data-for') || '');
    const parent = /^procsub/i.test(partOf.hash) ? whole.get(partOf.searchNumber) : null;
    const chain = [];
    if (!parent) for (let li = a.closest('li')?.parentElement?.closest('li'); li; li = li.parentElement?.closest('li')) chain.unshift(li);
    let list = parent ? (parent.children ||= []) : roots;
    for (const li of chain) {
      // a procedure with its parts listed under it: they go under the procedure, as in Ford's contents
      const proc = docNodes.get(li);
      if (proc) { list = (proc.children ||= []); continue; }
      let f = folders.get(li);
      if (!f) {
        const own = ownLink(li);
        f = { type: 'folder', title: folderTitle(li) || (own ? text(own) : '') || 'Section', children: [] };
        folders.set(li, f); list.push(f);
      }
      list = f.children;
    }
    const title = text(a);
    // for this vehicle? (its own options, and those of every section it sits in)
    let na = false;
    if (fit) {
      if (!qualFits(a.getAttribute('data-qual'), fit)) na = true;
      for (let li = a.closest('li'); li && !na; li = li.parentElement?.closest('li')) {
        const sec = li.querySelector(':scope > span[data-qual], :scope > [data-qual-source="section"]');
        if (sec && !qualFits(sec.getAttribute('data-qual'), fit)) na = true;
      }
    }
    const cls = a.getAttribute('class') || '';
    const forAttr = a.getAttribute('data-for') || '';
    if (/Quicklinkpdf-link/i.test(cls) || /^(https?:)?\//i.test(forAttr)) {
      list.push({ type: 'link', title, url: forAttr, na });
    } else {
      let { searchNumber, hash } = splitTarget(forAttr);
      const linkType = /publication-content/i.test(cls) ? 'PublicationContent' : 'Procedure';
      // a PC/ED DTC chart or pinpoint test in the current format: Ford asks for it as "spapced_…"
      if (/^y$/i.test(a.parentElement?.getAttribute('data-spa-file') || '') && !/^spapced_/i.test(searchNumber)) searchNumber = `spapced_${searchNumber}`;
      const node = { type: 'doc', title, na, link: { searchNumber, procUid: a.getAttribute('data-procuid') || '', linkType, hash } };
      list.push(node);
      if (!hash && !whole.has(searchNumber)) whole.set(searchNumber, node);
      const li = a.closest('li');
      if (li && ownLink(li) === a && !docNodes.has(li)) docNodes.set(li, node);
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
          class: `node folder d${Math.min(depth, 4)}${isOpen ? ' open' : ''}${n.na ? ' na' : ''} ${kindOf(n.title)}`, style: `--depth:${depth}`,
          title: n.na ? `${n.title}\n(doesn't apply to this vehicle)` : n.title,
          onclick: () => { toggleFolder(id); },
        }, el('span', { class: 'tw', text: isOpen ? '▾' : '▸' }), label,
        el('span', { class: 'count', text: String(n.count || '') })));
        if (isOpen) add(n.children, depth + 1);
      } else {
        const isOpen = !!n.children && open.has(id);
        frag.append(leafNode(n, depth, null, n.children ? { open: isOpen, id } : null));
        if (isOpen) add(n.children, depth + 1);
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

// parts: a procedure with parts listed under it ({ open, id }): its arrow shows or hides them, and opening it shows them
function leafNode(n, depth, extra, parts = null) {
  const on = n.type === 'doc' && activeDoc()?.key === docKey(n.link);
  return el('div', {
    class: `node ${n.type} d${Math.min(depth, 4)}${on ? ' on' : ''}${n.na ? ' na' : ''}${parts ? ' parent' : ''}${parts?.open ? ' open' : ''}`, style: `--depth:${depth}`,
    title: n.path.join(' › ') + (n.na ? "\n(doesn't apply to this vehicle)" : ''),
    onclick: (e) => { if (parts && !parts.open) openFolder(parts.id); openLeaf(n, e); },
    onauxclick: (e) => { if (e.button === 1) openLeaf(n, { ...e, newTab: true }); },
  }, el('span', { class: 'tw', text: parts ? (parts.open ? '▾' : '▸') : '',
    onclick: parts ? (e) => { e.stopPropagation(); toggleFolder(parts.id); } : null }),
  el('span', { class: 'label' }, extra || n.title),
  parts && n.count ? el('span', { class: 'count', text: String(n.count) }) : null);
}
function openFolder(id) {
  if (state.open.includes(id)) return;
  state.open = [...state.open, id];
  save();
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

function openLeaf(n, e) {
  if (n.type === 'link') { api.openUrl(n.url); return; }
  openDoc({ ...n.link, title: n.title, section: n.path.slice(-3, -1).join(' › ') }, { newTab: e?.ctrlKey || e?.newTab });
}
