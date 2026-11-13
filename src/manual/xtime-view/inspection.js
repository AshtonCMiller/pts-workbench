// Xtime View, the inspection form: read and changed exactly like Xtime's page does it (Green / Yellow / Red, values,
// sub-items, switching to another form). Yellow / Red items offer Add Service (service-editor.js).
// Drawn into its own box (inspBox), which ro-view.js places at the bottom of the page.
'use strict';

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
