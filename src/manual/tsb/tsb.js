// TSBs: the vehicle's TSBs, GSBs, SSMs (and ISBs) from Ford's list for this VIN (TSB_SSM DisplayArticles),
// with Ford's categories and a filter box, and each article redrawn in the app's style so it follows
// light / dark mode. "Ford's page" opens Ford's own version as a tab. The Diagnostics tab sends TSBs here.
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
function toast(text) {
  const t = $('o-toast');
  t.textContent = text; t.hidden = false;
  clearTimeout(toast.timer);
  toast.timer = setTimeout(() => { t.hidden = true; }, 1600);
}
const TSB_BASE = 'https://www.fordtechservice.dealerconnection.com/vdirsnet/TSB_SSM/';
const TYPES = { T: 'TSB', G: 'GSB', S: 'SSM', I: 'ISB' };

// Ford's inline styles would trip this page's security rules; note which elements they hid first
function prepare(html) {
  return String(html)
    .replace(/<(script|style|noscript)\b[\s\S]*?<\/\1>/gi, '')
    .replace(/<link\b[^>]*>/gi, '')
    .replace(/\sstyle\s*=\s*("[^"]*"|'[^']*')/gi, (m, v) => (/display\s*:\s*none|visibility\s*:\s*hidden/i.test(v) ? ' data-hidden="1"' : ''));
}
const parse = (html) => new DOMParser().parseFromString(prepare(html), 'text/html');

let state = { q: '', cat: '', type: '' };
let info = {};
function save() { api.save({ q: state.q, cat: state.cat, type: state.type }); }

// ---------- Ford's list ----------
let articles = [];   // { type, id, title, date, summary, url, cats:Set }
let cats = [];       // { id, label, depth, count }
function readList(html) {
  const doc = parse(html);
  articles = [];
  for (const li of doc.querySelectorAll('#articles li, ul.filterThis li')) {
    const a = li.querySelector('.article a, a.small');
    if (!a) continue;
    const href = a.getAttribute('href') || '';
    const hash = href.includes('#') ? href.slice(href.indexOf('#') + 1) : href;
    const m = /DisplayArticle\.aspx/i.test(hash) ? hash : (a.getAttribute('onclick') || '').match(/DisplayArticle\.aspx[^'"]*/i)?.[0];
    if (!m) continue;
    const url = new URL(m.replace(/&amp;/g, '&'), TSB_BASE).href;
    const sp = new URL(url).searchParams;
    const label = clean(a.textContent);                          // "TSB 26-2414"
    const classes = (li.getAttribute('class') || '').split(/\s+/).filter(Boolean);
    const letter = classes.find((c) => /^[TGSI]$/.test(c)) || (label.match(/^(TSB|GSB|SSM|ISB)/i)?.[1] || 'T').charAt(0).toUpperCase();
    const after = clean(li.querySelector('.article')?.textContent || '').replace(label, '');
    articles.push({
      type: TYPES[letter] || (sp.get('type') || 'TSB').toUpperCase(),
      id: sp.get('id') || label.replace(/^\S+\s+/, ''),
      title: label,
      date: (after.match(/Effective\s+(.+)$/i) || [])[1] || '',
      summary: clean(li.querySelector('.summary')?.textContent) || clean(sp.get('t')),
      url,
      cats: new Set(classes),
    });
  }
  // Ford's category menu: nested lists of <a id="CATEGORY"><div class="desc">…</div><div class="acnt">n</div></a>
  cats = [];
  const root = doc.querySelector('#container > ul, #symptoms ul');
  const walk = (ul, depth) => {
    for (const li of ul.children) {
      if (li.tagName !== 'LI') continue;
      const a = li.querySelector(':scope > a');
      const id = a?.getAttribute('id');
      const label = clean(a?.querySelector('.desc')?.textContent || a?.textContent);
      if (id && id !== 'x' && label) {
        const count = Number(clean(a.querySelector('.acnt')?.textContent)) || articles.filter((x) => x.cats.has(id)).length;
        if (count) cats.push({ id, label, depth, count });
      }
      const sub = li.querySelector(':scope > ul');
      if (sub) walk(sub, depth + 1);
    }
  };
  if (root) walk(root, 0);
  api.log(`tsb: ${articles.length} articles, ${cats.length} categories`);
}

function renderList() {
  // categories
  const sel = $('t-cat');
  sel.replaceChildren(el('option', { value: '', text: 'All categories' }),
    ...cats.map((c) => el('option', { value: c.id, text: `${'  '.repeat(c.depth)}${c.depth ? '› ' : ''}${c.label} (${c.count})` })));
  sel.value = cats.some((c) => c.id === state.cat) ? state.cat : '';
  sel.hidden = !cats.length;
  // type tabs with counts (for what the category and filter leave)
  const words = state.q.toLowerCase().split(/\s+/).filter(Boolean);
  const base = articles.filter((a) => (!sel.value || a.cats.has(sel.value))
    && words.every((w) => `${a.title} ${a.id} ${a.summary} ${[...a.cats].join(' ')}`.toLowerCase().includes(w)));
  const counts = { '': base.length };
  for (const a of base) counts[a.type] = (counts[a.type] || 0) + 1;
  const types = ['', ...['TSB', 'GSB', 'SSM', 'ISB'].filter((t) => articles.some((a) => a.type === t))];
  if (state.type && !types.includes(state.type)) state.type = '';
  $('t-types').replaceChildren(...types.map((t) => el('button', {
    class: `d-tab${state.type === t ? ' on' : ''}${counts[t] ? '' : ' empty'}`, type: 'button',
    onclick: () => { state.type = t; save(); renderList(); },
  }, t ? `${t}s` : 'All', el('span', { class: 'n', text: String(counts[t] || 0) }))));
  const shown = base.filter((a) => !state.type || a.type === state.type);
  $('t-body').replaceChildren(
    shown.length ? el('div', { class: 't-count', text: `${shown.length} of ${articles.length}, newest first` }) : null,
    shown.length
      ? el('div', { class: 'd-list' }, ...shown.map((a) => el('button', { class: 'd-item t-item', type: 'button', onclick: () => openArticle(a) },
        el('div', { class: 't' }, el('span', { class: `kind ${a.type.toLowerCase()}`, text: a.type }), el('span', { class: 'num', text: a.id }),
          el('span', { class: 'date', text: a.date })),
        el('div', { class: 'desc', text: a.summary }))))
      : el('div', { class: 'd-empty', text: articles.length ? 'Nothing matches that.' : 'Ford lists no TSBs, GSBs or SSMs for this VIN.' }));
}

// ---------- one article ----------
let current = null;
function showList() {
  current = null;
  $('t-article').hidden = true;
  $('t-list-view').hidden = false;
  ['btn-back', 'btn-print', 'btn-ford'].forEach((id) => { $(id).hidden = true; });
  $('t-title').textContent = 'TSBs';
}
async function openArticle(a) {
  current = a;
  $('t-list-view').hidden = true;
  $('t-article').hidden = false;
  ['btn-back', 'btn-print', 'btn-ford'].forEach((id) => { $(id).hidden = false; });
  $('t-title').textContent = a.title || `${a.type} ${a.id}`;
  window.scrollTo(0, 0);
  $('t-article').replaceChildren(el('div', { class: 'state' }, el('div', { class: 'spinner' }), 'Loading the article…'));
  let res;
  try { res = await api.tsbArticle(a.url); } catch (e) { res = { error: true, message: String(e?.message || e) }; }
  if (current !== a) return;
  if (res?.error) {
    $('t-article').replaceChildren(el('div', { class: 'state' }, el('h2', { text: res.needsLogin ? 'Sign in to PTS' : "Couldn't load the article" }),
      el('p', { text: res.message || '' }),
      el('div', { class: 'actions' },
        res.needsLogin ? el('button', { class: 'primary', text: 'Sign in to PTS', onclick: async () => { await api.signIn(); openArticle(a); } })
          : el('button', { class: 'primary', text: 'Try again', onclick: () => openArticle(a) }),
        el('button', { class: 'ghost', text: "Ford's page", onclick: fordPage }))));
    return;
  }
  if (res.pdf) {
    $('t-article').replaceChildren(el('div', { class: 'o-card o-msg' }, el('p', { text: 'Ford publishes this one as a PDF.' }),
      el('button', { class: 'primary', text: "Open Ford's page", onclick: fordPage })));
    return;
  }
  const head = el('div', { class: 't-head' }, el('span', { class: `kind d-kind ${a.type.toLowerCase()}`, text: a.type }), el('b', { text: a.id }),
    a.summary ? el('span', { class: 'muted', text: a.summary }) : null, a.date ? el('span', { class: 'muted small', text: `Effective ${a.date}` }) : null);
  const nodes = redraw(res.html, res.url);
  const pics = $('t-article').ownerDocument && nodes.flatMap((n) => (n.nodeType === 1 ? [...(n.tagName === 'IMG' ? [n] : []), ...n.querySelectorAll('img')] : []));
  if (pics.length) api.log(`tsb: ${a.type} ${a.id} has ${pics.length} picture${pics.length === 1 ? '' : 's'}, e.g. ${decodeURIComponent((pics[0].getAttribute('src') || '').replace(/^ptsasset:\/\/f\/\?u=/, '')).slice(0, 200)}`);
  $('t-article').replaceChildren(head, ...nodes);
  if (!clean($('t-article').textContent.replace(clean(head.textContent), '')) && !$('t-article').querySelector('img')) {
    $('t-article').append(el('div', { class: 'o-card o-msg', text: "Ford's article came back empty here. Use “Ford's page” to see it." }));
    api.log(`tsb: nothing readable in ${String(res.url).slice(0, 200)}`);
  }
}
function fordPage() { if (current) api.openArticle(current.url, `${current.type} ${current.id}`); }

// Ford's HTML → the app's styling
const KEEP = new Set(['href', 'src', 'alt', 'colspan', 'rowspan', 'name', 'id', 'start', 'type']);
function redraw(html, base) {
  const doc = parse(html);
  const root = doc.body;
  root.querySelectorAll('[data-hidden], input, select, button, textarea, iframe, frame, object, embed, map, area, form > [type="hidden"]').forEach((n) => n.remove());
  root.querySelectorAll('table').forEach((t) => { if (Number(t.getAttribute('border') || 0) > 0 || t.querySelector('th')) t.classList.add('grid'); });
  root.querySelectorAll('font, span').forEach((n) => { if (n.tagName === 'FONT' || !n.attributes.length) n.replaceWith(...n.childNodes); });
  root.querySelectorAll('center').forEach((c) => { const d = doc.createElement('div'); d.className = 'center'; d.append(...c.childNodes); c.replaceWith(d); });
  root.querySelectorAll('img').forEach((img) => {
    const src = img.getAttribute('src') || '';
    const w = Number(img.getAttribute('width') || 0), h = Number(img.getAttribute('height') || 0);
    if (!src || /spacer|empty\.gif|blank\.gif|printer|print\.(gif|png|jpe?g)|\/icons\//i.test(src) || (w && w <= 4) || (h && h <= 4)) { img.remove(); return; }
    let abs = '';
    try { abs = new URL(src, base).href; } catch { img.remove(); return; }
    img.setAttribute('src', /^data:/i.test(src) ? src : `ptsasset://f/?u=${encodeURIComponent(abs)}`);
    img.classList.add('pic');
  });
  root.querySelectorAll('*').forEach((n) => {
    for (const at of [...n.attributes]) if (!KEEP.has(at.name) && !(at.name === 'class' && /^(grid|center|pic)( |$)/.test(at.value))) n.removeAttribute(at.name);
  });
  root.querySelectorAll('a').forEach((a) => {
    const href = (a.getAttribute('href') || '').trim();
    a.removeAttribute('href');
    let url = '';
    if (/^javascript:/i.test(href)) url = (href.match(/window\.open\(\s*['"]([^'"]+)['"]/i) || [])[1] || '';
    else if (href.startsWith('#')) { a.dataset.jump = href.slice(1); a.className = 'a-link'; return; }
    else url = href;
    if (!url) { if (!a.getAttribute('name')) a.replaceWith(...a.childNodes); return; }
    try { a.dataset.url = new URL(url, base).href; a.className = 'a-link'; } catch { a.replaceWith(...a.childNodes); }
  });
  // Ford's "*****" separator lines → a rule
  const walker = doc.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  const stars = [];
  for (let t = walker.nextNode(); t; t = walker.nextNode()) if (/^\s*\*{10,}\s*$/.test(t.nodeValue)) stars.push(t);
  stars.forEach((t) => t.replaceWith(doc.createElement('hr')));
  root.querySelectorAll('br').forEach((br) => {
    let next = br.nextSibling;
    while (next && next.nodeType === 3 && !next.textContent.trim()) next = next.nextSibling;
    if (next && next.nodeType === 1 && next.tagName === 'BR') br.remove();
  });
  root.querySelectorAll('p, div, td').forEach((p) => {
    if (p.querySelector('p, div, table')) return;
    const m = clean(p.textContent).match(/^(WARNING|CAUTION|NOTICE|NOTE)\b/);
    if (m) p.classList.add('callout', m[1].toLowerCase());
  });
  root.querySelectorAll('div, p').forEach((n) => { if (!clean(n.textContent) && !n.querySelector('img')) n.remove(); });
  return [...root.childNodes];
}

$('t-article').addEventListener('click', (e) => {
  const a = e.target.closest('.a-link');
  if (!a) return;
  e.preventDefault();
  if (a.dataset.jump) {
    const t = document.getElementById(a.dataset.jump) || document.querySelector(`[name="${CSS.escape(a.dataset.jump)}"]`);
    t?.scrollIntoView({ behavior: 'smooth', block: 'start' });
    return;
  }
  const url = a.dataset.url;
  // another bulletin: open it here
  if (/\/vdirsnet\/TSB_SSM\/DisplayArticle\.aspx/i.test(url)) {
    const sp = new URL(url).searchParams;
    const id = sp.get('id') || '';
    const known = articles.find((x) => x.id === id);
    openArticle(known || { type: (sp.get('type') || 'TSB').toUpperCase(), id, title: clean(a.textContent), summary: '', date: '', url });
  } else if (/\.pdf(?:$|[?#])/i.test(url)) api.openArticle(url, clean(a.textContent).slice(0, 40) || 'Document');
  else api.openUrl(url);
});

// ---------- one sent from the Diagnostics tab ----------
function openFromDiag(t) {
  if (!t?.id) return;
  const id = String(t.id).toUpperCase();
  const known = articles.find((a) => a.id.toUpperCase() === id);
  if (known) { openArticle(known); return; }
  // Not on this VIN's list (or the list isn't loaded): open it from Diagnostics' own link
  const type = String(t.type || 'TSB').toUpperCase();
  openArticle({ type, id: t.id, title: `${type} ${t.id}`, summary: t.title || '', date: '', url: t.url ? new URL(t.url, TSB_BASE).href : '' });
}

// ---------- loading ----------
function showState(...kids) { $('t-body').replaceChildren(el('div', { class: 'state' }, ...kids)); }
async function load() {
  showState(el('div', { class: 'spinner' }), 'Loading TSBs for this VIN…');
  let res;
  try { res = await api.tsbList(); } catch (e) { res = { error: true, message: String(e?.message || e) }; }
  if (res?.needsDealer) {
    const input = el('input', { class: 'o-odo', placeholder: 'USA12345', maxlength: '12' });
    const go = async () => { if (input.value.trim() && (await api.setDealerCode(input.value.trim()))) load(); else input.focus(); };
    input.addEventListener('keydown', (e) => { if (e.key === 'Enter') go(); });
    showState(el('h2', { text: 'Your Warranty Dealer Code' }),
      el('p', { text: "Ford's TSB list needs the dealership's code. It's also in ⚙ Settings." }),
      el('div', { class: 'actions' }, input, el('button', { class: 'primary', text: 'Save', onclick: go })));
    setTimeout(() => input.focus(), 0);
    return false;
  }
  if (res?.error) {
    showState(el('h2', { text: res.needsLogin ? 'Sign in to PTS' : "Couldn't load TSBs" }), el('p', { text: res.message || '' }),
      el('div', { class: 'actions' }, res.needsLogin
        ? el('button', { class: 'primary', text: 'Sign in to PTS', onclick: async () => { await api.signIn(); load(); } })
        : el('button', { class: 'primary', text: 'Try again', onclick: load })));
    return false;
  }
  readList(res.html);
  renderList();
  return true;
}

$('t-q').addEventListener('input', () => { state.q = $('t-q').value; save(); renderList(); });
$('t-form').addEventListener('submit', (e) => e.preventDefault());
$('t-cat').addEventListener('change', () => { state.cat = $('t-cat').value; save(); renderList(); });
$('btn-back').onclick = showList;
$('btn-print').onclick = () => window.print();
$('btn-ford').onclick = fordPage;
document.addEventListener('keydown', (e) => { if (e.key === 'Escape' && current) showList(); });
let pendingDiag = null; // sent from Diagnostics before the list loaded
api.onOpen?.((o) => { if (o?.tsb) { if (articles.length) openFromDiag(o.tsb); else pendingDiag = o.tsb; } });

(async () => {
  info = (await api.init().catch(() => null)) || {};
  if (info.state && typeof info.state === 'object') state = { ...state, ...info.state };
  $('t-q').value = state.q || '';
  $('t-sub').textContent = [info.ymm, info.vin].filter(Boolean).join(' · ') || 'Bulletins and messages for this VIN';
  if (info.open?.tsb) pendingDiag = info.open.tsb;
  await load();
  if (pendingDiag) { const t = pendingDiag; pendingDiag = null; openFromDiag(t); }
})();