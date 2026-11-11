// OASIS, redrawn in the app's style: the vehicle's picture, connected vehicle data, vehicle information,
// field service actions, warning messages, then warranty coverage.
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

function toast(text) {
  const t = $('o-toast');
  t.textContent = text; t.hidden = false;
  clearTimeout(toast.timer);
  toast.timer = setTimeout(() => { t.hidden = true; }, 1400);
}
function copy(text) { api.copy(text); toast(`Copied ${text}`); }

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

// ---------- field service actions (recalls) ----------
const headerText = (doc, panel) => clean(doc.querySelector(`label[data-results="${panel}"]`)?.textContent).replace(/^[▴▾\s]+/, '');
function readFsa(doc) {
  const panel = doc.getElementById('fsa-panel');
  const out = { found: !!panel, title: headerText(doc, 'fsa-panel'), items: [], links: [] };
  if (!panel) return out;
  panel.querySelectorAll('li').forEach((li) => {
    const a = li.querySelector('a[data-fsanum], a');
    const num = clean(a?.getAttribute('data-fsanum') || a?.textContent);
    const desc = clean([...li.querySelectorAll('label')].map((l) => l.textContent).join(' ')) || clean(li.textContent).replace(num, '').trim();
    if (num || desc) out.items.push({ num, desc, url: a?.getAttribute('href') || '' });
  });
  // Ford's notices above the list (software repair steps, the list of recalls the process supports)
  panel.querySelectorAll(':scope .row a[href]').forEach((a) => {
    if (a.closest('li')) return;
    const text = clean(a.closest('label')?.textContent || a.textContent);
    if (text) out.links.push({ text: text.replace(/^NOTICE\s*[–-]\s*/i, ''), url: a.getAttribute('href'), label: clean(a.textContent) });
  });
  return out;
}
function renderFsa(fsa) {
  const box = el('section', { class: 'o-section', id: 'fsa' }, el('h2', { text: 'Field service actions (recalls)' }));
  if (!fsa.found) return box.appendChild(el('div', { class: 'o-card o-msg', text: "OASIS didn't include field service actions for this VIN." })), box;
  if (!fsa.items.length) {
    box.append(el('div', { class: 'o-card o-msg ok' }, el('b', { text: '✓ ' }), /no\b/i.test(fsa.title) ? nice(fsa.title) : 'No open field service actions for this VIN.'));
    return box;
  }
  const rows = fsa.items.map((f) => el('div', { class: 'o-fsa' },
    f.url ? el('button', { class: 'o-fsa-num', text: f.num, title: `Open ${f.num}`, onclick: () => api.openArticle(f.url, `FSA ${f.num}`) })
      : el('span', { class: 'o-fsa-num', text: f.num }),
    el('span', { class: 'o-fsa-desc', text: f.desc }),
    el('span', { class: 'o-svc', 'data-fsa': f.num }),
    el('button', { class: 'ghost small', text: 'Copy FSA', title: `Copy ${f.num}`, onclick: () => copy(f.num) })));
  const notes = fsa.links.map((l) => el('div', { class: 'o-note' }, el('button', { class: 'link', text: l.text, onclick: () => api.openArticle(l.url, l.label || 'Notice') })));
  box.append(el('div', { class: 'o-card o-fsas' }, ...rows), ...(notes.length ? [el('div', { class: 'o-notes' }, ...notes)] : []));
  return box;
}

// ---------- software validation codes (SVC) ----------
// Only FSAs on Ford's software verification form get a "Retrieve SVC" button
let svcInfo = null; // { fsas, jv, saved, dealerCode }
const svcState = new Map(); // FSA → { status, code, message, … } for this page
const svcSlot = (num) => document.querySelector(`.o-svc[data-fsa="${CSS.escape(num)}"]`);

async function loadSvc() {
  if (!document.querySelector('.o-svc')) return;
  let res;
  try { res = await api.svcList(); } catch (e) { res = { error: true, message: String(e?.message || e) }; }
  if (res?.error) { api.log(`oasis: SVC list: ${res.message || 'failed'}`); return; }
  svcInfo = res;
  for (const [num, v] of Object.entries(res.saved || {})) if (v?.code && !svcState.has(num)) svcState.set(num, { status: 'ok', code: v.code, saved: v.at });
  for (const f of res.fsas || []) renderSvc(f.num);
}

