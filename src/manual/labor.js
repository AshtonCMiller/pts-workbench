// Labor Times: Ford's SLTS (Service Labor Time Standards) for this vehicle, redrawn in the app's style.
// SLTS is an old ASP.NET page that works by posting its form back to itself, so this page keeps the
// last page Ford sent, fills in its form the way SLTS's own controls would, and reads the report out
// of what comes back. Picked operations go in a tray that adds up warranty and customer pay times.
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
// replaceChildren would show a skipped (null) piece as the text "null"
const fill = (node, ...kids) => node.replaceChildren(...kids.filter((k) => k != null && k !== false));
const clean = (t) => String(t || '').replace(/ /g, ' ').replace(/\s+/g, ' ').trim();
const CUSTOMER_PAY = 1.5; // customer pay time = warranty time × 1.5
const hrs = (n) => (Math.round(n * 100) / 100).toFixed(2).replace(/0$/, '');
const num = (t) => (/^\d+(\.\d+)?$/.test(String(t).trim()) ? Number(t) : null);

function toast(text) {
  const t = $('o-toast');
  t.textContent = text; t.hidden = false;
  clearTimeout(toast.timer);
  toast.timer = setTimeout(() => { t.hidden = true; }, 1600);
}
function copy(text, label) { api.copy(text); toast(`Copied ${label || text}`); }

// ---------- saved per vehicle (this tab) ----------
let saved = { tray: [], type: '', pick: '', q: '' };
function save() {
  api.save({ tray: saved.tray, type: saved.type, pick: saved.pick, q: saved.q });
}

// ---------- talking to SLTS ----------
let page = null; // { doc, url } — the last SLTS page, whose form the next request posts back
let busy = false;

