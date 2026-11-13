// Labor Times, the search form: mirrors SLTS's own (search type, the number or words, the pick list), changing the
// search type the way SLTS does, and running a search.
'use strict';

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

// SLTS opens Word Search in its own little window (AlphaIndex1.aspx) instead of on its main form
const WORD = (t) => /^word/i.test(t || '');
function showWordForm() {
  $('l-pick').hidden = true;
  $('l-q').hidden = false;
  $('l-q').placeholder = 'Words in the operation name, e.g. compressor, water pump…';
  for (const o of $('l-type').options) o.selected = o.value === 'Word' || WORD(o.value);
}

async function changeType(type) {
  if (busy) return;
  if (WORD(type)) { saved.type = type; saved.pick = ''; save(); showWordForm(); hint(); $('l-q').focus(); return; }
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
  if (WORD(type)) return wordSearch(q);
  busy = true;
  showState(el('div', { class: 'spinner' }), 'Asking SLTS…');
  try {
    if (currentType() !== type) { await postBack({ DropDownList_SearchType: type }, { eventTarget: 'DropDownList_SearchType' }); renderForm(); }
    const over = { DropDownList_SearchType: type };
    if (!$('l-pick').hidden) over.DropDownList_SearchFor = $('l-pick').value;
    // SLTS's operation search takes the base number (19700); a full code (19700A4) is looked up and jumped to
    if (!$('l-q').hidden) over[$('l-q').dataset.name || 'TextBox_OperationOrPartNumber'] = /^operation/i.test(type) ? baseNumber(q.toUpperCase().replace(/\s+/g, '')) : q;
    const doc = await postBack(over, { click: true });
    saved.type = type; saved.pick = $('l-pick').hidden ? '' : $('l-pick').value; saved.q = $('l-q').hidden ? '' : q; save();
    renderForm();
    showResults(doc, q);
  } catch (e) { failed(e, search); } finally { busy = false; }
}
