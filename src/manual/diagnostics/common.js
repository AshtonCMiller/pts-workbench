// Diagnostics, shared pieces: the app's bridge, small helpers, Ford's result groups, what a DTC looks like, and what's
// saved with the tab. Loaded first; the other files use these.
//
// Diagnostics is Ford's "Enhanced Search" for this vehicle, in the app's style. Type DTCs and/or words; results come back
// grouped like Ford's Diagnostics tab (TSB/GSB/SSM, pinpoint tests, procedures, PC/ED, wiring, videos). Its code:
//   common.js        this file: helpers, groups, saved state
//   search.js        reading what was typed (DTCs and words), running the search, the group tabs, loading more
//   results.js       the results list, and opening a result (Workshop Manual / PC/ED at the test, TSBs tab, wiring…)
//   vehicle-dtcs.js  "DTCs from the vehicle": the DTCs it last reported (connected vehicle data), to search
//   diag.js          starting the page, and the search box and buttons (loaded last)
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
function toast(text) {
  const t = $('o-toast');
  t.textContent = text; t.hidden = false;
  clearTimeout(toast.timer);
  toast.timer = setTimeout(() => { t.hidden = true; }, 1600);
}

// The groups, in Ford's order
const GROUPS = [
  { id: 'tsb', label: 'TSB / GSB / SSM', sources: ['tsb', 'gsb', 'ssm'] },
  { id: 'wsmppt', label: 'Pinpoint tests', sources: ['wsmppt'] },
  { id: 'wsmproc', label: 'Workshop procedures', sources: ['wsmproc'] },
  { id: 'pced', label: 'PC/ED', sources: ['pced'] },
  { id: 'wiring', label: 'Wiring', sources: ['wiring'] },
  { id: 'tsv', label: 'Support videos', sources: ['tsv'] },
];
const ROWS = 50;
// Like Ford's: P0420, U3000:49, U3000:49:ABS, B1A2B|87
const DTC_RE = /(?<=\s|^|[,;:])[PBUC][0-9A-F]{4,6}(?:[|:][0-9A-F]{2}(?:[|:][A-Z]{2,12})?|[|:][A-Z]{2,12})?(?=\s|$|[,;])/gi;

let state = { query: '', exact: false, group: '' };
let search = null; // { text, dtcs, counts, dtcInfo, lists: { group: { items, more, loading, error } } }
let token = 0;

function save() { api.save({ ...state }); }
