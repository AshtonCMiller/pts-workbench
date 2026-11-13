// Labor Times, Word Search: SLTS's "Keyword Search for Operation Description" window. 1. open AlphaIndex1.aspx for this
// vehicle, 2. post the words → a list of matching operation names, 3. post the one picked with "View" → SLTS then shows
// that operation on its main page.
'use strict';

// ---------- Word Search (SLTS's "Keyword Search for Operation Description" window) ----------
// 1. open AlphaIndex1.aspx for this vehicle, 2. post the words → a list of matching operation names,
// 3. post the one picked with "View" → SLTS then shows that operation on its main page.
let word = null; // { doc, url, raw } — the keyword window's last page
function hidden(name) { return page?.doc.getElementById(name)?.getAttribute('value') || ''; }
function wordUrl() {
  const q = new URLSearchParams({ year_select: hidden('hidden_Year'), model_select: hidden('hidden_Model'), vin: hidden('hidden_Vin'),
    fileName: hidden('hidden_FileName').replace(/\\/g, '|'), cookie_tps_membership: decodeURIComponent(hidden('hidden_cookie_tps_membership')), tab: 'tab' });
  return `AlphaIndex1.aspx?${q}`;
}
function fieldsOf(doc) { const keep = page; page = { doc, url: '' }; const f = formFields(); page = keep; return f; }
function actionOf(doc, url) {
  const f = doc.querySelector('form');
  try { return new URL(f?.getAttribute('action') || url, url).href; } catch { return url; }
}
async function wordPost(overrides) {
  const fields = { ...fieldsOf(word.doc), __EVENTTARGET: '', __EVENTARGUMENT: '', ...overrides };
  const res = await request({ op: 'post', method: 'POST', url: actionOf(word.doc, word.url), fields });
  return { raw: res.html, url: res.url, doc: parse(res.html) };
}

async function wordSearch(q) {
  if (busy) return;
  busy = true;
  showState(el('div', { class: 'spinner' }), 'Asking SLTS…');
  try {
    if (!page) await start();
    const first = await request({ op: 'get', url: wordUrl() });
    word = { raw: first.html, url: first.url, doc: parse(first.html) };
    if (!word.doc.getElementById('TextBox_OperationDesc')) throw new Error("SLTS's word search didn't open");
    word = await wordPost({ TextBox_OperationDesc: q, 'ImageButton_View_OperationDesc.x': '9', 'ImageButton_View_OperationDesc.y': '9' });
    saved.q = q; save();
    const list = [...(word.doc.getElementById('ListBox_OperationDesc')?.querySelectorAll('option') || [])]
      .map((o) => ({ value: o.getAttribute('value') ?? o.textContent, text: clean(o.textContent) })).filter((o) => o.text);
    api.log(`labor: word search "${q}": ${list.length} match${list.length === 1 ? '' : 'es'}${list.length ? ` (e.g. ${list.slice(0, 3).map((o) => `${o.text} = ${o.value}`).join('; ')})` : ''}`);
    if (!list.length) {
      const msg = clean(word.doc.querySelector('form')?.textContent).replace(/Keyword Search for Operation Description|View\s+Cancel/g, '').trim();
      $('l-body').replaceChildren(el('div', { class: 'd-empty', text: `SLTS found no operations named “${q}”.` }));
      if (msg) api.log(`labor: word search page said: ${msg.slice(0, 300)}`);
      return;
    }
    $('l-body').replaceChildren(
      el('div', { class: 'l-title', text: `${list.length} operation${list.length === 1 ? '' : 's'} matching “${q}”: pick one to see its times` }),
      el('div', { class: 'l-matches' }, ...list.map((o) => {
        const m = o.text.match(/^([0-9A-Z]+)\s+-+\s+(.*)$/i); // "18124A ---- Shock Absorber - Front - …"
        return el('button', { class: 'l-match', onclick: () => wordOpen(o) },
          m ? el('span', { class: 'l-code', text: m[1] }) : null, el('span', { text: m ? m[2] : o.text }), el('span', { class: 'go', text: 'View ›' }));
      })));
  } catch (e) { failed(e, () => wordSearch(q)); } finally { busy = false; }
}

// One match: its value is the operation code (e.g. "18124A", "19700D12"). Search SLTS by that
// operation's base number (18124, 19700) like the Operation Number search does, then jump to it.
async function wordOpen(o) {
  if (busy) return;
  const code = (clean(o.value) || clean(o.text).split(/\s/)[0]).toUpperCase().replace(/[^0-9A-Z]/g, '');
  if (!code) return;
  busy = true;
  showState(el('div', { class: 'spinner' }), 'Asking SLTS…');
  try {
    if (currentType() !== 'Operation') await postBack({ DropDownList_SearchType: 'Operation' }, { eventTarget: 'DropDownList_SearchType' });
    const box = searchBox(page.doc)?.getAttribute('name') || 'TextBox_OperationOrPartNumber';
    const doc = await postBack({ DropDownList_SearchType: 'Operation', [box]: baseNumber(code) }, { click: true });
    showResults(doc, code);
    showWordForm();
    $('l-body').prepend(el('button', { class: 'ghost small l-back', text: '‹ Back to the matches', onclick: () => wordSearch(saved.q) }));
  } catch (e) { failed(e, () => wordOpen(o)); } finally { busy = false; }
}
