// Xtime View: the app's own page for this RO in Xtime (a trial, next to Xtime's own page).
// Shows the RO's stage, flags, vehicle and people, and every line: approval, done, labor type, hours, prices,
// concern / cause / correction, notes and parts. Read from Xtime through the signed-in Xtime page (the same
// address Xtime's RO page reads), every minute while it's showing. No customer details are read.
// The only change it makes in Xtime is a line's Done box, exactly like Xtime's Done button.
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
  for (const c of kids) if (c != null && c !== false && c !== '') n.append(c);
  return n;
}
function toast(text) {
  const t = $('o-toast');
  t.textContent = text; t.hidden = false;
  clearTimeout(toast.timer);
  toast.timer = setTimeout(() => { t.hidden = true; }, 1600);
}
const copy = (text, what) => { api.copy(String(text)); toast(`${what || 'Copied'}: ${text}`); };

// Xtime's stages (its ROModes list)
const MODES = { 9: 'Pre-RO', 2: 'Dispatch', 3: 'Being Inspected', 4: 'Pending Approval', 6: 'Being Repaired', 7: 'Review', 100: 'Closed' };
// Lines Xtime adds to every RO for the inspection (multipoint, green tires / battery / brakes): grouped at the bottom
const INSPECTION = /^(Q99PX|GTIRE|GBATT|GBK)\b/i;

const money = (n) => (n == null ? '' : `$${Number(n).toFixed(2)}`);
const hrs = (n) => (n == null ? '' : `${Number(n).toFixed(1)} h`);
// Xtime's times have no zone: they're the store's local time
function when(t, { date = true } = {}) {
  if (!t) return '';
  const d = t instanceof Date ? t : new Date(String(t).replace(/Z$/, ''));
  if (Number.isNaN(d.getTime())) return String(t);
  const time = d.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });
  if (!date) return time;
  const today = new Date();
  if (d.toDateString() === today.toDateString()) return `today ${time}`;
  const y = new Date(today); y.setDate(today.getDate() - 1);
  if (d.toDateString() === y.toDateString()) return `yesterday ${time}`;
  return `${d.toLocaleDateString([], { weekday: 'short', month: 'short', day: 'numeric' })} ${time}`;
}
function promised(v) {
  if (!v.promisedDate) return '';
  const [m, d, y] = v.promisedDate.split('/').map(Number);
  const [hh = 0, mm = 0] = String(v.promisedTime || '').split(':').map(Number);
  if (!y) return `${v.promisedDate} ${v.promisedTime || ''}`.trim();
  return when(new Date(y, m - 1, d, hh, mm));
}

let info = {};
let view = null;
let loading = false;
let open = new Set();      // line ids shown expanded
let showInspection = false;

function status(l) {
  if (l.declined) return ['declined', 'Declined'];
  if (l.approved) return ['approved', 'Approved'];
  return ['undecided', 'Undecided'];
}