// Ford's inline styles and scripts aren't needed, and this page's security rules would flag each one
const stripStyles = (html) => String(html).replace(/<(script|style)\b[\s\S]*?<\/\1>/gi, '').replace(/\sstyle\s*=\s*(?:"[^"]*"|'[^']*')/gi, '');
const parse = (html) => new DOMParser().parseFromString(stripStyles(html), 'text/html');
function setPage(res) {
  const doc = parse(res.html);
  page = { doc, url: res.url };
  return doc;
}
function formAction() {
  const f = page.doc.querySelector('form');
  try { return new URL(f?.getAttribute('action') || page.url, page.url).href; } catch { return page.url; }
}
// Every field SLTS's form would send (no image buttons: those are added for a click)
function formFields() {
  const out = {};
  const f = page.doc.querySelector('form');
  if (!f) return out;
  for (const n of f.querySelectorAll('input, select, textarea')) {
    const name = n.getAttribute('name');
    if (!name) continue;
    const type = (n.getAttribute('type') || '').toLowerCase();
    if (n.tagName === 'SELECT') {
      const opt = n.querySelector('option[selected]') || n.querySelector('option');
      out[name] = opt ? (opt.getAttribute('value') ?? opt.textContent) : '';
    } else if (type === 'image' || type === 'submit' || type === 'button' || type === 'reset') continue;
    else if (type === 'checkbox' || type === 'radio') { if (n.hasAttribute('checked')) out[name] = n.getAttribute('value') || 'on'; }
    else out[name] = n.getAttribute('value') || n.textContent || '';
  }
  return out;
}
async function request(req) {
  const res = await api.slts(req);
  if (res?.error) { const e = new Error(res.message || 'SLTS failed'); e.needsLogin = res.needsLogin; throw e; }
  return res;
}
async function start() {
  setPage(await request({ op: 'start' }));
}
// Post the form back: a dropdown change (eventTarget) or the "View" button (click)
async function postBack(overrides, { eventTarget = '', eventArgument = '', click = false } = {}) {
  if (!page) await start();
  const fields = { ...formFields(), ...overrides, __EVENTTARGET: eventTarget, __EVENTARGUMENT: eventArgument };
  if (click) { fields['ImageButton_View_SearchFor.x'] = '9'; fields['ImageButton_View_SearchFor.y'] = '9'; }
  const res = await request({ op: 'post', method: 'POST', url: formAction(), fields });
  const doc = setPage(res);
  if (!doc.getElementById('DropDownList_SearchType')) { // the session ran out: start over once
    await start();
    const again = await request({ op: 'post', method: 'POST', url: formAction(), fields: { ...formFields(), ...overrides, __EVENTTARGET: eventTarget, __EVENTARGUMENT: '', ...(click ? { 'ImageButton_View_SearchFor.x': '9', 'ImageButton_View_SearchFor.y': '9' } : {}) } });
    setPage(again);
  }
  return page.doc;
}

// ---------- the search form (mirrors SLTS's own) ----------
// SLTS's typing box: any text-like field (no type is text too) except the overlap checker's ten boxes
function searchBox(d) {
  const ok = (n) => {
    const type = (n.getAttribute('type') || 'text').toLowerCase();
    const name = n.getAttribute('name') || '';
    return n.tagName === 'TEXTAREA' || ['text', 'search'].includes(type) ? name && !/^TextBox_OperationNumber\d+$/i.test(name) : false;
  };
  const all = [...d.querySelectorAll('form input, form textarea')].filter(ok);
  return all.find((n) => n.closest('#TableRow_Form, #Panel_SearchFor')) || all.find((n) => /SearchFor|Word|Keyword|OperationOrPart/i.test(n.getAttribute('name'))) || null;
}
function currentType() { return page?.doc.getElementById('DropDownList_SearchType')?.querySelector('option[selected]')?.getAttribute('value') || ''; }
function renderForm() {
  const d = page.doc;
  const types = [...(d.getElementById('DropDownList_SearchType')?.querySelectorAll('option') || [])]
    .map((o) => ({ value: o.getAttribute('value'), label: clean(o.textContent), on: o.hasAttribute('selected') }));
  $('l-type').replaceChildren(...types.map((t) => el('option', { value: t.value, selected: t.on ? 'selected' : null, text: t.label })));
  const pick = d.getElementById('DropDownList_SearchFor');
  const text = searchBox(d);
  if (!text && !pick) {
    const names = [...d.querySelectorAll('form input, form select, form textarea')].map((n) => `${n.getAttribute('name')}:${n.getAttribute('type') || n.tagName.toLowerCase()}`).filter((x) => !/^(__|hidden_)/.test(x));
    api.log(`labor: no search box found for ${currentType()} (fields: ${names.join(', ').slice(0, 600)})`);
  }
  const label = clean(d.getElementById('Label_SearchFor')?.textContent);
  if (pick) {
    const opts = [...pick.querySelectorAll('option')];
    $('l-pick').replaceChildren(...opts.map((o) => el('option', { value: o.getAttribute('value') ?? o.textContent, text: clean(o.textContent),
      selected: o.hasAttribute('selected') ? 'selected' : null })));
    if (saved.pick && opts.some((o) => (o.getAttribute('value') ?? o.textContent) === saved.pick)) $('l-pick').value = saved.pick;
    $('l-pick').title = label || 'Choose';
  }
  const wantsText = /^(Word|Part|Operation)$/i.test(currentType());
  $('l-pick').hidden = !pick;
  $('l-q').hidden = !text && (pick || !wantsText);
  if (!text && !$('l-q').hidden) { $('l-q').dataset.name = 'TextBox_OperationOrPartNumber'; $('l-q').placeholder = label ? `${label}…` : 'Search…'; }
  if (text) {
    $('l-q').dataset.name = text.getAttribute('name');
    $('l-q').placeholder = label ? `${label}…` : 'Search…';
    if (text.getAttribute('value') && !$('l-q').value) $('l-q').value = text.getAttribute('value');
  }
}

async function changeType(type) {
  if (busy) return;
  busy = true;
  showState(el('div', { class: 'spinner' }), 'Opening SLTS…');
  try {
    await postBack({ DropDownList_SearchType: type }, { eventTarget: 'DropDownList_SearchType' });
    saved.type = type; saved.pick = ''; save();
    renderForm();
    hint();
  } catch (e) { failed(e, () => changeType(type)); } finally { busy = false; }
}

async function search() {
  if (busy) return;
  const type = $('l-type').value;
  const q = $('l-q').value.trim();
  if (!$('l-q').hidden && !q) { $('l-q').focus(); return; }
  busy = true;
  showState(el('div', { class: 'spinner' }), 'Asking SLTS…');
  try {
    if (currentType() !== type) { await postBack({ DropDownList_SearchType: type }, { eventTarget: 'DropDownList_SearchType' }); renderForm(); }
    const over = { DropDownList_SearchType: type };
    if (!$('l-pick').hidden) over.DropDownList_SearchFor = $('l-pick').value;
    if (!$('l-q').hidden) over[$('l-q').dataset.name || 'TextBox_OperationOrPartNumber'] = q;
    const doc = await postBack(over, { click: true });
    saved.type = type; saved.pick = $('l-pick').hidden ? '' : $('l-pick').value; saved.q = $('l-q').hidden ? '' : q; save();
    renderForm();
    showResults(doc, q);
  } catch (e) { failed(e, search); } finally { busy = false; }
}

// ---------- reading SLTS's report ----------
// SLTS's report is one long table. Each operation starts with a row holding <a name="CODE"> and its
// title; then come its description, notes, overlaps and (when it has them) one row per option
// ("2.0L GTDI", "...FROM CONDENSER"). "Combinations" and "Supplements" rows start add-on operations.
const isBase = (code) => /^[0-9A-Z]*\d[A-Z]$/.test(code); // 19700A, 19700D: base operations
function codeOf(td) { return clean(td?.textContent).replace(/\s+/g, '').toUpperCase(); }
function cartOf(td) {
  const a = td?.querySelector('a[href*="discardPartList"]');
  const m = a && a.getAttribute('href').match(/discardPartList\(([\s\S]*)\)/);
  if (!m) return null;
  const args = [...m[1].matchAll(/'((?:[^'\\]|\\.)*)'/g)].map((x) => x[1].replace(/&quot;/g, '"').replace(/&amp;/g, '&'));
  const [opNumBase, opNum, partNo, qty, desc, qual] = args;
  return { opNumBase: opNumBase || '', opNum: opNum || '', partNo: partNo || '', qty: qty || '', desc: desc || '', qual: qual || '' };
}
function parseOverlaps(row) {
  return [...row.querySelectorAll('token')].map((t) => clean(t.textContent)).filter(Boolean);
}
// "When claiming 19700A4: Do not use with; 6007A1, 6007A34*, …" → rules
function overlapRules(lines) {
  const rules = [];
  for (const line of lines) {
    const m = line.match(/When claiming\s+([^:]+):\s*Do not use with;?\s*(.+)$/i);
    if (!m) continue;
    let list = m[2], cond = '';
    const i = list.search(/\s+For vehicles/i);
    if (i >= 0) { cond = list.slice(i).trim(); list = list.slice(0, i); }
    const not = list.split(',').map((x) => x.trim().replace(/\.+$/, '').toUpperCase()).filter((x) => /^[0-9A-Z*]+$/.test(x));
    rules.push({ subject: m[1].trim().toUpperCase(), not, cond });
  }
  return rules;
}
const matches = (pattern, code) => (pattern.endsWith('*') ? code.startsWith(pattern.slice(0, -1)) : code === pattern);

