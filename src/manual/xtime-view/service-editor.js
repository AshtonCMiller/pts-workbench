// Xtime View, service lines: Xtime's Add Service window (also used to edit a line already on the RO) and Delete
// Service. Changes go to Xtime through the signed-in Xtime page, the same way Xtime's own windows send them.
'use strict';

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

// ---------- Edit Service: the Add Service window, filled in from the line ----------
function editButton(l) {
  return el('button', { class: 'x-del x-edit', title: 'Edit this service (service, color, labor type, hours, prices, notes)', text: '✎',
    onclick: (e) => { e.stopPropagation(); openAddService({ line: l }); } });
}

// ---------- Add Service: Xtime's Add Service window ----------
let catalog = null;
// line: a line already on the RO, to edit (the same window, filled in from it)
async function openAddService({ item = null, line = null } = {}) {
  if (!catalog) {
    toast('Loading the services…');
    let r;
    try { r = await api.xinspect({ type: 'catalog' }); } catch (e) { r = { error: true, message: String(e?.message || e) }; }
    if (r?.error) { toast(r.message || "Couldn't load the services"); return; }
    catalog = r.catalog;
  }
  const only = item ? new Set(item.services.map(String)) : null;
  const services = catalog.services.filter((s) => !only || only.has(String(s.sgid)));
  // editing: the line's own service, even when it isn't one the Add Service list offers (e.g. a customer concern)
  if (line?.sgcid && !services.some((s) => String(s.sgcid) === String(line.sgcid))) {
    services.unshift({ sgcid: line.sgcid, sgid: line.sgid, name: line.serviceName || line.name || 'This service', opCode: line.opCode || '', price: 0, labor: 0 });
  }
  const defaultPay = catalog.payTypes.find((p) => p.isDefault && p.payTypeId === (catalog.internal ? 2 : 1)) || catalog.payTypes.find((p) => p.isDefault);
  const st = { service: null, color: line ? line.color : item && (item.color === 'Yellow' || item.color === 'Red') ? item.color : (catalog.defaultColor || 'Red') };
  const back = el('div', { class: 'x-modal' });
  const close = () => { back.remove(); document.removeEventListener('keydown', onKey); };
  const onKey = (e) => { if (e.key === 'Escape') close(); };
  document.addEventListener('keydown', onKey);
  back.addEventListener('pointerdown', (e) => { if (e.target === back) close(); });
  const field = (label, input, req = false) => el('label', { class: 'x-f' }, el('span', {}, label, req ? el('b', { class: 'x-req', text: ' *' }) : null), input);
  const inp = (attrs = {}) => el('input', { type: 'text', ...attrs });
  const area = (rows = 2) => el('textarea', { rows: String(rows) });
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
  const details = area(), complaint = area(), cause = area(), correction = area(), note = area(8); // the technician note often holds a parts list: room for one
  note.classList.add('x-note');
  // Dispatch code and estimated duration: the store's default code and its time, like Xtime
  const codes = catalog.dispatchCodes || [];
  const defCode = codes.find((c) => c.isDefault) || codes[0] || null;
  const dispatch = el('select', {}, ...(codes.length ? codes : [{ code: '', description: 'none' }]).map((c) => el('option', {
    value: c.code, selected: defCode && c.code === defCode.code ? '' : null, text: c.description ? `${c.code} · ${c.description}` : c.code })));
  const duration = inp({ inputmode: 'decimal', placeholder: '—', value: defCode?.duration != null ? String(defCode.duration) : '' });
  if (line) { // start from the line as it is in Xtime
    st.service = services.find((s) => String(s.sgcid) === String(line.sgcid)) || null;
    if (line.payTypeId != null) pay.value = String(line.payTypeId);
    const n = (v) => (v == null ? '' : String(Number(v)));
    const m = (v) => (v == null ? '' : Number(v).toFixed(2));
    hours.value = n(line.hours); labor.value = m(line.labor); parts.value = m(line.parts); total.value = m(line.total);
    details.value = line.details || line.name || ''; complaint.value = line.concern || ''; cause.value = line.cause || '';
    correction.value = line.correction || ''; note.value = line.techNote || '';
    if (line.dispatch) {
      if (![...dispatch.options].some((o) => o.value === line.dispatch)) dispatch.append(el('option', { value: line.dispatch, text: line.dispatch }));
      dispatch.value = line.dispatch;
    }
    if (line.estimate != null && line.estimate > 0) duration.value = n(line.estimate);
  }
  dispatch.addEventListener('change', () => {
    const c = codes.find((x) => x.code === dispatch.value);
    if (c?.duration != null) duration.value = String(c.duration); // a code with its own time fills it in; otherwise the time stays
  });
  list.addEventListener('change', () => {
    st.service = services.find((s) => String(s.sgcid) === list.value) || null;
    if (!st.service) return;
    // the search box shows what was picked, so it's clear it took
    find.value = st.service.name;
    // Details, Concern / Service Request and Correction start as the service's name ("* Used Car Check" → "Used Car Check");
    // anything typed into them yourself is kept when another service is picked
    const plain = st.service.name.replace(/^[\s*•\-–—"“”']+|["“”']+$/g, '').replace(/\s+/g, ' ').trim() || st.service.name;
    for (const box of [details, complaint, correction]) {
      if (!box.value.trim() || box.value === st.auto) box.value = plain;
    }
    st.auto = plain;
    if (line) return; // editing: the line's hours and prices stay as they are
    if (!labor.value && st.service.labor) labor.value = st.service.labor.toFixed(2);
    if (!total.value && st.service.price) total.value = st.service.price.toFixed(2);
  });
  const colorBtn = (c, label) => el('button', { type: 'button', class: `x-cbtn ${c.toLowerCase()}${st.color === c ? ' on' : ''}`, title: label,
    onclick: (e) => { st.color = c; for (const b of e.target.parentElement.children) b.classList.toggle('on', b === e.target); colorNote.textContent = c === 'Red' ? 'Requires Immediate Attention' : 'May Require Future Attention'; } });
  const colorNote = el('span', { class: 'muted', text: st.color === 'Yellow' ? 'May Require Future Attention' : 'Requires Immediate Attention' });
  const err = el('div', { class: 'x-err' });
  const save = el('button', { class: 'primary', text: line ? 'Save' : 'Add', onclick: async () => {
    err.textContent = '';
    if (!st.service) { err.textContent = 'Pick a service from the list.'; return; }
    if (!details.value.trim()) { err.textContent = 'Details are required.'; return; }
    if (duration.value.trim() && !(Number(duration.value) > 0)) { err.textContent = 'Estimated duration should be a number of hours, like 0.5.'; return; }
    save.disabled = true; save.textContent = line ? 'Saving…' : 'Adding…';
    let r;
    try {
      r = await api.xinspect({ type: line ? 'editLine' : 'addLine', id: line?.id, line: { sgcid: st.service.sgcid, sgid: st.service.sgid, color: st.color, payTypeId: pay.value,
        hours: hours.value.trim(), labor: labor.value.trim(), parts: parts.value.trim(), total: total.value.trim(),
        dispatchCode: dispatch.value, duration: duration.value.trim(),
        details: details.value.trim(), complaint: complaint.value.trim(), cause: cause.value.trim(), correction: correction.value.trim(), techNote: note.value.trim() } });
    } catch (e) { r = { error: true, message: String(e?.message || e) }; }
    save.disabled = false; save.textContent = line ? 'Save' : 'Add';
    if (r?.error) { err.textContent = r.message || (line ? "Xtime didn't save the changes" : "Xtime didn't add the service"); return; }
    toast(line ? `Saved ${line.dms ? `line ${line.dms}` : st.service.name}` : `Added: ${st.service.name}`);
    close();
    load();
  } });
  back.append(el('div', { class: 'x-modal-box', role: 'dialog' },
    el('div', { class: 'x-modal-head' }, el('b', { text: line ? `Edit Service${line.dms ? ` · line ${line.dms}` : ''}` : item ? `Add Service · ${item.name}` : 'Add Service' }), el('button', { class: 'icon', text: '×', title: 'Close (Esc)', onclick: close })),
    el('div', { class: 'x-modal-body' },
      field('Service Name', el('div', { class: 'x-svc' }, find, list), true),
      el('div', { class: 'x-f x-colors' }, el('span', {}, 'Color'), el('div', {}, colorBtn('Yellow', 'May Require Future Attention'), colorBtn('Red', 'Requires Immediate Attention'), colorNote)),
      el('div', { class: 'x-grid4' }, field('Labor Type', pay, true), field('Hours', hours), field('Labor', labor), field('Parts', parts), field('Total', total)),
      el('div', { class: 'x-grid4' }, field('Dispatch Code', dispatch, true), field('Est. Duration', duration, true)),
      field('Details', details, true), field('Concern / Service Request', complaint), field('Cause', cause), field('Correction', correction), field('Technician Note', note),
      el('p', { class: 'muted x-small', text: 'Empty hours or prices use the service\'s own, like Xtime.' })),
    el('div', { class: 'x-modal-foot' }, err, el('button', { class: 'ghost', text: 'Cancel', onclick: close }), save)));
  document.body.append(back);
  fill();
  if (line && st.service) { list.value = String(st.service.sgcid); find.value = st.service.name; details.focus(); }
  else find.focus();
}
