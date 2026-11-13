// Labor Times, showing the results: the report as operation cards with their options (hours, warranty and customer pay
// time, Parts and Add buttons), links SLTS follows, and scrolling to the operation that was asked for.
'use strict';

// ---------- showing results ----------
let lastOps = [];
function showState(...kids) { $('l-body').replaceChildren(el('div', { class: 'state' }, ...kids)); }
function failed(e, retry) {
  const actions = el('div', { class: 'actions' });
  if (e.needsLogin) actions.append(el('button', { class: 'primary', text: 'Sign in to PTS', onclick: async (ev) => { ev.target.disabled = true; await api.signIn(); page = null; retry(); } }));
  else actions.append(el('button', { class: 'primary', text: 'Try again', onclick: () => { page = null; retry(); } }));
  actions.append(el('button', { class: 'ghost', text: 'Open log', onclick: () => api.openLog() }));
  $('l-body').replaceChildren(el('div', { class: 'state' },
    el('h2', { text: e.needsLogin ? 'Sign in to PTS' : "Couldn't get labor times" }),
    el('p', { text: e.needsLogin ? 'Labor times need your PTS login.' : String(e.message || e) }), actions));
}
function hint() {
  $('l-body').replaceChildren(el('div', { class: 'd-hint' },
    el('p', {}, 'Search the way SLTS does: by ', el('b', { text: 'section' }), ', ', el('b', { text: 'operation number' }), ' (e.g. 19700), ',
      el('b', { text: 'word' }), ', part number or the alphabetic index.'),
    el('p', {}, 'Press ', el('b', { text: '+ Add' }), ' on an option to put it in your list on the right: it adds up the ',
      el('b', { text: 'warranty' }), ' time and the ', el('b', { text: 'customer pay' }), ` time (warranty × ${CUSTOMER_PAY}), checks Ford's overlaps, and lists the one-time use parts.`)));
}

function showResults(doc, q) {
  const rep = parseReport(doc);
  if (!rep || !rep.ops.length) {
    lastOps = [];
    // Not an operation report (an index, a message, nothing found): show what SLTS sent, tidied up
    // SLTS's results: everything in its form below the search controls, minus its standing notes
    const form = doc.querySelector('form');
    let box = null;
    if (form) {
      box = form.cloneNode(true);
      box.querySelectorAll('#Table_Main, #TableRow_Wiring_SearchDoc, #TableRow_DiffMarket, #MinimumOperationNumbers, #ValidateOperationNumbers, #DuplicateOperationNumbers, input[type="hidden"]')
        .forEach((n) => n.remove());
    }
    const msg = [...doc.querySelectorAll('#MinimumOperationNumbers, #ValidateOperationNumbers, #Label_InfoNotAvaiable_DiffMarket, #Label_DiffMarket')]
      .map((n) => clean(n.textContent)).filter(Boolean);
    const plain = box ? tidy(box) : null;
    const text = plain ? clean(plain.textContent) : '';
    api.log(`labor: ${currentType()} search "${q}" gave no operation report; page text: ${text.slice(0, 400) || '(none)'}`);
    $('l-body').replaceChildren(...[
      msg.length ? el('div', { class: 'o-card o-msg', text: msg.join(' ') }) : null,
      text
        ? el('div', { class: 'o-card l-raw' }, plain)
        : el('div', { class: 'd-empty', text: 'SLTS found nothing for that search.' }),
    ].filter(Boolean));
    return;
  }
  lastOps = rep.ops;
  const byCode = new Map(rep.ops.map((o) => [o.code, o]));
  const roots = rep.ops.filter((o) => !o.parentCode || !byCode.has(o.parentCode));
  const kids = (o) => rep.ops.filter((x) => x.parentCode === o.code);
  const cards = [];
  let group = null;
  for (const o of roots) {
    if (o.group && o.group !== group) { group = o.group; cards.push(el('h2', { class: 'l-group', text: group })); }
    cards.push(opCard(o, kids(o).map((k) => opCard(k, kids(k).map((s) => opCard(s))))));
  }
  $('l-body').replaceChildren(el('div', { class: 'l-title', text: rep.title }), ...cards);
  // An operation number search: bring the one asked for into view
  const want = String(q || '').toUpperCase().replace(/\s+/g, '');
  if (want) {
    let hit = document.querySelector(`[data-op="${CSS.escape(want)}"]`) || document.querySelector(`[data-opt="${CSS.escape(want)}"]`);
    // an option's number: the operation box it's in
    if (hit && !hit.matches('[data-op]')) hit = hit.closest('[data-op]') || hit;
    // after the page has settled (a "Back" button may be added above), glide down to the TOP of its box
    // (just under the page's header bar), so a long operation starts at its title instead of somewhere in the middle
    if (hit) setTimeout(() => {
      // the part of the page that scrolls (the page body here, not the window)
      let sc = hit.parentElement;
      while (sc && sc !== document.documentElement && !(sc.scrollHeight > sc.clientHeight && /(auto|scroll)/.test(getComputedStyle(sc).overflowY))) sc = sc.parentElement;
      sc ||= document.scrollingElement;
      const head = document.querySelector('.o-head');
      const y = hit.getBoundingClientRect().top - sc.getBoundingClientRect().top + sc.scrollTop - (head ? head.getBoundingClientRect().height : 0) - 12;
      sc.scrollTo({ top: Math.max(0, y), behavior: 'smooth' });
      hit.classList.add('l-flash');
    }, 150);
  }
}