function parseReport(doc) {
  const xml = doc.getElementById('TableRow_XmlContent');
  if (!xml) return null;
  const outer = xml.querySelector('table[border="1"]');
  const inner = outer?.querySelector('table');
  if (!inner) return null;
  const title = clean(xml.querySelector('td.copysubtitle')?.textContent);
  const ops = [];
  let cur = null, base = null, mode = 'base', suppParent = null, group = '';
  for (const row of inner.rows) {
    const tds = row.cells;
    if (tds.length < 2) continue;
    const text = clean(row.textContent);
    if (!text || /^Operation Description\s+Operation Number\s+Time$/i.test(text)) continue;
    const head = row.querySelector('a[name]');
    if (head) {
      const code = head.getAttribute('name').toUpperCase();
      const b = tds[1]?.querySelector('b');
      const parts = [...(b?.querySelectorAll('a[href*="partlink"]') || [])].map((a) => clean(a.textContent));
      const name = clean(b?.textContent).replace(/\(\s*[^)]*\)\s*/g, (m) => (parts.some((p) => m.includes(p)) ? '' : m)).replace(/\s+-\s+/, ' - ').trim();
      let kind = 'base', parent = null;
      if (isBase(code)) { mode = 'base'; suppParent = null; }
      else if (mode === 'supp' && suppParent && code.startsWith(suppParent.code)) { kind = 'supp'; parent = suppParent; }
      else if (mode === 'supp') { mode = 'combo'; }
      if (kind !== 'supp' && mode === 'combo' && base) { kind = 'combo'; parent = base; }
      cur = { code, name: name || code, parts, kind, parentCode: parent?.code || '', group, desc: [], notes: [], overlaps: [], options: [] };
      if (kind === 'base') base = cur;
      ops.push(cur);
      const optCode = codeOf(tds[3]);
      if (optCode) cur.options.push({ code: optCode, qual: '', time: clean(tds[5]?.textContent), cart: cartOf(tds[3]) });
      continue;
    }
    if (/^Combinations/i.test(text) && row.querySelector('b u, u b')) { mode = 'combo'; continue; }
    if (/^Supplements\b/i.test(text) && text.length < 40) { mode = 'supp'; suppParent = cur; continue; }
    if (row.querySelector('token')) { if (cur) cur.overlaps.push(...parseOverlaps(row)); continue; }
    if (tds[1]?.getAttribute('align') === 'right' && codeOf(tds[3])) {
      if (cur) cur.options.push({ code: codeOf(tds[3]), qual: clean(tds[1].textContent), time: clean(tds[5]?.textContent), cart: cartOf(tds[3]) });
      continue;
    }
    const blue = row.querySelector('font[color] a, a.hyperlink');
    if (blue && cur) {
      const href = blue.getAttribute('href') || '';
      const url = (href.match(/window\.open\('([^']+)'/) || [])[1] || '';
      cur.notes.push({ text: clean(blue.textContent), url });
      continue;
    }
    const descTd = tds[1];
    const t = clean(descTd?.textContent);
    if (!t) continue;
    if (cur) cur.desc.push(t);
    else group = t;
  }
  for (const op of ops) op.rules = overlapRules(op.overlaps);
  return { title, ops };
}

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
    const hit = document.querySelector(`[data-op="${CSS.escape(want)}"]`) || document.querySelector(`[data-opt="${CSS.escape(want)}"]`);
    if (hit) { hit.scrollIntoView({ block: 'center' }); hit.classList.add('l-flash'); }
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

