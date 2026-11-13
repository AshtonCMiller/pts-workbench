// Workshop Manual (and PC/ED), shared pieces: the app's bridge, what's saved with the tab (open procedures, open
// folders, where you were in each), the manual's contents as loaded, and small helpers. Loaded first.
//
// The viewer shows Ford's contents and procedures redrawn in the app's style, for the Workshop Manual and PC/ED tabs
// (the same viewer, a different Ford book). Closing and reopening the app (or the RO) brings everything back. Its code:
//   common.js          this file: state and helpers
//   contents.js        loading Ford's contents and drawing them (folders, procedures with their parts under them)
//   vehicle-filter.js  "Only this vehicle": which entries apply to this vehicle's engine, transmission, options
//   search.js          searching the contents (Ctrl+K), and DTCs: PC/ED's own results, or Diagnostics
//   documents.js       open procedures as tabs, showing one, the welcome / problem screens
//   procedure.js       turning Ford's procedure HTML into the app's (pictures, tables, links), Back and Print
//   pinpoints.js       pinpoint tests: single-step view, Yes / No answers, reporting to Vehicle History
//   viewer.js          the layout (contents width, hide / show), keys, and starting the page (loaded last)
'use strict';

const api = window.manual;
const $ = (id) => document.getElementById(id);

let state = { docs: [], active: null, open: [], query: '', tocWidth: 340, tocHidden: false, allOptions: false };
let tree = null;          // [{ type: 'folder'|'doc'|'link', title, children, link, url, path }] (as shown: this vehicle's, or everything)
let fullTree = null;      // everything in Ford's contents; entries that don't apply to this vehicle have na: true
let filterable = false;   // Ford marked which entries apply to which options, and the vehicle's options are known
let leaves = [];          // every doc/link, for search
let bookKind = 'workshop'; // 'workshop' or 'pced' (PC/ED: the same viewer, Ford's PC/ED book)
const bookName = () => (bookKind === 'pced' ? 'PC/ED' : 'Workshop Manual');
const docCache = new Map(); // doc key -> rendered element
const backStacks = new Map(); // doc key -> [scrollTop, ...] (this session only)

// ---------- helpers ----------
function el(tag, props = {}, ...kids) {
  const n = document.createElement(tag);
  for (const [k, v] of Object.entries(props)) {
    if (v == null || v === false) continue;
    if (k === 'class') n.className = v;
    else if (k === 'text') n.textContent = v;
    else if (k === 'style') { for (const part of String(v).split(';')) { const i = part.indexOf(':'); if (i > 0) n.style.setProperty(part.slice(0, i).trim(), part.slice(i + 1).trim()); } }
    else if (k.startsWith('on')) n.addEventListener(k.slice(2), v);
    else n.setAttribute(k, v);
  }
  for (const c of kids) if (c != null && c !== false) n.append(c);
  return n;
}
const docKey = (link) => `${link.linkType || 'Procedure'}|${link.searchNumber}|${link.procUid || ''}`;
let saveTimer = null;
function save() {
  clearTimeout(saveTimer);
  saveTimer = setTimeout(() => api.save(JSON.parse(JSON.stringify(state))), 400);
}
const activeDoc = () => state.docs.find((d) => d.key === state.active) || null;

// Ford's inline styles aren't used (and this page's security rules would flag every one), so they're
// dropped before parsing; only "hidden" is kept, as a marker.
const stripStyles = (html) => String(html).replace(/\sstyle\s*=\s*(?:"([^"]*)"|'([^']*)')/gi,
  (_m, a, b) => (/display\s*:\s*none/i.test(a || b || '') ? ' data-hidden="1"' : ''));

// The manual writes a link target either as "G123#part" or "G123:procsub2"
function splitTarget(forAttr) {
  const s = String(forAttr || '');
  const m = s.match(/^(.*?):((?:procsub|figure_|table_).*)$/);
  if (m) return { searchNumber: m[1], hash: m[2] };
  const [searchNumber, hash = ''] = s.split('#');
  return { searchNumber, hash };
}