function lineCard(l, children = []) {
  // a customer concern (primary line) has no approve / decline: only recommendations do
  const [cls, label] = l.primary ? ['concern', ''] : status(l);
  const hasDetail = l.concern || l.cause || (l.correction && l.correction.toUpperCase() !== l.name.replace(/^[^:]*:\s*/, '').toUpperCase())
    || l.techNote || l.advisorNote || l.managerNote || l.partsNote || l.parts_.length || l.subLines.length;
  const isOpen = open.has(l.id);
  const done = el('label', { class: `x-done${l.completed ? ' on' : ''}`, title: l.declined ? "A declined line can't be marked done" : 'Mark this line done in Xtime (like Xtime\'s Done button)',
    onclick: (e) => e.stopPropagation() },
  el('input', { type: 'checkbox', checked: l.completed ? '' : null, disabled: l.declined ? '' : null,
    onchange: async (e) => {
      const box = e.target;
      box.disabled = true;
      let r;
      try { r = await api.xviewDone(l.id, box.checked); } catch (err) { r = { error: true, message: String(err?.message || err) }; }
      if (r?.error) { box.checked = !box.checked; toast(r.message || "Couldn't update the line in Xtime"); }
      else toast(box.checked ? `Line ${l.dms || ''} marked done` : `Line ${l.dms || ''} not done`);
      box.disabled = false;
      load();
    } }), 'Done');
  const head = el('div', { class: 'x-line-head', onclick: () => { if (open.has(l.id)) open.delete(l.id); else open.add(l.id); draw(); } },
    el('span', { class: 'x-dms', text: l.dms || '·' }),
    el('div', { class: 'x-name' },
      el('div', { class: 'x-title', text: l.name || l.opCode || 'Line' }),
      l.concern && !isOpen ? el('div', { class: 'x-peek', text: l.concern }) : null),
    label ? el('span', { class: `x-status ${cls}`, text: label }) : null,
    l.payType ? el('span', { class: 'x-pay', title: 'Labor type', text: l.payType }) : null,
    el('span', { class: 'x-num', title: 'Hours', text: hrs(l.hours) }),
    el('span', { class: 'x-num x-total', title: `Labor ${money(l.labor)} · parts ${money(l.parts)}`, text: money(l.total) }),
    done,
    l.primary ? null : deleteButton(l),
    el('span', { class: 'x-tw', text: hasDetail ? (isOpen ? '▾' : '▸') : '' }));
  const card = el('div', { class: `x-line ${cls}${l.completed ? ' done' : ''}${isOpen ? ' open' : ''}` }, head);
  if (isOpen) {
    const body = el('div', { class: 'x-line-body' });
    const field = (k, v, extra = '') => (v ? el('div', { class: `x-field ${extra}` }, el('div', { class: 'k', text: k }), el('div', { class: 'v', text: v })) : null);
    body.append(...[
      field('Concern', l.concern),
      field('Cause', l.cause),
      field('Correction', l.correction),
      field('Technician note', l.techNote, 'note'),
      field('Advisor note', l.advisorNote, 'note'),
      field('Manager note', l.managerNote, 'note'),
      field('Parts note', l.partsNote, 'note'),
    ].filter(Boolean));
    if (l.subLines.length) body.append(el('div', { class: 'x-field' }, el('div', { class: 'k', text: 'Sub-operations' }), el('div', { class: 'v', text: l.subLines.join(' · ') })));
    if (l.parts_.length) {
      body.append(el('div', { class: 'x-field' }, el('div', { class: 'k', text: 'Parts' }),
        el('table', { class: 'x-parts' },
          el('tr', {}, ...['Part #', 'Description', 'Qty', 'Status', 'Price'].map((h) => el('th', { text: h }))),
          ...l.parts_.map((p) => el('tr', {},
            el('td', { class: 'code', title: 'Click to copy', onclick: () => p.number && copy(p.number, 'Part number'), text: p.number || '—' }),
            el('td', { text: p.description || '' }),
            el('td', { text: p.quantity != null ? String(p.quantity) : '' }),
            el('td', { text: [p.status, p.pickedUp ? 'picked up' : ''].filter(Boolean).join(' · ') }),
            el('td', { text: money(p.price) }))))));
    } else if (l.partsNotNeeded) body.append(el('div', { class: 'x-meta', text: 'No parts needed' }));
    const meta = [
      l.dispatch && `Dispatch ${l.dispatch}`,
      l.estimate != null && l.estimate > 0 && `Estimated ${hrs(l.estimate)}`,
      l.labor != null && `Labor ${money(l.labor)}`, l.parts != null && `Parts ${money(l.parts)}`,
      l.techs.length && `Tech: ${l.techs.join(', ')}`,
      l.techStart && `Started ${when(l.techStart)}`, l.techEnd && `Finished ${when(l.techEnd)}`,
      l.images && `${l.images} picture${l.images === 1 ? '' : 's'} in Xtime`,
      l.addedBy && `Added by ${l.addedBy}`,
      l.partsPickedUp && 'Parts picked up',
    ].filter(Boolean);
    if (meta.length) body.append(el('div', { class: 'x-meta', text: meta.join(' · ') }));
    card.append(body);
  }
  // recommendations added under this concern
  if (children.length) card.append(el('div', { class: 'x-children' }, el('div', { class: 'x-children-label', text: `Recommended for ${l.dms || 'this concern'}` }), ...children.map((c) => lineCard(c))));
  return card;
}