// ---------- the tray (kept per vehicle) ----------
function toggleTray(o, opt) {
  const i = saved.tray.findIndex((t) => t.code === opt.code && t.qual === opt.qual);
  if (i >= 0) saved.tray.splice(i, 1);
  else {
    const base = o.parentCode ? lastOps.find((x) => x.code === o.parentCode) : null;
    saved.tray.push({ code: opt.code, qual: opt.qual, time: opt.time, name: o.name, kind: o.kind, base: base?.code || '',
      baseName: base?.name || '', cart: opt.cart, rules: o.rules || [] });
  }
  save();
  renderTray();
}

function trayWarnings() {
  const out = [];
  const codes = saved.tray.map((t) => t.code);
  saved.tray.forEach((t) => {
    for (const r of t.rules || []) {
      if (!matches(r.subject, t.code)) continue;
      for (const other of codes) {
        if (other === t.code) continue;
        if (r.not.some((p) => matches(p, other))) {
          const pair = [t.code, other].sort();
          out.push(`${pair[0]} and ${pair[1]} overlap: Ford says not to claim them together${r.cond ? ` (${r.cond})` : ''}.`);
        }
      }
    }
    if (t.kind === 'combo' && t.base && !saved.tray.some((x) => x.kind === 'base' && x.code.startsWith(t.base))) out.push(`${t.code} is a combination: also claim its base operation ${t.base}${t.baseName ? ` (${t.baseName})` : ''}.`);
  });
  return [...new Set(out)];
}

