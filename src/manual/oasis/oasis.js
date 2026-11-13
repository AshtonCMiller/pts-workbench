// OASIS, putting the page together: reading Ford's OASIS page for this VIN and drawing each section in order (picture,
// connected vehicle data, vehicle information, FSAs, warnings, warranty), loading it, the buttons, and starting the
// page. Loaded last, after the other files (see common.js).
'use strict';

function render(res) {
  const doc = new DOMParser().parseFromString(stripStyles(res.html), 'text/html');
  const vin = clean(doc.title).replace(/^OASIS RESULTS?:\s*/i, '');
  const pairs = vehicleInfo(doc) || [];
  const map = new Map(pairs.map(([k, v]) => [k.toUpperCase(), [k, v]]));
  const desc = map.get('VEHICLE DESCRIPTION')?.[1] || '';
  const series = map.get('VERSION/SERIES')?.[1] || '';
  $('o-title').textContent = [desc, series && series.replace(/\s*VERSION$/i, '')].filter(Boolean).join(' · ') || 'OASIS';
  const stamp = clean(doc.querySelector('.panel-primary .panel-heading .col-md-4:last-child strong')?.textContent).replace(/LOCAL TIME.*$/i, '');
  $('o-sub').replaceChildren(el('span', { class: 'vin', text: vin, title: 'Click to copy the VIN', onclick: () => copy(vin) }),
    stamp ? `  ·  OASIS as of ${stamp}` : '');

  const used = new Set();
  const cards = [];
  const row = (key) => {
    const hit = map.get(key);
    if (!hit) return null;
    used.add(key);
    const [label, value] = hit;
    return el('div', { class: 'o-row' }, el('span', { class: 'k', text: nice(label) }),
      value ? el('span', { class: `v${CODES.has(key) ? ' code' : ''}`, text: value, title: 'Click to copy', onclick: () => copy(value) })
        : el('span', { class: 'v empty', text: '—' }));
  };
  for (const [title, keys] of GROUPS) {
    const rows = keys.map(row).filter(Boolean);
    if (rows.length) cards.push(el('div', { class: 'o-card' }, el('h3', { text: title }), ...rows));
  }
  const rest = [...map.keys()].filter((k) => !used.has(k)).map(row).filter(Boolean);
  if (rest.length) cards.push(el('div', { class: 'o-card' }, el('h3', { text: 'Other' }), ...rest));

  warranty = readWarranty(doc);
  const fsa = readFsa(doc);
  const warn = readWarnings(doc);
  $('o-body').replaceChildren(
    hero(res.image, $('o-title').textContent, fsa, warn),
    el('section', { class: 'o-section', id: 'cv' }),
    pairs.length
      ? el('section', { class: 'o-section' }, el('h2', { text: 'Vehicle information' }), el('div', { class: 'o-grid' }, ...cards))
      : el('section', { class: 'o-section' }, el('h2', { text: 'Vehicle information' }),
        el('div', { class: 'o-card o-msg', text: "OASIS didn't include the vehicle information section for this VIN." })),
    renderFsa(fsa),
    renderWarnings(warn),
    el('section', { class: 'o-section', id: 'warranty' }));
  if (!pairs.length) api.log(`oasis: no vehicle information found (${res.html.length} characters)`);
}

async function load() {
  $('o-body').replaceChildren(el('div', { class: 'state' }, el('div', { class: 'spinner' }), 'Loading OASIS…'));
  const res = await api.oasis();
  if (res?.error) {
    const actions = el('div', { class: 'actions' });
    if (res.needsLogin) actions.append(el('button', { class: 'primary', text: 'Sign in to PTS', onclick: async (e) => { e.target.disabled = true; await api.signIn(); load(); } }));
    else actions.append(el('button', { class: 'primary', text: 'Try again', onclick: load }));
    actions.append(el('button', { class: 'ghost', text: 'Open log', onclick: () => api.openLog() }));
    $('o-body').replaceChildren(el('div', { class: 'state' },
      el('h2', { text: res.needsLogin ? 'Sign in to PTS' : "Couldn't load OASIS" }),
      el('p', { text: res.needsLogin ? 'OASIS needs your PTS login.' : (res.message || '') }), actions));
    return;
  }
  try { render(res); renderWarranty(); loadCv(); loadSvc(); } catch (e) {
    api.log(`oasis: ${e && e.stack || e}`);
    $('o-body').replaceChildren(el('div', { class: 'state' }, el('h2', { text: "Couldn't show OASIS" }), el('p', { text: String(e?.message || e) })));
  }
}

$('btn-refresh').onclick = load;
$('btn-full').onclick = () => api.openPts();
load();