async function follow(target, arg) {
  if (busy) return;
  busy = true;
  showState(el('div', { class: 'spinner' }), 'Asking SLTS…');
  try { showResults(await postBack({}, { eventTarget: target, eventArgument: arg }), ''); } catch (e) { failed(e, () => follow(target, arg)); } finally { busy = false; }
}
// Ford's own markup for things this page doesn't redraw (index lists, messages), minus scripts and styles
function tidy(node) {
  const c = node.cloneNode(true);
  c.querySelectorAll('script, style, img, input, select').forEach((n) => n.remove());
  c.querySelectorAll('*').forEach((n) => {
    for (const a of [...n.attributes]) if (!['href', 'colspan', 'rowspan'].includes(a.name)) n.removeAttribute(a.name);
    if (n.tagName === 'A') {
      const href = n.getAttribute('href') || '';
      n.removeAttribute('href');
      const code = clean(n.textContent).replace(/\s+/g, '');
      if (/^\d{3,6}[A-Z]{0,2}\d*$/i.test(code)) { // an operation number: search for it
        n.className = 'l-link';
        n.addEventListener('click', () => { $('l-type').value = 'Operation'; $('l-q').hidden = false; $('l-q').value = code.replace(/[A-Z].*$/i, ''); search(); });
      } else if (/__doPostBack\(/.test(href)) { // SLTS's own link (an index entry, a word search hit): post it back
        const m = href.match(/__doPostBack\(\s*'([^']*)'\s*,\s*'([^']*)'\s*\)/);
        if (m) { n.className = 'l-link'; n.addEventListener('click', () => follow(m[1], m[2])); }
      } else if (/^https:/i.test(href)) { n.className = 'l-link'; n.addEventListener('click', () => api.openUrl(href)); }
    }
  });
  return c;
}

const KIND_LABEL = { base: '', combo: 'Combination', supp: 'Supplement' };
function opCard(o, children = []) {
  const parent = o.parentCode ? lastOps.find((x) => x.code === o.parentCode) : null;
  const options = o.options.map((opt) => optionRow(o, opt));
  const card = el('section', { class: `l-op ${o.kind}`, 'data-op': o.code },
    el('div', { class: 'l-op-head' },
      KIND_LABEL[o.kind] ? el('span', { class: `l-kind ${o.kind}`, text: KIND_LABEL[o.kind], title: o.kind === 'combo' && parent ? `Claimed with the base operation ${parent.code}` : null }) : null,
      el('span', { class: 'l-name', text: o.name }),
      ...o.parts.map((p) => el('span', { class: 'l-part', text: p, title: 'Base part number' }))),
    o.desc.length ? el('div', { class: 'l-desc', text: o.desc.join(' ') }) : null,
    options.length ? el('div', { class: 'l-opts' },
      el('div', { class: 'l-opt l-opt-head' }, el('span', { text: 'Option' }), el('span', { text: 'Op code' }), el('span', { class: 'n', text: 'Warranty' }),
        el('span', { class: 'n', text: 'Customer pay' }), el('span'), el('span')), ...options)
      : el('div', { class: 'l-desc muted', text: 'No time listed for this vehicle.' }),
    ...o.notes.map((n) => el('div', { class: 'l-note' }, n.url ? el('button', { class: 'link', text: n.text, onclick: () => api.openUrl(n.url) }) : n.text)),
    o.overlaps.length ? el('details', { class: 'l-overlaps' }, el('summary', { text: 'Overlaps (don’t claim together)' }),
      ...o.overlaps.map((t) => el('div', { text: t }))) : null,
    children.length ? el('div', { class: 'l-children' }, ...children) : null);
  return card;
}

function optionRow(o, opt) {
  const w = num(opt.time);
  const add = el('button', { 'data-add': `${opt.code}|${opt.qual}`, onclick: () => { toggleTray(o, opt); refreshAdds(); } });
  paintAdd(add);
  return el('div', { class: 'l-opt', 'data-opt': opt.code },
    el('span', { class: 'l-qual', text: opt.qual || 'All' }),
    el('span', { class: 'l-code', title: 'Click to copy', text: opt.code, onclick: () => copy(opt.code) }),
    el('span', { class: 'n l-w', text: w == null ? (opt.time || '—') : hrs(w) }),
    el('span', { class: 'n l-c', text: w == null ? '—' : hrs(w * CUSTOMER_PAY) }),
    opt.cart ? el('button', { class: 'ghost small', text: 'Parts', title: 'One-time use parts and required items', onclick: () => showParts([{ op: o, opt }]) }) : el('span'),
    add);
}