function renderTray() {
  const tray = saved.tray;
  $('tray-count').textContent = String(tray.length);
  let w = 0, actual = 0;
  for (const t of tray) { const n = num(t.time); if (n == null) actual++; else w += n; }
  const box = $('l-tray');
  if (!tray.length) {
    box.replaceChildren(el('h2', { text: 'Selected operations' }), el('div', { class: 'l-empty', text: 'Press + Add on an option to add it up here.' }));
    return;
  }
  const warns = trayWarnings();
  fill(box,
    el('h2', {}, 'Selected operations', el('span', { class: 'l-grow' }), el('button', { class: 'ghost small', text: 'Clear', onclick: () => { saved.tray = []; save(); renderTray(); refreshAdds(); } })),
    el('div', { class: 'l-tray-list' }, ...tray.map((t, i) => el('div', { class: 'l-ti' },
      el('div', { class: 'l-ti-top' }, el('span', { class: 'l-code', text: t.code, title: 'Click to copy', onclick: () => copy(t.code) }),
        el('span', { class: 'l-grow' }), el('span', { class: 'n', text: num(t.time) == null ? (t.time || '—') : hrs(num(t.time)) }),
        el('button', { class: 'l-x', title: 'Remove', text: '✕', onclick: () => { saved.tray.splice(i, 1); save(); renderTray(); refreshAdds(); } })),
      el('div', { class: 'l-ti-name', text: [t.name, t.qual].filter(Boolean).join(' · ') })))),
    el('div', { class: 'l-totals' },
      el('div', {}, el('span', { text: 'Warranty' }), el('b', { text: `${hrs(w)} hrs` })),
      el('div', { class: 'cp' }, el('span', { text: `Customer pay (×${CUSTOMER_PAY})` }), el('b', { text: `${hrs(w * CUSTOMER_PAY)} hrs` })),
      actual ? el('div', { class: 'l-actual', text: `${actual} operation${actual === 1 ? '' : 's'} without a set time (actual time) not included.` }) : null),
    warns.length ? el('div', { class: 'l-warns' }, ...warns.map((x) => el('div', { text: x }))) : null,
    el('div', { class: 'l-tray-actions' },
      el('button', { class: 'ghost small', text: 'Copy op codes', onclick: () => copy(tray.map((t) => t.code).join(', '), 'op codes') }),
      el('button', { class: 'ghost small', text: 'One-time use parts', onclick: () => showParts(tray.map((t) => ({ op: { name: t.name, code: t.code }, opt: { code: t.code, qual: t.qual, cart: t.cart } }))) }),
      el('button', { class: 'primary small', text: 'Print', onclick: printTray })));
}
function paintAdd(btn) {
  const on = saved.tray.some((t) => `${t.code}|${t.qual}` === btn.dataset.add);
  btn.className = on ? 'ghost small added' : 'primary small';
  btn.textContent = on ? '✓ Added' : '+ Add';
  btn.title = on ? 'Take it out of your list' : 'Add to your list';
}
function refreshAdds() { document.querySelectorAll('button[data-add]').forEach(paintAdd); }

// ---------- one-time use parts (SLTS's "Required Parts" list for an operation) ----------
const partsCache = new Map();
async function partsFor(opt) {
  const c = opt.cart;
  if (!c) return { rows: [], note: 'No one-time use parts listed.' };
  const key = `${c.opNum}|${c.qual}`;
  if (partsCache.has(key)) return partsCache.get(key);
  let out;
  try { if (!page) await start(); } catch { /* the request below reports it */ }
  const f = page ? formFields() : {};
  const q = new URLSearchParams({ strVin: f.hidden_Vin || '', yearSelected: f.hidden_Year || '', modelSelected: f.hidden_Model || '',
    market: f.hidden_market || '', market_orgi: f.hidden_market_orgi || '', country: f.hidden_country || '', language: f.hidden_language || '',
    opNumBase: c.opNumBase, opNum: c.opNum, disPrtNo: c.partNo, disPrtQty: c.qty, disPrtDesc: c.desc, qualDesc: c.qual, filename: f.hidden_FileName || '' });
  try {
    const res = await request({ op: 'get', url: `DiscardPartList_New.aspx?${q}` });
    out = readPartsPage(res.html);
  } catch (e) { out = { rows: [], note: `Couldn't get the full list from SLTS (${e.message}).` }; }
  // SLTS's link always names the first item; keep it if the page couldn't be read
  if (!out.rows.length && c.partNo) out.rows = [{ cells: [c.partNo, c.desc, c.qty] }], out.head = ['Part / item', 'Description', 'Qty'];
  partsCache.set(key, out);
  return out;
}
// The parts page is a plain table; read whatever columns it has
function readPartsPage(html) {
  const doc = parse(html);
  let best = null;
  for (const t of doc.querySelectorAll('table')) {
    if (t.querySelector('table')) continue; // innermost tables only
    const rows = [...t.rows].map((r) => [...r.cells].map((c) => clean(c.textContent))).filter((r) => r.some(Boolean));
    if (rows.length >= 2 && rows[0].length >= 2 && (!best || rows.length > best.length)) best = rows;
  }
  if (!best) return { rows: [], note: clean(doc.body?.textContent).slice(0, 200) };
  const [head, ...rest] = best;
  return { head, rows: rest.filter((r) => r.length >= 2 && r.join('') !== head.join('')).map((cells) => ({ cells })) };
}

async function partsBlock(list) {
  const blocks = [];
  for (const { op, opt } of list) {
    const r = await partsFor(opt);
    blocks.push(el('div', { class: 'l-parts' },
      el('div', { class: 'l-parts-head' }, el('span', { class: 'l-code', text: opt.code }), el('span', { text: [op.name, opt.qual].filter(Boolean).join(' · ') })),
      r.rows.length ? el('table', { class: 'l-ptable' },
        r.head ? el('thead', {}, el('tr', {}, ...r.head.map((h) => el('th', { text: h })))) : null,
        el('tbody', {}, ...r.rows.map((row) => el('tr', {}, ...row.cells.map((c) => el('td', { text: c }))))))
        : el('div', { class: 'muted', text: r.note || 'No one-time use parts listed.' })));
  }
  return blocks;
}

