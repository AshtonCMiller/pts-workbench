// TSBs, shared pieces: the app's bridge, small helpers, Ford's article types, reading Ford's HTML safely, and what's
// saved with the tab. Loaded first; the other files use these.
//
// The TSBs tab lists the vehicle's TSBs, GSBs, SSMs (and ISBs) from Ford's list for this VIN (TSB_SSM DisplayArticles),
// with Ford's categories and a filter box, and shows each article redrawn in the app's style (so it follows light /
// dark mode). "Ford's page" opens Ford's own version as a tab. The Diagnostics tab sends TSBs here. Its code:
//   common.js   this file: helpers, saved state
//   list.js     Ford's list for this VIN: reading it, the categories, the filter, drawing the list
//   article.js  one article: fetching it, redrawing Ford's HTML in the app's style, links inside it, ones sent from Diagnostics
//   tsb.js      loading the list, the buttons and keys, and starting the page (loaded last)
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
function toast(text) {
  const t = $('o-toast');
  t.textContent = text; t.hidden = false;
  clearTimeout(toast.timer);
  toast.timer = setTimeout(() => { t.hidden = true; }, 1600);
}
const TSB_BASE = 'https://www.fordtechservice.dealerconnection.com/vdirsnet/TSB_SSM/';
const TYPES = { T: 'TSB', G: 'GSB', S: 'SSM', I: 'ISB' };

// Ford's inline styles would trip this page's security rules; note which elements they hid first
function prepare(html) {
  return String(html)
    .replace(/<(script|style|noscript)\b[\s\S]*?<\/\1>/gi, '')
    .replace(/<link\b[^>]*>/gi, '')
    .replace(/\sstyle\s*=\s*("[^"]*"|'[^']*')/gi, (m, v) => (/display\s*:\s*none|visibility\s*:\s*hidden/i.test(v) ? ' data-hidden="1"' : ''));
}
const parse = (html) => new DOMParser().parseFromString(prepare(html), 'text/html');

let state = { q: '', cat: '', type: '' };
let info = {};
function save() { api.save({ q: state.q, cat: state.cat, type: state.type }); }