function section(title, lines, { note = '', collapsed = false, toggle = null, children = new Map() } = {}) {
  if (!lines.length) return null;
  const sum = (k, list) => list.reduce((a, l) => a + (Number(l[k]) || 0), 0);
  const recs = [...lines.filter((l) => !l.primary), ...lines.flatMap((l) => children.get(l.id) || [])];
  const approved = recs.filter((l) => l.approved && !l.declined);
  const h = el('h2', {}, title, el('span', { class: 'count', text: String(lines.length) }),
    approved.length ? el('span', { class: 'x-sum', text: `${approved.length} approved · ${hrs(sum('hours', approved))} · ${money(sum('total', approved))}` }) : null,
    toggle ? el('button', { class: 'ghost small x-sec-btn', text: collapsed ? 'Show' : 'Hide', onclick: toggle }) : null);
  return el('section', { class: 'o-section' }, h, note ? el('p', { class: 'muted x-note', text: note }) : null,
    collapsed ? null : el('div', { class: 'x-lines' }, ...lines.map((l) => lineCard(l, children.get(l.id) || []))));
}

function draw() {
  const v = view;
  if (!v) return;
  const ymm = [v.vehicle.year, v.vehicle.make, v.vehicle.model].filter(Boolean).join(' ');
  $('x-title').textContent = `RO ${v.ro}${v.tag ? ` · Tag ${v.tag}` : ''}`;
  $('x-sub').replaceChildren(el('span', { class: 'x-mode', text: MODES[v.mode] || `Stage ${v.mode}` }),
    v.modeSince ? ` since ${when(v.modeSince)}` : '', ymm ? ` · ${ymm}` : '',
    ...[v.waiter && 'Waiter', v.express && 'Express', v.internal && 'Internal', v.warranty && 'Warranty', v.cpo && 'CPO']
      .filter(Boolean).map((t) => el('span', { class: `x-kind ${t.toLowerCase()}`, text: t })));
  // Flags, as Xtime shows them (PA, CA, WA, OH and the store's own)
  const flags = [
    v.waitingParts && ['PA', 'Parts', 'wait'], v.waitingCustomer && ['CA', 'Customer Approval', 'wait'], v.waitingWarranty && ['WA', 'Warranty Approval', 'wait'],
    v.onHold && ['OH', 'On Hold', 'hold'], ...v.flags.map((f) => [f, f, 'plain']),
  ].filter(Boolean);
  const row = (k, val, opts = {}) => el('div', { class: 'o-row' }, el('div', { class: 'k', text: k }),
    opts.node || el('div', { class: `v${val ? '' : ' empty'}${opts.code ? ' code' : ''}`, title: val && opts.copy ? 'Click to copy' : null,
      onclick: val && opts.copy ? () => copy(val, k) : null, text: val || '—' }));
  const person = (n, num) => (n ? `${n}${num ? ` (${num})` : ''}` : '');
  const pDate = promised(v);
  const lines = v.lines;
  const isInsp = (l) => INSPECTION.test(l.name) || INSPECTION.test(l.opCode);
  const main = lines.filter((l) => !isInsp(l));
  const insp = lines.filter(isInsp);
  // Recommendations added under a concern: Xtime links them to it (ParentLineId), or they share its line letter
  const primaries = main.filter((l) => l.primary);
  const children = new Map();
  const parentOf = (r) => {
    // Xtime's own grouping: a line's GroupID is the concern it was added under
    if (r.groupId) { const g = primaries.find((x) => x.id === r.groupId); if (g) return g; }
    if (r.parentId != null) { const p = primaries.find((x) => x.id === r.parentId || (x.rolid != null && x.rolid === r.parentId)); if (p) return p; }
    if (r.dms) return primaries.find((x) => x.dms === r.dms) || null;
    return null;
  };
  const loose = [];
  for (const r of main.filter((l) => !l.primary)) {
    const p = parentOf(r);
    if (p) { if (!children.has(p.id)) children.set(p.id, []); children.get(p.id).push(r); } else loose.push(r);
  }
  const scroll = document.scrollingElement.scrollTop;
  $('o-body').replaceChildren(...[
    el('section', { class: 'o-section' }, el('div', { class: 'o-grid x-top' },
      el('div', { class: 'o-card' }, el('h3', { text: 'Service Details' }),
        row('Tag Number', v.tag, { copy: true }),
        row('Advisor', person(v.advisor, v.advisorNumber)),
        row('Technician', person(v.technician, v.technicianNumber)),
        row('Parts Employee', v.partsEmployee),
        row('Promise Date', pDate ? pDate.replace(/\s+\d{1,2}:\d{2}\s*[AP]M$/i, '') : ''),
        row('Promise Time', v.promisedTime ? when(new Date(2000, 0, 1, ...String(v.promisedTime).split(':').map(Number)), { date: false }) : ''),
        row('Flags', '', { node: flags.length ? el('div', { class: 'v x-flags' }, ...flags.map(([code, name, c]) => el('span', { class: `x-flag ${c}`, title: name, text: code === name ? name : `${code} · ${name}` })))
          : el('div', { class: 'v empty', text: 'None' }) })),
      el('div', { class: 'o-card' }, el('h3', { text: 'Vehicle Info' }),
        row('VIN', v.vehicle.vin, { copy: true, code: true }),
        row('Vehicle', ymm),
        row('Mileage', v.vehicle.mileage ? v.vehicle.mileage.toLocaleString() : '')))),
    el('div', { class: 'x-toolbar' }, el('button', { class: 'primary small', text: '＋ Add Service', title: 'Add a service line to this RO (Xtime\'s Add Service)', onclick: () => openAddService() })),
    section('Customer Initial Concerns', primaries, { children }),
    section('Additional Service Recommendations', loose),
    !main.length ? el('div', { class: 'state' }, el('p', { text: 'No lines on this RO yet.' })) : null,
    section('Inspection lines', insp, { collapsed: !showInspection, note: showInspection ? '' : 'Multipoint inspection and green tires / battery / brakes.',
      toggle: () => { showInspection = !showInspection; draw(); } }),
    inspBox, // the inspection form (kept as it is when the RO above refreshes)
  ].filter(Boolean));
  document.scrollingElement.scrollTop = scroll;
  api.save({ open: [...open], showInspection }); // what's expanded, kept with the tab
}