async function showParts(list) {
  $('l-modal-title').textContent = list.length === 1 ? `One-time use parts · ${list[0].opt.code}` : 'One-time use parts · selected operations';
  $('l-modal-body').replaceChildren(el('div', { class: 'o-msg' }, el('div', { class: 'spinner' }), 'Asking SLTS…'));
  $('l-modal').hidden = false;
  const blocks = await partsBlock(list);
  $('l-modal-body').replaceChildren(...blocks);
  $('l-modal-print').onclick = () => printBlocks($('l-modal-title').textContent, blocks.map((b) => b.cloneNode(true)));
}
$('l-modal-close').onclick = () => { $('l-modal').hidden = true; };
$('l-modal').addEventListener('click', (e) => { if (e.target === $('l-modal')) $('l-modal').hidden = true; });
document.addEventListener('keydown', (e) => { if (e.key === 'Escape') $('l-modal').hidden = true; });

// ---------- printing ----------
let vehicleLine = '';
function printBlocks(title, blocks, extra = []) {
  fill($('l-print'), el('h1', { text: title }), vehicleLine ? el('div', { class: 'l-print-sub', text: vehicleLine }) : null, ...extra, ...blocks);
  document.body.classList.add('printing');
  setTimeout(() => { window.print(); document.body.classList.remove('printing'); }, 50);
}
async function printTray() {
  const tray = saved.tray;
  let w = 0;
  for (const t of tray) { const n = num(t.time); if (n != null) w += n; }
  const table = el('table', { class: 'l-ptable' },
    el('thead', {}, el('tr', {}, el('th', { text: 'Op code' }), el('th', { text: 'Operation' }), el('th', { text: 'Warranty' }), el('th', { text: 'Customer pay' }))),
    el('tbody', {}, ...tray.map((t) => el('tr', {}, el('td', { text: t.code }), el('td', { text: [t.name, t.qual].filter(Boolean).join(' · ') }),
      el('td', { text: num(t.time) == null ? (t.time || '—') : hrs(num(t.time)) }), el('td', { text: num(t.time) == null ? '—' : hrs(num(t.time) * CUSTOMER_PAY) })))),
    el('tfoot', {}, el('tr', {}, el('td', { text: 'Total' }), el('td'), el('td', { text: hrs(w) }), el('td', { text: hrs(w * CUSTOMER_PAY) }))));
  toast('Getting one-time use parts…');
  const parts = await partsBlock(tray.map((t) => ({ op: { name: t.name }, opt: { code: t.code, qual: t.qual, cart: t.cart } })));
  const warns = trayWarnings();
  printBlocks('Labor operations', [el('h2', { text: 'One-time use parts' }), ...parts],
    [table, ...(warns.length ? [el('div', { class: 'l-warns' }, ...warns.map((x) => el('div', { text: x })))] : [])]);
}

// ---------- start ----------
$('l-type').addEventListener('change', () => changeType($('l-type').value));
$('l-form').addEventListener('submit', (e) => { e.preventDefault(); search(); });
$('l-pick').addEventListener('change', () => search());
$('btn-tray').onclick = () => { $('l-tray').scrollIntoView({ behavior: 'smooth' }); $('l-tray').classList.add('l-flash'); setTimeout(() => $('l-tray').classList.remove('l-flash'), 1200); };

async function init() {
  const info = await api.init().catch(() => null);
  if (info?.state && typeof info.state === 'object') saved = { tray: [], type: '', pick: '', q: '', ...info.state };
  if (!Array.isArray(saved.tray)) saved.tray = [];
  vehicleLine = [info?.ymm, info?.vin, info?.ro ? `RO ${info.ro}` : ''].filter(Boolean).join(' · ');
  $('l-sub').textContent = vehicleLine ? `Ford SLTS · ${vehicleLine}` : 'Ford SLTS';
  renderTray();
  showState(el('div', { class: 'spinner' }), 'Opening SLTS…');
  busy = true;
  try {
    await start();
    if (saved.type && saved.type !== currentType()) await postBack({ DropDownList_SearchType: saved.type }, { eventTarget: 'DropDownList_SearchType' });
    renderForm();
    if (saved.q) $('l-q').value = saved.q;
    hint();
  } catch (e) { failed(e, init); } finally { busy = false; }
}
init();