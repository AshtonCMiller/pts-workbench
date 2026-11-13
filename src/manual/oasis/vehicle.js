// OASIS, the vehicle: its picture at the top (from the PTS manual page) with the recall / warning counts, and the
// Vehicle information cards (Ford's fields grouped; codes click to copy).
'use strict';

// How the vehicle information is grouped (by Ford's label); anything new lands in "Other"
const GROUPS = [
  ['Vehicle', ['VEHICLE DESCRIPTION', 'VERSION/SERIES', 'BODY STYLE', 'DRIVE TYPE', 'FUEL TYPE', 'GROSS VEHICLE WEIGHT']],
  ['Powertrain', ['ENGINE', 'ENGINE CALIBRATION', 'TRANSMISSION', 'AXLE RATIO', 'AXLE CODE']],
  ['Paint, wheels & tires', ['PAINT COLOR', 'PAINT CODE', 'WHEEL SIZE', 'TIRE']],
  ['Electronics', ['SYNC VERSION', 'RADIO', 'MODEM', 'VHR ACTIVATED']],
  ['Sale', ['SOLD TO FLEET', 'RETAIL SALES TYPE']],
];
const CODES = new Set(['PAINT CODE', 'ENGINE CALIBRATION', 'AXLE CODE', 'RETAIL SALES TYPE']);
const nice = (k) => k.toLowerCase().replace(/(^|[\s/(])([a-z])/g, (m) => m.toUpperCase()).replace(/\bGvw\b/, 'GVW').replace(/\bSync\b/, 'SYNC').replace(/\bVhr\b/, 'VHR');

function vehicleInfo(doc) {
  const panel = doc.getElementById('vehicleinfo-panel');
  if (!panel) return null;
  const pairs = [];
  panel.querySelectorAll('label.label').forEach((lab) => {
    const k = clean(lab.textContent).replace(/:$/, '');
    let v = lab.nextElementSibling;
    while (v && v.tagName !== 'LABEL') v = v.nextElementSibling;
    if (k) pairs.push([k, clean(v?.textContent)]);
  });
  return pairs;
}

// ---------- picture of the vehicle (from the PTS manual page) ----------
function hero(image, name, fsa, warn) {
  const box = el('section', { class: 'o-hero' });
  if (image) {
    const img = el('img', { class: 'o-car', alt: name || 'Vehicle', src: `ptsasset://f/?u=${encodeURIComponent(image)}` });
    img.addEventListener('error', () => { api.log(`oasis: vehicle picture didn't load (${image.slice(0, 160)})`); img.remove(); box.classList.add('no-pic'); });
    box.append(el('div', { class: 'o-stage' }, img));
  } else box.classList.add('no-pic');
  // A quick heads-up for what's further down the page
  const chips = [];
  const jump = (id) => () => document.getElementById(id)?.scrollIntoView({ behavior: 'smooth', block: 'start' });
  if (fsa.items.length) chips.push(el('button', { class: 'o-chip bad', onclick: jump('fsa') }, `${fsa.items.length} open field service action${fsa.items.length === 1 ? '' : 's'}`));
  else if (fsa.found) chips.push(el('button', { class: 'o-chip good', onclick: jump('fsa') }, 'No open field service actions'));
  if (warn.lines.length) chips.push(el('button', { class: 'o-chip warn', onclick: jump('warnings') }, `${warn.lines.length} warning message${warn.lines.length === 1 ? '' : 's'}`));
  if (chips.length) box.append(el('div', { class: 'o-chips' }, ...chips));
  return box;
}
