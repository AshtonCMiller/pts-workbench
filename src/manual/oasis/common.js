// OASIS, shared pieces: the app's bridge, small helpers (elements, toasts, copying), and reading Ford's HTML without
// its scripts and styles. Loaded first; the other files use these.
//
// The OASIS tab is Ford's OASIS for this VIN redrawn in the app's style. Its code is split by section:
//   common.js     this file: helpers
//   vehicle.js    the vehicle's picture (with recall / warning counts) and the Vehicle information cards
//   fsa.js        field service actions (recalls), with Retrieve SVC for the ones that need a software validation code
//   warnings.js   OASIS warning messages
//   warranty.js   warranty coverage, checked against the start date and the odometer
//   connected.js  connected vehicle data (odometer, fuel, oil life, battery, tires…)
//   oasis.js      putting the page together, loading OASIS, the buttons, and starting the page (loaded last)
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
const clean = (t) => String(t || '').replace(/\s+/g, ' ').trim();
// Ford's styles aren't used, and this page's security rules would flag each one
const stripStyles = (html) => String(html).replace(/<(script|style)\b[\s\S]*?<\/\1>/gi, '').replace(/\sstyle\s*=\s*(?:"([^"]*)"|'([^']*)')/gi,
  (_m, a, b) => (/display\s*:\s*none/i.test(a || b || '') ? ' data-hidden="1"' : ''));

// a panel's heading in Ford's page (FSAs, warnings…)
const headerText = (doc, panel) => clean(doc.querySelector(`label[data-results="${panel}"]`)?.textContent).replace(/^[▴▾\s]+/, '');

function toast(text) {
  const t = $('o-toast');
  t.textContent = text; t.hidden = false;
  clearTimeout(toast.timer);
  toast.timer = setTimeout(() => { t.hidden = true; }, 1400);
}
function copy(text) { api.copy(text); toast(`Copied ${text}`); }
