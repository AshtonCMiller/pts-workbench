// TSBs, one article: fetched from Ford and redrawn in the app's style (Ford's HTML trimmed to plain content, pictures
// through the app's PTS connection), links inside it (another TSB opens here, a PDF as its own tab), and articles sent
// from the Diagnostics tab.
'use strict';

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
