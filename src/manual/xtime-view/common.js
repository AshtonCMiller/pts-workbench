// Xtime View, shared pieces: the app's bridge, small helpers (elements, toasts, copying, money / hours / times)
// and Xtime's names for things. Loaded first; the other files use these.
//
// Xtime View is the app's own page for an RO in Xtime (next to Xtime's own page). Its code is split by what it does:
//   common.js          this file: helpers every part uses
//   inspection.js      the inspection form (Green / Yellow / Red, values, sub-items, switching forms)
//   service-editor.js  adding, editing and deleting service lines (Xtime's Add Service window)
//   ro-view.js         the RO itself: header, Service Details, Vehicle Info, concerns, recommendations; loading
//                      and refreshing from Xtime, and starting the page (loaded last)
// Everything is read through the signed-in Xtime page (the same addresses Xtime's RO page uses). No customer details are read.
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
  for (const c of kids) if (c != null && c !== false && c !== '') n.append(c);
  return n;
}
function toast(text) {
  const t = $('o-toast');
  t.textContent = text; t.hidden = false;
  clearTimeout(toast.timer);
  toast.timer = setTimeout(() => { t.hidden = true; }, 1600);
}
const copy = (text, what) => { api.copy(String(text)); toast(`${what || 'Copied'}: ${text}`); };

// Xtime's stages (its ROModes list)
const MODES = { 9: 'Pre-RO', 2: 'Dispatch', 3: 'Being Inspected', 4: 'Pending Approval', 6: 'Being Repaired', 7: 'Review', 100: 'Closed' };
// Lines Xtime adds to every RO for the inspection (multipoint, green tires / battery / brakes): grouped at the bottom
const INSPECTION = /^(Q99PX|GTIRE|GBATT|GBK)\b/i;

const money = (n) => (n == null ? '' : `$${Number(n).toFixed(2)}`);
const hrs = (n) => (n == null ? '' : `${Number(n).toFixed(1)} h`);
// Xtime's times have no zone: they're the store's local time
function when(t, { date = true } = {}) {
  if (!t) return '';
  const d = t instanceof Date ? t : new Date(String(t).replace(/Z$/, ''));
  if (Number.isNaN(d.getTime())) return String(t);
  const time = d.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });
  if (!date) return time;
  const today = new Date();
  if (d.toDateString() === today.toDateString()) return `today ${time}`;
  const y = new Date(today); y.setDate(today.getDate() - 1);
  if (d.toDateString() === y.toDateString()) return `yesterday ${time}`;
  return `${d.toLocaleDateString([], { weekday: 'short', month: 'short', day: 'numeric' })} ${time}`;
}
function promised(v) {
  if (!v.promisedDate) return '';
  const [m, d, y] = v.promisedDate.split('/').map(Number);
  const [hh = 0, mm = 0] = String(v.promisedTime || '').split(':').map(Number);
  if (!y) return `${v.promisedDate} ${v.promisedTime || ''}`.trim();
  return when(new Date(y, m - 1, d, hh, mm));
}
