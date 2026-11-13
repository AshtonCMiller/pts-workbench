// Xtime View, the RO itself: the header (stage, waiter / express), Service Details, Vehicle Info, customer concerns with
// their recommendations under them, the other recommendations and the inspection lines; each line's approval, Done box,
// labor type, hours, prices, concern / cause / correction, notes and parts. Read from Xtime every minute while the tab
// is showing. Also starts the page, so it's loaded last (after common.js, inspection.js and service-editor.js).
'use strict';

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
    editButton(l),
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