function renderSvc(num) {
  const slot = svcSlot(num);
  const f = svcInfo?.fsas?.find((x) => x.num === num);
  if (!slot || !f) return;
  const st = svcState.get(num) || {};
  const retrieve = (label = 'Retrieve SVC', cls = 'primary small') =>
    el('button', { class: cls, text: label, title: 'Ask Ford for the software validation code', onclick: () => getSvc(num) });
  const some = f.some ? el('span', { class: 'o-svc-note', title: 'Some vehicles in this FSA don’t need a software update or SVC. Check the FSA instructions.', text: '**' }) : null;
  let kids;
  if (st.status === 'loading') kids = [el('span', { class: 'o-svc-wait' }, el('span', { class: 'spinner' }), 'Checking…')];
  else if (st.status === 'ok') {
    kids = [el('span', { class: 'o-svc-code', title: 'Software validation code. Click to copy', text: st.code, onclick: () => copy(st.code) }),
      el('button', { class: 'primary small', text: 'Copy SVC', onclick: () => copy(st.code) }),
      el('button', { class: 'ghost small', text: '↻', title: 'Ask Ford again', onclick: () => getSvc(num) })];
  } else if (st.status === 'incomplete') {
    kids = [el('span', { class: 'o-svc-bad', title: [st.modules && `Modules: ${st.modules}`, st.detail && `Status: ${st.detail}`].filter(Boolean).join('\n') || 'The software updates for this FSA aren’t finished yet',
      text: 'Conditions not met' }), retrieve('Try again', 'ghost small')];
  } else if (st.status === 'needs-dealer') {
    const input = el('input', { class: 'o-odo upper', placeholder: 'USA12345', maxlength: '12', title: 'Warranty Dealer Code (WDC)' });
    const save = async () => {
      const code = input.value.trim();
      if (!code) { input.focus(); return; }
      if (await api.setDealerCode(code)) { svcState.delete(num); getSvc(num); } else toast('That dealer code doesn’t look right');
    };
    input.addEventListener('keydown', (e) => { if (e.key === 'Enter') save(); });
    kids = [el('span', { class: 'o-svc-ask', text: 'Your Warranty Dealer Code:' }), input, el('button', { class: 'primary small', text: 'Save', onclick: save })];
    setTimeout(() => input.focus(), 0);
  } else if (st.status === 'error') {
    kids = [el('span', { class: 'o-svc-err', title: st.message || '', text: st.needsLogin ? 'Sign in to PTS' : (st.usePts ? 'Use PTS for this one' : 'Couldn’t get SVC') }),
      st.needsLogin ? el('button', { class: 'ghost small', text: 'Sign in', onclick: async () => { await api.signIn(); getSvc(num); } })
        : st.usePts ? el('button', { class: 'ghost small', text: 'Open form', onclick: () => (st.url ? api.openUrl(st.url) : api.openPts()) })
          : retrieve('Try again', 'ghost small')];
  } else kids = [retrieve()];
  slot.replaceChildren(...[...kids, some].filter(Boolean));
}

async function getSvc(num) {
  svcState.set(num, { status: 'loading' });
  renderSvc(num);
  let res;
  try { res = await api.svc(num); } catch (e) { res = { error: true, message: String(e?.message || e) }; }
  if (res?.error) res = { status: 'error', message: res.message, needsLogin: !!res.needsLogin };
  svcState.set(num, res || { status: 'error' });
  if (res?.status === 'ok') copy(res.code);
  renderSvc(num);
}

