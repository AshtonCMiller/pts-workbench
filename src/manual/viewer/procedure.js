// Workshop Manual, one procedure: Ford's HTML turned into the app's (no Ford scripts or styles; pictures through the
// app's PTS connection and zoomable; tables, notes and cautions in the app's style; links that open other procedures),
// jumping to a part of it, and the Back and Print buttons.
'use strict';

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