// ---------- the inspection form: read and changed exactly like Xtime's (Green / Yellow / Red, values, sub-items) ----------
const inspBox = el('section', { class: 'o-section x-insp' });
let form = null;          // { id, name, forms, categories: [{ id, name, items: [{ id, name, color, value, required, showValue, subItems… }] }] }
let formError = '';
let formBusy = false;
const parseSub = (t) => String(t || '').split(',').map((x) => x.trim()).filter(Boolean);

async function loadInspection({ quiet = false } = {}) {
  // not while you're typing a value or a change is on its way
  if (formBusy || (quiet && inspBox.contains(document.activeElement) && document.activeElement.tagName === 'INPUT')) return;
  if (form?.categories.some((c) => c.items.some((i) => i.saving))) return;
  formBusy = true;
  if (!form && !quiet) drawInspection('Reading the inspection form from Xtime…');
  let r;
  try { r = await api.xinspect({ type: 'get' }); } catch (e) { r = { error: true, message: String(e?.message || e) }; }
  formBusy = false;
  if (r?.error) { formError = r.message || "Couldn't read the inspection form"; if (!form || !quiet) drawInspection(); return; }
  formError = '';
  form = r.form;
  drawInspection();
}

// Xtime's saveItem, step for step: the same button twice clears it; a sub-item adds to the value (once) and keeps the
// "worse" colour (green < yellow < red); "Clear" empties the value
async function saveItem(item, color, value) {
  if (item.saving) return;
  const isSub = value !== undefined && value !== 'Clear';
  if (isSub) {
    const have = item.value ? item.value.split(',').map((v) => v.trim()) : [];
    if (have.includes(value)) return;
  }
  if (color === item.color && value === undefined) color = 'Clear';
  else if (color === undefined && value === undefined) color = item.color;
  else if (value !== undefined) {
    if (item.color === 'Red' && (color === 'Green' || color === 'Yellow')) color = 'Red';
    else if (item.color === 'Yellow' && color === 'Green') color = 'Yellow';
  }
  if (value === 'Clear') value = '';
  else if (value === undefined) value = item.value;
  else if (value.trim().length) value = item.value && item.value.trim().length ? `${item.value}, ${value}` : value;
  item.saving = true;
  drawInspection();
  let r;
  try { r = await api.xinspect({ type: 'save', iiid: item.id, color, value }); } catch (e) { r = { error: true, message: String(e?.message || e) }; }
  item.saving = false;
  if (r?.error) toast(r.message || "Couldn't save to Xtime");
  else {
    item.color = color || item.color; item.value = value ?? item.value; item.answered = true;
    if (color === 'Red' && r.red === 'service added') { toast('Saved · Xtime added its service'); setTimeout(() => load(), 1500); }
  }
  drawInspection();
}