// ---------- warning messages ----------
function readWarnings(doc) {
  const body = doc.querySelector('#warnmsg-panel .panel-body');
  const title = headerText(doc, 'warnmsg-panel');
  const lines = [];
  if (body) {
    const items = body.querySelectorAll('li, p, label, .row > div');
    const seen = new Set();
    (items.length ? [...items] : [body]).forEach((n) => {
      if (n.querySelector('li, p, label') && items.length) return; // only the innermost pieces
      const t = clean(n.textContent);
      if (t && !seen.has(t)) { seen.add(t); lines.push(t); }
    });
  }
  // OASIS hides the "imported by HUSM" notice unless it applies
  const husm = doc.getElementById('husmDiv');
  if (husm && !husm.hasAttribute('data-hidden')) {
    const t = clean(doc.getElementById('husmMessage')?.textContent);
    if (t) lines.push([t, clean(doc.getElementById('husmInformation')?.textContent)].filter(Boolean).join(' — '));
  }
  return { found: !!body, title, lines };
}
function renderWarnings(warn) {
  const box = el('section', { class: 'o-section', id: 'warnings' }, el('h2', { text: 'Warning messages' }));
  if (!warn.lines.length) {
    box.append(el('div', { class: 'o-card o-msg ok' }, el('b', { text: '✓ ' }),
      warn.found ? 'No warning messages for this VIN.' : "OASIS didn't include warning messages for this VIN."));
    return box;
  }
  box.append(el('div', { class: 'o-card o-warns' }, ...warn.lines.map((t) => el('div', { class: 'o-warn', text: t }))));
  return box;
}

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

// ---------- warranty coverage ----------
// Each coverage ("5 years / 60,000 Miles") is checked against the warranty start date (build date if there
// isn't one) and the odometer: connected vehicle data when there is some, or a reading typed in here.
let warranty = null;
let typedOdometer = null;
const MONTHS = { jan: 0, feb: 1, mar: 2, apr: 3, may: 4, jun: 5, jul: 6, aug: 7, sep: 8, oct: 9, nov: 10, dec: 11 };
function parseDate(t) {
  const m = String(t || '').match(/(\d{1,2})[-\s/]([A-Za-z]{3,})[-\s/,]+(\d{4})/);
  if (m && MONTHS[m[2].slice(0, 3).toLowerCase()] != null) return new Date(Number(m[3]), MONTHS[m[2].slice(0, 3).toLowerCase()], Number(m[1]));
  const d = new Date(t);
  return isNaN(d) ? null : d;
}
const fmtDate = (d) => d.toLocaleDateString([], { month: 'short', day: 'numeric', year: 'numeric' });

function pairsIn(panel) {
  const out = [];
  panel?.querySelectorAll('label.label').forEach((lab) => {
    const k = clean(lab.textContent).replace(/:$/, '').toUpperCase();
    let v = lab.nextElementSibling;
    while (v && !/^(LABEL|SPAN|P|DIV)$/.test(v.tagName)) v = v.nextElementSibling;
    if (k) out.push([k, clean(v?.textContent)]);
  });
  return out;
}

function readWarranty(doc) {
  const general = new Map(pairsIn(doc.getElementById('gwi-panel')));
  const months = clean(doc.querySelector('[data-results="gwi-panel"]')?.closest('.panel-heading')?.textContent).match(/MONTHS IN SERVICE:\s*(\d+)/i);
  const coverages = [];
  let cur = null;
  for (const [k, v] of pairsIn(doc.getElementById('warantycov-panel'))) {
    if (k === 'COVERAGE TYPE') { cur = { type: v, description: '', info: '' }; coverages.push(cur); } else if (cur && k === 'COVERAGE DESCRIPTION') cur.description = v;
    else if (cur && k === 'ADDITIONAL INFO') cur.info = v;
  }
  const startText = general.get('WARRANTY START DATE') || '';
  const buildText = general.get('BUILD DATE') || '';
  return { start: parseDate(startText), startText, build: parseDate(buildText), buildText, release: general.get('RELEASE DATE') || '',
    saleMileage: general.get('SALE MILEAGE') || '', monthsInService: months ? Number(months[1]) : null, coverages };
}

// "5 years / 60,000 Miles", "36 Months/36,000 Miles", "8 years / 100,000 miles", "unlimited miles" …
function readLimits(text) {
  const t = String(text || '');
  const y = t.match(/(\d+(?:\.\d+)?)\s*(years?|yrs?|months?|mos?)\b/i);
  const mi = t.match(/(unlimited|[\d,]+)\s*(miles?|mi\b|kilomet(?:er|re)s?|km)/i);
  const months = y ? (/^m/i.test(y[2]) ? Number(y[1]) : Number(y[1]) * 12) : null;
  let miles = null;
  let unlimited = false;
  if (mi) {
    if (/unlimited/i.test(mi[1])) unlimited = true;
    else miles = Number(mi[1].replace(/,/g, '')) / (/^k/i.test(mi[2]) ? KM_PER_MILE : 1);
  }
  return { months, miles, unlimited };
}

