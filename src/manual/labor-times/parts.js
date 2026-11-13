// Labor Times, one-time use parts: SLTS's "Required Parts" list for an operation (fetched once and kept), shown in a
// window that can be printed.
'use strict';

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
