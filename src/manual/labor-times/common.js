// Labor Times, shared pieces: the app's bridge, small helpers (elements, toasts, copying, hours), and what's saved with
// the tab (the tray, the search type, the last search). Loaded first; the other files use these.
//
// Labor Times is Ford's SLTS (Service Labor Time Standards) for this vehicle, redrawn in the app's style. SLTS is an old
// ASP.NET page that works by posting its form back to itself, so this page keeps the last page Ford sent, fills in its
// form the way SLTS's own controls would, and reads the report out of what comes back. Its code:
//   common.js       this file: helpers, saved state
//   slts.js         talking to SLTS: keeping its last page, filling in and posting back its form
//   search-form.js  the search form (mirrors SLTS's own: search type, number / text, the pick list)
//   word-search.js  Word Search (SLTS's "Keyword Search for Operation Description" window)
//   report.js       reading SLTS's report into operations, options, overlaps, combinations and supplements
//   results.js      showing the results: operation cards, their options, scrolling to the one asked for
//   tray.js         the tray: picked operations adding up warranty and customer pay time, with overlap warnings
//   parts.js        one-time use parts (SLTS's "Required Parts" list for an operation)
//   print.js        printing the tray or a parts list
//   labor.js        the buttons, and starting the page (loaded last)
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
// 18124A → 18124, 19700D12 → 19700, 19700A4WA → 19700
const baseNumber = (code) => code.replace(/[A-Z]{1,2}\d*[A-Z]*$/, '') || code;
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