function checkCoverage(c, start, odo) {
  const lim = readLimits(c.description);
  const now = new Date();
  let ends = null; let timeLeft = null; let timeOut = false;
  if (start && lim.months != null) {
    ends = new Date(start); ends.setMonth(ends.getMonth() + lim.months);
    timeOut = now > ends;
    timeLeft = Math.round((ends - now) / 86400000);
  }
  let milesLeft = null; let milesOut = false;
  if (odo != null && lim.miles != null) { milesLeft = Math.round(lim.miles - odo); milesOut = milesLeft < 0; }
  let status; let text;
  if (lim.months == null && lim.miles == null && !lim.unlimited) { status = 'unknown'; text = 'Check the description'; }
  else if (timeOut || milesOut) {
    status = 'out';
    text = [timeOut && `Out by time (ended ${fmtDate(ends)})`, milesOut && `Out by mileage (${Math.abs(milesLeft).toLocaleString()} mi over)`].filter(Boolean).join(' · ');
  } else if (lim.miles != null && odo == null) {
    status = 'maybe';
    text = `In time${ends ? ` until ${fmtDate(ends)}` : ''}, mileage unknown: enter the odometer above`;
  } else {
    status = 'in';
    const parts = [];
    if (ends) parts.push(`${timeLeft.toLocaleString()} days left (until ${fmtDate(ends)})`);
    if (milesLeft != null) parts.push(`${milesLeft.toLocaleString()} mi left`);
    else if (lim.unlimited) parts.push('unlimited miles');
    text = parts.join(' · ') || 'Covered';
  }
  return { status, text };
}

function renderWarranty() {
  const box = $('warranty');
  if (!box || !warranty) return;
  const w = warranty;
  const start = w.start || w.build;
  const odo = typedOdometer ?? cvInfo?.odometerMiles ?? null;
  const odoSource = typedOdometer != null ? 'typed in' : cvInfo?.odometerMiles != null ? `connected vehicle${cvInfo.when ? `, ${fmtTime(cvInfo.when)}` : ''}` : '';
  const input = el('input', { class: 'o-odo', type: 'text', inputmode: 'numeric', placeholder: 'Odometer (mi)', value: odo != null ? String(odo) : '', title: 'Odometer reading used for the mileage check' });
  input.addEventListener('change', () => {
    const n = Number(String(input.value).replace(/[^\d.]/g, ''));
    typedOdometer = input.value.trim() && !isNaN(n) ? Math.round(n) : null;
    renderWarranty();
  });
  const facts = el('div', { class: 'o-facts' },
    el('span', {}, 'Warranty start ', el('b', { text: w.start ? fmtDate(w.start) : (w.startText || '—') })),
    el('span', {}, 'Build ', el('b', { text: w.build ? fmtDate(w.build) : (w.buildText || '—') })),
    w.monthsInService != null ? el('span', {}, 'In service ', el('b', { text: `${w.monthsInService} months` })) : null,
    el('span', { class: 'o-odo-wrap' }, 'Odometer ', input, el('span', { class: 'muted small', text: odoSource ? ` (${odoSource})` : '' })));
  const note = !w.start && w.build ? el('div', { class: 'o-card o-msg', text: 'No warranty start date in OASIS: time is counted from the build date.' }) : null;
  const rows = w.coverages.map((c) => {
    const r = checkCoverage(c, start, odo);
    return el('div', { class: `o-cov ${r.status}` },
      el('div', { class: 'o-cov-head' }, el('span', { class: 'o-badge', text: { in: 'Covered', out: 'Expired', maybe: 'Check miles', unknown: 'See terms' }[r.status] }),
        el('b', { text: c.type || 'Coverage' }), el('span', { class: 'muted', text: c.description })),
      el('div', { class: 'o-cov-text', text: r.text }),
      c.info ? el('details', { class: 'o-cov-info' }, el('summary', { text: 'Additional info' }), el('p', { text: c.info })) : null);
  });
  box.replaceChildren(...[el('h2', { text: 'Warranty coverage' }), facts, note,
    rows.length ? el('div', { class: 'o-card o-covs' }, ...rows) : el('div', { class: 'o-card o-msg', text: 'OASIS listed no warranty coverage for this VIN.' })].filter(Boolean));
}

