// Connected Vehicle, shared pieces: the app's bridge, small helpers (elements, toasts, copying, dates, units), the
// messages shown when Ford has nothing or asks to sign in, and what's saved with the tab. Loaded first.
//
// The Connected Vehicle tab shows what the vehicle reports to Ford through its modem, the way PTS's Connected Vehicle
// page does, in the app's style. Its code:
//   common.js          this file: helpers, saved state
//   dtcs.js            each DTC with its TSBs / GSBs / SSMs, the Recent DTCs summary, picking DTCs to search in Diagnostics
//   home.js            Home: the latest status (odometer, fuel, oil life, tires, 12V battery) and the health history (DTCs, alerts)
//   ota.js             Over-the-air updates: the vehicle's OTA settings, updates lined up, in progress and finished, release notes
//   subscriptions.js   Software: the vehicle's software subscriptions (connectivity, BlueCruise…)
//   cv.js              the tabs along the top, loading, the buttons, and starting the page (loaded last)
// Nothing here is saved to disk; Ford is asked again each time the tab opens.
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
  toast.timer = setTimeout(() => { t.hidden = true; }, 1400);
}
function copy(text) { api.copy(text); toast(`Copied ${text}`); }

// ---------- dates, numbers, units ----------
const KM_PER_MILE = 1.60934;
const PSI_PER_KPA = 0.145038;
// Ford's times: "2026-03-02 14:05:00" (UTC), ISO, or seconds / milliseconds since 1970
function toDate(t) {
  if (t == null || t === '') return null;
  if (typeof t === 'number' || /^\d{9,13}$/.test(String(t))) { const n = Number(t); return new Date(n < 1e11 ? n * 1000 : n); }
  const s = String(t);
  const d = new Date(/^\d{4}-\d\d-\d\d[ T]\d/.test(s) && !/[zZ]|[+-]\d\d:?\d\d$/.test(s) ? `${s.replace(' ', 'T')}Z` : s);
  return isNaN(d) ? null : d;
}
const fmtTime = (t) => { const d = toDate(t); return d ? d.toLocaleString([], { month: 'short', day: 'numeric', year: 'numeric', hour: 'numeric', minute: '2-digit' }) : String(t || ''); };
const fmtDate = (t) => { const d = toDate(t); return d ? d.toLocaleDateString([], { month: 'short', day: 'numeric', year: 'numeric' }) : String(t || ''); };
// 2026-03-02 for Ford's requests (in local time)
const ymd = (d) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
// LOW_PRESSURE → Low pressure
const words = (v) => (v == null || v === '' ? null : /^[A-Z0-9_]+$/.test(String(v)) && /[A-Z]{2}/.test(String(v)) ? String(v).toLowerCase().replace(/_/g, ' ').replace(/^./, (c) => c.toUpperCase()) : String(v));
const num = (m) => { const v = m && typeof m === 'object' ? m.value : m; return v != null && v !== '' && v !== -1000 && !isNaN(Number(v)) ? Number(v) : null; };
const pct = (m) => { const n = num(m); return n == null || n < 0 || n > 100 ? null : `${Math.floor(n)}%`; };
let miles = true; // the vehicle's country: miles in the USA, km elsewhere
const dist = (km) => (km == null ? null : miles ? `${Math.floor(km / KM_PER_MILE).toLocaleString()} mi` : `${Math.floor(km).toLocaleString()} km`);
// tire pressures: Ford sends kPa; shown in psi in the USA
const tire = (kpa) => (kpa == null || kpa < 0 ? null : miles ? Math.round(kpa > 80 ? kpa * PSI_PER_KPA : kpa) : Math.round(kpa));
const tireUnit = () => (miles ? 'psi' : 'kPa');

// label / value row, as on the OASIS tab
function statusRow(label, value, when, cls) {
  return el('div', { class: 'o-row' }, el('span', { class: 'k', text: label }),
    value != null && value !== ''
      ? el('span', { class: `v ${cls || ''}`, title: when ? `Reported ${fmtTime(when)}` : 'Click to copy', onclick: () => copy(String(value)) },
        String(value), when ? el('span', { class: 'when', text: fmtTime(when) }) : null)
      : el('span', { class: 'v empty', text: '—' }));
}
const msg = (text, cls = '') => el('div', { class: `o-card o-msg ${cls}` }, text);
const waiting = (text) => el('div', { class: 'o-card o-msg' }, el('div', { class: 'spinner' }), text);

// A request that didn't work: sign in, or try again (and the log for the details)
function failed(res, what, retry) {
  const actions = el('div', { class: 'actions' });
  if (res.needsLogin) actions.append(el('button', { class: 'primary', text: 'Sign in to PTS', onclick: async (e) => { e.target.disabled = true; await api.signIn(); retry(); } }));
  else actions.append(el('button', { class: 'primary', text: 'Try again', onclick: retry }));
  actions.append(el('button', { class: 'ghost', text: 'Open log', onclick: () => api.openLog() }));
  return el('div', { class: 'state' },
    el('h2', { text: res.needsLogin ? 'Sign in to PTS' : `Couldn't load ${what}` }),
    el('p', { text: res.needsLogin ? 'Connected vehicle data needs your PTS login.' : (res.message || '') }), actions);
}

// The one call each part makes: never throws, gives { error, message } instead
async function ask(fn) {
  try { return await fn(); } catch (e) { return { error: true, message: String(e?.message || e) }; }
}

// what's saved with the tab: which part was open
let state = { view: 'home' };
function save() { api.save({ view: state.view }); }