function itemRow(item) {
  const c = item.color;
  const dot = (col) => el('button', { class: `x-dot ${col.toLowerCase()}${c === col ? ' on' : ''}`, title: col, disabled: item.saving ? '' : null,
    onclick: () => saveItem(item, col) });
  const chosen = new Set(String(item.value || '').split(',').map((v) => v.trim()).filter(Boolean));
  const sub = (label, col) => el('button', { class: `x-sub ${col ? col.toLowerCase() : ''}${chosen.has(label) ? ' on' : ''}`, disabled: item.saving ? '' : null,
    title: col ? `${label} (${col})` : label, onclick: () => saveItem(item, col || item.color, label), text: label });
  const subs = [...parseSub(item.subItems).map((x) => sub(x, '')), ...parseSub(item.subRed).map((x) => sub(x, 'Red')),
    ...parseSub(item.subYellow).map((x) => sub(x, 'Yellow')), ...parseSub(item.subGreen).map((x) => sub(x, 'Green'))];
  const input = item.showValue ? el('input', { class: `x-val${item.required && !item.value ? ' need' : ''}`, type: 'text', maxlength: '255', value: item.value || '',
    disabled: item.saving ? '' : null,
    onkeydown: (e) => { if (e.key === 'Enter') e.target.blur(); },
    onblur: (e) => { if (e.target.value !== (item.value || '')) { item.value = e.target.value; saveItem(item); } } }) : null;
  return el('div', { class: `x-item${item.saving ? ' saving' : ''}${c && c !== 'Clear' ? ` c-${c.toLowerCase()}` : ''}` },
    el('div', { class: 'x-dots' }, dot('Green'), dot('Yellow'), dot('Red')),
    el('div', { class: 'x-item-main' },
      el('div', { class: 'x-item-name' }, item.name, item.required ? el('span', { class: 'x-req', title: 'Required', text: ' *' }) : null,
        (c === 'Yellow' || c === 'Red') && item.services?.length ? el('button', { class: 'ghost small x-item-add', text: '🔧 Add Service',
          title: 'Add a service for this item', onclick: () => openAddService({ item }) }) : null),
      subs.length || input ? el('div', { class: 'x-item-vals' }, input, ...subs,
        item.showValue ? el('button', { class: 'x-sub clear', disabled: item.saving ? '' : null, onclick: () => saveItem(item, 'Clear', 'Clear'), text: 'Clear' }) : null) : null));
}