// ---------- connected vehicle data ----------
const KM_PER_MILE = 1.60934;
let cvInfo = null; // kept for warranty coverage (odometer)
const fmtTime = (t) => {
  if (!t) return '';
  const d = new Date(String(t).replace(' ', 'T') + (/[zZ]|[+-]\d\d:?\d\d$/.test(t) ? '' : 'Z'));
  return isNaN(d) ? String(t) : d.toLocaleString([], { month: 'short', day: 'numeric', year: 'numeric', hour: 'numeric', minute: '2-digit' });
};
// LOW_PRESSURE → Low pressure
const words = (v) => (v == null || v === '' ? null : /^[A-Z0-9_]+$/.test(String(v)) && /[A-Z]{2}/.test(String(v)) ? String(v).toLowerCase().replace(/_/g, ' ').replace(/^./, (c) => c.toUpperCase()) : v);
const num = (m) => (m && m.value != null && m.value !== -1000 && !isNaN(Number(m.value)) ? Number(m.value) : null);
const pct = (m) => { const n = num(m); return n == null || n < 0 || n > 100 ? null : `${Math.floor(n)}%`; };

function statusRow(label, value, when, cls) {
  return el('div', { class: 'o-row' }, el('span', { class: 'k', text: label }),
    value != null && value !== ''
      ? el('span', { class: `v ${cls || ''}`, title: when ? `Reported ${fmtTime(when)}` : 'Click to copy', onclick: () => copy(String(value)) },
        String(value), when ? el('span', { class: 'when', text: fmtTime(when) }) : null)
      : el('span', { class: 'v empty', text: '—' }));
}

