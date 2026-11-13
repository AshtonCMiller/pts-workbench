// OASIS, warranty coverage: each coverage ("5 years / 60,000 Miles") checked against the warranty start date (the
// build date if there isn't one) and the odometer: connected vehicle data when there is some, or a reading typed in here.
'use strict';

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