function drawInspection(message = '') {
  const head = el('h2', {}, 'Inspection Form');
  if (form?.forms?.length > 1) {
    const sel = el('select', { class: 'x-form-sel', title: "This RO's inspection form" },
      ...form.forms.map((f) => el('option', { value: String(f.id), selected: String(f.id) === String(form.id) ? '' : null, text: f.name })));
    const go = el('button', { class: 'ghost small', hidden: '', text: 'Switch form', onclick: async () => {
      const name = sel.options[sel.selectedIndex].text;
      go.disabled = true;
      let r;
      try { r = await api.xinspect({ type: 'switch', formId: Number(sel.value) }); } catch (e) { r = { error: true, message: String(e?.message || e) }; }
      if (r?.error) { toast(r.message || "Couldn't switch the form"); go.disabled = false; return; }
      toast(`Switched to ${name}`);
      form = null; loadInspection();
    } });
    sel.addEventListener('change', () => { go.hidden = sel.value === String(form.id); });
    head.append(sel, go);
  } else if (form) head.append(el('span', { class: 'x-sum', text: form.name }));
  const kids = [head];
  if (message) kids.push(el('p', { class: 'muted', text: message }));
  else if (formError && !form) kids.push(el('div', { class: 'x-insp-error' }, el('p', { class: 'muted', text: formError }),
    el('button', { class: 'ghost small', text: 'Try again', onclick: () => loadInspection() })));
  else if (form) {
    const all = form.categories.flatMap((c) => c.items);
    const missing = all.filter((i) => i.required && !String(i.value || '').trim()).length;
    const counts = { Red: 0, Yellow: 0 };
    for (const i of all) if (counts[i.color] != null && i.answered) counts[i.color]++;
    kids.push(el('div', { class: 'x-insp-sum muted' },
      counts.Red ? el('span', { class: 'x-count red', text: `${counts.Red} red` }) : null,
      counts.Yellow ? el('span', { class: 'x-count yellow', text: `${counts.Yellow} yellow` }) : null,
      missing ? `${missing} required still empty` : 'All required items filled in',
      formError ? ` · couldn't refresh: ${formError}` : ''));
    for (const c of form.categories) kids.push(el('div', { class: 'x-cat' }, el('div', { class: 'x-cat-head', text: c.name }), ...c.items.map(itemRow)));
  }
  inspBox.replaceChildren(...kids);
}

// ---------- Delete Service (recommendations only; a click asks once more) ----------
function deleteButton(l) {
  const b = el('button', { class: 'x-del', title: 'Delete this service from the RO', text: '🗑' });
  let armed = null;
  b.addEventListener('click', async (e) => {
    e.stopPropagation();
    if (!armed) {
      b.textContent = 'Delete?'; b.classList.add('armed');
      armed = setTimeout(() => { armed = null; b.textContent = '🗑'; b.classList.remove('armed'); }, 3000);
      return;
    }
    clearTimeout(armed); armed = null;
    b.disabled = true; b.textContent = '…';
    let r;
    try { r = await api.xinspect({ type: 'deleteLine', id: l.id }); } catch (err) { r = { error: true, message: String(err?.message || err) }; }
    if (r?.error) { toast(r.message || "Couldn't delete the service"); b.disabled = false; b.textContent = '🗑'; b.classList.remove('armed'); return; }
    toast(`Deleted ${l.dms ? `line ${l.dms}` : 'the service'}`);
    load();
  });
  return b;
}