function renderCv(res) {
  const box = $('cv');
  if (!box) return;
  const head = el('h2', {}, 'Connected vehicle data', el('button', { class: 'ghost small inline', text: '↻', title: 'Ask for the latest data', onclick: loadCv }));
  if (res?.error) {
    box.replaceChildren(head, el('div', { class: 'o-card o-msg' }, res.needsLogin ? 'Sign in to PTS to see connected vehicle data.' : `Couldn't get connected vehicle data (${res.message}).`));
    return;
  }
  const cap = res.capability || {};
  const data = res.data || {};
  const miles = (res.country || 'USA') === 'USA';
  const dist = (km) => (km == null ? null : miles ? `${Math.floor(km / KM_PER_MILE).toLocaleString()} mi` : `${Math.floor(km).toLocaleString()} km`);
  const notes = [];
  if (cap.dealerAgreed && String(cap.dealerAgreed).toLowerCase() !== 'yes') notes.push('Your dealership hasn’t accepted the connected vehicle terms in PTS.');
  if (cap.privacyResponse && cap.privacyResponse.auth && String(cap.privacyResponse.auth).toLowerCase() !== 'yes') notes.push("The customer's privacy settings don't allow connected vehicle data.");
  if (cap.cvasPassed && String(cap.cvasPassed).toLowerCase() !== 'yes') notes.push('This vehicle isn’t authorized to send data (modem activation or Ford app settings).');

  const m = data.tmcData?.metrics || null;
  const vs = data.vstateData?.vehiclestatus || null;
  const cards = [];
  if (m || vs) {
    const odoKm = num(m?.odometer) ?? num(vs?.odometer);
    cvInfo = { odometerKm: odoKm, odometerMiles: odoKm == null ? null : Math.floor(odoKm / KM_PER_MILE), when: m?.odometer?.updateTime || vs?.odometer?.timestamp };
    renderWarranty();
    const rows = [
      statusRow('Odometer', dist(odoKm), m?.odometer?.updateTime),
      statusRow('Fuel level', pct(m?.fuelLevel) ?? pct(vs?.fuel?.fuelLevel ? { value: vs.fuel.fuelLevel } : null), m?.fuelLevel?.updateTime),
      statusRow('Fuel range', dist(num(m?.fuelRange)), m?.fuelRange?.updateTime),
      statusRow('Oil life', pct(m?.oilLifeRemaining), m?.oilLifeRemaining?.updateTime),
      statusRow('12V battery', pct(m?.batteryStateOfCharge), m?.batteryStateOfCharge?.updateTime),
      statusRow('Tire pressure', words(m?.tirePressureSystemStatus?.value), m?.tirePressureSystemStatus?.updateTime),
    ];
    if (m?.xevBatteryStateOfCharge) rows.push(statusRow('High voltage battery', pct(m.xevBatteryStateOfCharge), m.xevBatteryStateOfCharge.updateTime));
    if (m?.xevBatteryRange) rows.push(statusRow('Electric range', dist(num(m.xevBatteryRange)), m.xevBatteryRange.updateTime));
    if (m?.dieselExhaustFluidLevel) rows.push(statusRow('Diesel exhaust fluid', pct(m.dieselExhaustFluidLevel), m.dieselExhaustFluidLevel.updateTime));
    cards.push(el('div', { class: 'o-card' }, el('h3', { text: 'Vehicle status' }), ...rows));
  }

  // DTCs and health alerts from the latest report
  const diag = data.diagCurrentData;
  if (diag) {
    const dtcs = (diag.Metadata?.dtcs || []).filter((x) => x && x.dtcCode);
    const alerts = [];
    for (const g of diag.vhAlerts || []) for (const a of g?.alerts || []) alerts.push(a);
    const rows = [];
    if (diag.eventTime) rows.push(statusRow('Reported', fmtTime(diag.eventTime)));
    rows.push(...dtcs.map((x) => el('div', { class: 'o-row' }, el('span', { class: 'k code', text: x.ecuId || x.ecuName || '' }),
      el('span', { class: 'v code', title: 'Click to copy', onclick: () => copy(x.dtcCode) }, x.dtcCode,
        x.dtcDescription || x.description ? el('span', { class: 'when', text: x.dtcDescription || x.description }) : null))));
    rows.push(...alerts.map((a) => el('div', { class: 'o-row' }, el('span', { class: 'k', text: 'Alert' }),
      el('span', { class: 'v', text: [a.alertName || a.title || a.name, a.alertType && a.alertType !== 'NEW_ALERT' ? `(${a.alertType})` : ''].filter(Boolean).join(' ') || JSON.stringify(a).slice(0, 120) }))));
    if (!dtcs.length && !alerts.length) rows.push(el('div', { class: 'o-row' }, el('span', { class: 'k', text: 'Codes' }), el('span', { class: 'v ok', text: 'None reported' })));
    cards.push(el('div', { class: 'o-card' }, el('h3', { text: 'DTCs & health alerts' }), ...rows));
  }

  // Everything Ford sent in the status report, in case something above is missing
  if (m) {
    const all = Object.entries(m).filter(([, x]) => x && typeof x === 'object' && 'value' in x)
      .map(([k, x]) => statusRow(k.replace(/([a-z])([A-Z0-9])/g, '$1 $2').replace(/^./, (c) => c.toUpperCase()),
        typeof x.value === 'object' ? JSON.stringify(x.value) : words(x.value), x.updateTime));
    if (all.length) cards.push(el('details', { class: 'o-card o-more' }, el('summary', { text: `All values Ford sent (${all.length})` }), ...all));
  }

  if (!cards.length) {
    notes.push(res.status && res.status >= 700 ? 'Connected vehicle data isn’t available for this vehicle.' : 'No connected vehicle data for this vehicle.');
  }
  box.replaceChildren(head, ...notes.map((t) => el('div', { class: 'o-card o-msg', text: t })), cards.length ? el('div', { class: 'o-grid' }, ...cards) : null);
}

async function loadCv() {
  const box = $('cv');
  if (!box) return;
  box.replaceChildren(el('h2', { text: 'Connected vehicle data' }), el('div', { class: 'o-card o-msg' }, el('div', { class: 'spinner' }), 'Asking the vehicle’s latest data…'));
  let res;
  try { res = await api.cv(); } catch (e) { res = { error: true, message: String(e?.message || e) }; }
  try { renderCv(res); } catch (e) {
    api.log(`connected vehicle: ${e && e.stack || e}`);
    renderCv({ error: true, message: String(e?.message || e) });
  }
}

$('btn-refresh').onclick = load;
$('btn-full').onclick = () => api.openPts();
load();