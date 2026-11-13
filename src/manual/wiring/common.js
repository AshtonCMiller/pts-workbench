// Wiring Diagrams, shared pieces: the app's bridge, small helpers, what's saved with the tab, and whether a diagram or
// page applies to this VIN. Loaded first; the other files use these.
//
// Wiring Diagrams shows the vehicle's wiring book from Ford (the same lists and drawings PTS's /wiring pages use). Its
// code is split by what it does:
//   common.js    this file: helpers, saved state, "does it apply to this VIN?"
//   contents.js  the list of diagrams (gray when they don't apply to this VIN), its filter and sorting
//   search.js    the Search window (Ctrl+K): Ford's search, wiring only (components, connectors, splices, grounds, diagrams)
//   page.js      opening a page: Ford's page lists and drawings (fetched once, kept), the page buttons, Back
//   drawing.js   Ford's drawing made safe and themed: no scripts, its styles as attributes, the app's colors
//   zoom.js      zoom and drag, the zoom-in-then-out on a searched item, marking items on the page
//   find.js      find words on this page (Ctrl+F)
//   links.js     links in the drawing: page numbers open that page; connectors / splices / grounds show where they are
//   wiring.js    the buttons and keys, and starting the page (loaded last)
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
const clean = (t) => String(t || '').replace(/ /g, ' ').replace(/\s+/g, ' ').trim();
const cellNo = (c) => String(Number(String(c || '').replace(/\D/g, '')) || '');
function toast(text) {
  const t = $('o-toast');
  t.textContent = text; t.hidden = false;
  clearTimeout(toast.timer);
  toast.timer = setTimeout(() => { t.hidden = true; }, 1800);
}

let state = { q: '', sort: 'cell', only: true, at: null }; // at: { cell, page, item, title } — the page showing
let backList = [];
function save() { api.save({ q: state.q, sort: state.sort, onlyVin: state.only, at: state.at }); }

// ---------- does it apply to this VIN? (as Ford's wiring pages work it out) ----------
let features = null; // { pfc, mfc, vehicleLine, buildDate } or null (then everything shows as applying)
function cellApplies(it) {
  if (!features || !Array.isArray(it.Code) || !it.Code.length) return true;
  return it.Code.some((c) => {
    const code = String(c).replace('-', '').replace('_', '0');
    return features.pfc.includes(code) || features.mfc.includes(code);
  });
}
function pageApplies(pg) {
  if (!features) return true;
  const build = parseInt(features.buildDate, 10);
  const day = (v) => (v && String(v).length ? parseInt(String(v).replace(/-/g, ''), 10) : NaN);
  const sd = day(pg.Startdate), ed = day(pg.Enddate);
  if (build && !Number.isNaN(sd) && !(sd <= build)) return false;
  if (build && !Number.isNaN(ed) && !(ed >= build)) return false;
  const groups = Array.isArray(pg.Code) && pg.Code.length ? String(pg.Code).replace(/[()]/g, '') : '';
  if (groups) {
    const all = [...features.pfc, ...features.mfc, ...features.vehicleLine];
    for (const g of groups.split('+')) {
      const quals = g.toUpperCase().split('|');
      if (!quals.some((q) => (q.startsWith('!') ? !all.includes(q.slice(1)) : all.includes(q)))) return false;
    }
  }
  return true;
}