// ---------- Add Service: Xtime's Add Service window ----------
let catalog = null;
async function openAddService({ item = null } = {}) {
  if (!catalog) {
    toast('Loading the services…');
    let r;
    try { r = await api.xinspect({ type: 'catalog' }); } catch (e) { r = { error: true, message: String(e?.message || e) }; }
    if (r?.error) { toast(r.message || "Couldn't load the services"); return; }
    catalog = r.catalog;
  }
  const only = item ? new Set(item.services.map(String)) : null;
  const services = catalog.services.filter((s) => !only || only.has(String(s.sgid)));
  const defaultPay = catalog.payTypes.find((p) => p.isDefault && p.payTypeId === (catalog.internal ? 2 : 1)) || catalog.payTypes.find((p) => p.isDefault);
  const st = { service: null, color: item && (item.color === 'Yellow' || item.color === 'Red') ? item.color : (catalog.defaultColor || 'Red') };
  const back = el('div', { class: 'x-modal' });
  const close = () => { back.remove(); document.removeEventListener('keydown', onKey); };
  const onKey = (e) => { if (e.key === 'Escape') close(); };
  document.addEventListener('keydown', onKey);
  back.addEventListener('pointerdown', (e) => { if (e.target === back) close(); });
  const field = (label, input, req = false) => el('label', { class: 'x-f' }, el('span', {}, label, req ? el('b', { class: 'x-req', text: ' *' }) : null), input);
  const inp = (attrs = {}) => el('input', { type: 'text', ...attrs });
  const area = () => el('textarea', { rows: '2' });
  const find = inp({ placeholder: services.length > 12 ? `Search ${services.length} services — name or op code` : 'Search services', spellcheck: 'false' });
  const list = el('select', { size: '8', class: 'x-svc-list' });
  const fill = () => {
    const q = find.value.trim().toLowerCase();
    const hits = services.filter((s) => !q || `${s.name} ${s.opCode}`.toLowerCase().includes(q)).slice(0, 300);
    list.replaceChildren(...hits.map((s) => el('option', { value: String(s.sgcid), text: `${s.name}${s.opCode ? `  (${s.opCode})` : ''}${s.price ? `  $${s.price.toFixed(2)}` : ''}` })));
    if (st.service && hits.some((h) => h.sgcid === st.service.sgcid)) list.value = String(st.service.sgcid);
  };
  find.addEventListener('input', fill);
  const pay = el('select', {}, ...catalog.payTypes.map((p) => el('option', { value: String(p.id), selected: defaultPay && p.id === defaultPay.id ? '' : null, text: p.code })));
  const hours = inp({ inputmode: 'decimal', placeholder: '—' }), labor = inp({ inputmode: 'decimal', placeholder: '—' }), parts = inp({ inputmode: 'decimal', placeholder: '—' }), total = inp({ inputmode: 'decimal', placeholder: '—' });
  const details = area(), complaint = area(), cause = area(), correction = area(), note = area();
  list.addEventListener('change', () => {
    st.service = services.find((s) => String(s.sgcid) === list.value) || null;
    if (!st.service) return;
    // as Xtime: the service's own name and prices to start from
    details.value = st.service.name;
    if (!labor.value && st.service.labor) labor.value = st.service.labor.toFixed(2);
    if (!total.value && st.service.price) total.value = st.service.price.toFixed(2);
  });
  const colorBtn = (c, label) => el('button', { type: 'button', class: `x-cbtn ${c.toLowerCase()}${st.color === c ? ' on' : ''}`, title: label,
    onclick: (e) => { st.color = c; for (const b of e.target.parentElement.children) b.classList.toggle('on', b === e.target); colorNote.textContent = c === 'Red' ? 'Requires Immediate Attention' : 'May Require Future Attention'; } });
  const colorNote = el('span', { class: 'muted', text: st.color === 'Yellow' ? 'May Require Future Attention' : 'Requires Immediate Attention' });
  const err = el('div', { class: 'x-err' });
  const save = el('button', { class: 'primary', text: 'Add', onclick: async () => {
    err.textContent = '';
    if (!st.service) { err.textContent = 'Pick a service from the list.'; return; }
    if (!details.value.trim()) { err.textContent = 'Details are required.'; return; }
    save.disabled = true; save.textContent = 'Adding…';
    let r;
    try {
      r = await api.xinspect({ type: 'addLine', line: { sgcid: st.service.sgcid, sgid: st.service.sgid, color: st.color, payTypeId: pay.value,
        hours: hours.value.trim(), labor: labor.value.trim(), parts: parts.value.trim(), total: total.value.trim(),
        details: details.value.trim(), complaint: complaint.value.trim(), cause: cause.value.trim(), correction: correction.value.trim(), techNote: note.value.trim() } });
    } catch (e) { r = { error: true, message: String(e?.message || e) }; }
    save.disabled = false; save.textContent = 'Add';
    if (r?.error) { err.textContent = r.message || "Xtime didn't add the service"; return; }
    toast(`Added: ${st.service.name}`);
    close();
    load();
  } });
  back.append(el('div', { class: 'x-modal-box', role: 'dialog' },
    el('div', { class: 'x-modal-head' }, el('b', { text: item ? `Add Service · ${item.name}` : 'Add Service' }), el('button', { class: 'icon', text: '×', title: 'Close (Esc)', onclick: close })),
    el('div', { class: 'x-modal-body' },
      field('Service Name', el('div', { class: 'x-svc' }, find, list), true),
      el('div', { class: 'x-f x-colors' }, el('span', {}, 'Color'), el('div', {}, colorBtn('Yellow', 'May Require Future Attention'), colorBtn('Red', 'Requires Immediate Attention'), colorNote)),
      el('div', { class: 'x-grid4' }, field('Labor Type', pay, true), field('Hours', hours), field('Labor', labor), field('Parts', parts), field('Total', total)),
      field('Details', details, true), field('Concern / Service Request', complaint), field('Cause', cause), field('Correction', correction), field('Technician Note', note),
      el('p', { class: 'muted x-small', text: 'Empty hours or prices use the service\'s own, like Xtime.' })),
    el('div', { class: 'x-modal-foot' }, err, el('button', { class: 'ghost', text: 'Cancel', onclick: close }), save)));
  document.body.append(back);
  fill();
  find.focus();
}

async function load({ quiet = true } = {}) {
  if (loading) return;
  loading = true;
  $('btn-refresh').disabled = true;
  if (!view && !quiet) $('o-body').replaceChildren(el('div', { class: 'state' }, el('p', { text: 'Reading this RO from Xtime…' })));
  let res;
  try { res = await api.xview(); } catch (e) { res = { error: true, message: String(e?.message || e) }; }
  loading = false;
  $('btn-refresh').disabled = false;
  if (res?.error) {
    if (view) { $('x-updated').textContent = `Couldn't refresh: ${res.message || 'error'}`; return; }
    $('x-sub').textContent = info.ymm || '';
    $('o-body').replaceChildren(el('div', { class: 'state' },
      el('h2', { text: res.expired ? 'Sign in to Xtime' : "Couldn't read this RO" }),
      el('p', { text: res.expired ? 'Xtime needs you to sign in again (on the Xtime tab).' : res.message || '' }),
      el('div', { class: 'actions' },
        el('button', { class: 'primary', text: 'Try again', onclick: () => load({ quiet: false }) }),
        el('button', { class: 'ghost', text: "Xtime's page", onclick: () => api.openXtime() }))));
    return;
  }
  view = res.view;
  $('x-updated').textContent = `Updated ${new Date(res.at || Date.now()).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' })}`;
  draw();
}

$('btn-refresh').addEventListener('click', () => { load({ quiet: false }); loadInspection(); });
$('btn-xtime').addEventListener('click', () => api.openXtime());
// every minute while it's showing, and when it comes back into view
setInterval(() => { if (document.visibilityState === 'visible') { load(); loadInspection({ quiet: true }); } }, 60 * 1000);
document.addEventListener('visibilitychange', () => {
  if (document.visibilityState === 'visible' && view && Date.now() - (load.last || 0) > 20 * 1000) { load.last = Date.now(); load(); }
});

(async () => {
  info = (await api.init().catch(() => null)) || {};
  const s = info.state || {};
  open = new Set(Array.isArray(s.open) ? s.open : []);
  showInspection = !!s.showInspection;
  $('x-title').textContent = `RO ${info.ro || ''}`;
  load.last = Date.now();
  load({ quiet: false });
  loadInspection();
})();
