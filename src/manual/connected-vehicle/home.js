// Connected Vehicle › Home: the vehicle's latest status (odometer, fuel, oil life, 12V battery, tire pressures, DTCs and
// alerts it's reporting now) and its health history: each report with DTCs, alerts, odometer and battery, newest first.
// Ford's status request already includes the last 60 days or so of history; "Show older" asks for 60 days more at a time.
'use strict';

let cvRes = null;   // Ford's status answer (see manual.js loadConnectedVehicle)
const hist = { events: [], keys: new Set(), oldest: null, loading: false, all: false, modules: {}, error: '' };
const HISTORY_DAYS = 60;    // per request, as Ford's page asks
const HISTORY_MAX = 365;    // furthest back "Show older" goes

// ---------- the latest status ----------
function tireCard(m, vs) {
  const t = vs?.TPMS || m?.TPMS || null;
  const spots = [ // [label, field in Ford's vehicle status, wheel name in its newer report]
    ['Left front', 'leftFrontTire', 'FRONT_LEFT'], ['Right front', 'rightFrontTire', 'FRONT_RIGHT'],
    ['Left rear', 'outerLeftRearTire', 'REAR_LEFT'], ['Right rear', 'outerRightRearTire', 'REAR_RIGHT'],
    ['Inner left rear', 'innerLeftRearTire', 'INNER_REAR_LEFT'], ['Inner right rear', 'innerRightRearTire', 'INNER_REAR_RIGHT'],
  ];
  const list = Array.isArray(m?.tirePressure) ? m.tirePressure : [];
  const tires = spots.map(([label, field, wheel]) => {
    const fromList = list.find((x) => String(x?.vehicleWheel || x?.wheel || '').toUpperCase() === wheel);
    const kpa = num(t?.[`${field}Pressure`]) ?? num(fromList);
    const status = t?.[`${field}Status`]?.value || '';
    return { label, value: tire(kpa), low: /low/i.test(status), status, when: t?.[`${field}Pressure`]?.timestamp || fromList?.updateTime };
  }).filter((x, i) => i < 4 || x.value != null); // the inner rear tires only on duallys
  if (!tires.some((x) => x.value != null)) return null;
  const recF = tire(num(t?.recommendedFrontTirePressure));
  const recR = tire(num(t?.recommendedRearTirePressure));
  const sys = t?.tirePressureSystemStatus?.value || m?.tirePressureSystemStatus?.value || (Array.isArray(m?.tirePressureSystemStatus) ? m.tirePressureSystemStatus[0]?.value : '');
  const front = tires.slice(0, 2);
  const rear = tires.slice(2);
  const box = (x) => el('div', { class: `cv-tire${x.low ? ' low' : ''}`, title: x.status ? `${words(x.status)}${x.when ? ` · ${fmtTime(x.when)}` : ''}` : '' },
    el('span', { class: 'n', text: x.value == null ? '—' : String(x.value) }), el('span', { class: 'k', text: x.label }));
  return el('div', { class: 'o-card' }, el('h3', { text: `Tire pressure (${tireUnit()})` }),
    el('div', { class: 'cv-tires' }, el('div', { class: 'cv-axle' }, ...front.map(box)), el('div', { class: 'cv-car' }), el('div', { class: 'cv-axle' }, ...rear.map(box))),
    recF != null || recR != null ? el('div', { class: 'cv-rec' }, 'Recommended: ', el('b', { text: `Front ${recF ?? '—'}` }), ' · ', el('b', { text: `Rear ${recR ?? '—'}` })) : null,
    sys && !/normal|^ok$/i.test(String(sys)) ? el('div', { class: 'cv-rec bad', text: `System: ${words(sys)}` }) : null);
}

function renderStatus(res) {
  const cap = res.capability || {};
  const data = res.data || {};
  const notes = [];
  if (cap.dealerAgreed && String(cap.dealerAgreed).toLowerCase() !== 'yes') notes.push('Your dealership hasn’t accepted the connected vehicle terms in PTS.');
  if (cap.privacyResponse && cap.privacyResponse.auth && String(cap.privacyResponse.auth).toLowerCase() !== 'yes') notes.push("The customer's privacy settings don't allow connected vehicle data.");
  if (cap.cvasPassed && String(cap.cvasPassed).toLowerCase() !== 'yes') notes.push('This vehicle isn’t authorized to send data (modem activation or Ford app settings).');

  const m = data.tmcData?.metrics || null;
  const vs = data.vstateData?.vehiclestatus || null;
  const cards = [];
  let latest = null;
  if (m || vs) {
    const when = (x) => x?.updateTime || x?.timestamp;
    for (const x of [m?.odometer, m?.fuelLevel, m?.batteryStateOfCharge, m?.oilLifeRemaining]) {
      const d = toDate(when(x));
      if (d && (!latest || d > latest)) latest = d;
    }
    const odoKm = num(m?.odometer) ?? num(vs?.odometer);
    const fuel = pct(m?.fuelLevel) ?? pct(vs?.fuel?.fuelLevel);
    const range = dist(num(m?.fuelRange) ?? num(vs?.fuel?.distanceToEmpty));
    const volts = num(m?.batteryVoltage) ?? num(vs?.battery?.batteryStatusActual);
    const rows = [
      statusRow('Odometer', dist(odoKm), when(m?.odometer)),
      statusRow('Fuel', [fuel, range ? `${range} range` : null].filter(Boolean).join(' · ') || null, when(m?.fuelLevel)),
      statusRow('Oil life', pct(m?.oilLifeRemaining) ?? pct(vs?.oil?.oilLifeActual), when(m?.oilLifeRemaining)),
      statusRow('12V battery', [pct(m?.batteryStateOfCharge), volts != null ? `${volts.toFixed(1)} V` : null].filter(Boolean).join(' · ') || null, when(m?.batteryStateOfCharge)),
    ];
    if (m?.xevBatteryStateOfCharge) rows.push(statusRow('High voltage battery', pct(m.xevBatteryStateOfCharge), when(m.xevBatteryStateOfCharge)));
    if (m?.xevBatteryRange) rows.push(statusRow('Electric range', dist(num(m.xevBatteryRange)), when(m.xevBatteryRange)));
    if (m?.dieselExhaustFluidLevel) rows.push(statusRow('Diesel exhaust fluid', pct(m.dieselExhaustFluidLevel), when(m.dieselExhaustFluidLevel)));
    cards.push(el('div', { class: 'o-card' }, el('h3', { text: 'Vehicle status' }), ...rows));
    const t = tireCard(m, vs);
    if (t) cards.push(t);
  }

  // DTCs and alerts in the latest report
  const diag = data.diagCurrentData;
  if (diag) {
    const dtcs = (diag.Metadata?.dtcs || diag.dtcs || []).filter((x) => x && x.dtcCode);
    const alerts = [];
    for (const g of diag.vhAlerts || []) for (const a of g?.alerts || []) alerts.push(a);
    const rows = [];
    if (diag.eventTime) rows.push(statusRow('Reported', fmtTime(diag.eventTime)));
    rows.push(...dtcs.map((x) => dtcRow(x)));
    rows.push(...alerts.map((a) => alertRow(a)));
    if (!dtcs.length && !alerts.length) rows.push(el('div', { class: 'o-row' }, el('span', { class: 'k', text: 'Codes' }), el('span', { class: 'v ok', text: 'None reported' })));
    cards.push(el('div', { class: 'o-card' }, el('h3', { text: 'DTCs & alerts now' }), ...rows));
  }
  if (!cards.length) notes.push(res.status && res.status >= 700 ? 'Connected vehicle data isn’t available for this vehicle.' : 'Ford has no connected vehicle data for this vehicle.');

  return el('section', { class: 'o-section' },
    el('h2', {}, 'Connected vehicle status', latest ? el('span', { class: 'cv-when', text: `Last update ${fmtTime(latest)}` }) : null),
    ...notes.map((t) => msg(t)), cards.length ? el('div', { class: 'o-grid' }, ...cards) : null);
}

// ---------- alerts (now and in the history) ----------
// (each DTC's row is in dtcs.js)
function alertRow(a) {
  const what = a.messageDesc || a.alertName || a.title || words(a.symptomKey) || a.wilCode || '';
  const kind = /clear/i.test(a.alertType || '') ? 'Clear alert' : /new/i.test(a.alertType || '') ? 'New alert' : words(a.alertType) || 'Alert';
  const urgent = /high|immediate|critical/i.test(a.urgency || '');
  return el('div', { class: `cv-alert${/clear/i.test(a.alertType || '') ? ' clear' : ''}${urgent ? ' urgent' : ''}` },
    el('span', { class: 'kind', text: kind }), el('span', { text: what || '(no description)' }),
    a.customerActionKey ? el('span', { class: 'desc', text: words(a.customerActionKey) }) : null);
}

// ---------- the health history ----------
function addEvents(list) {
  for (const day of list || []) {
    for (const ev of day?.vhDataOnEventTime || []) {
      const key = `${ev.eventTime}|${ev.odometer}`;
      if (hist.keys.has(key)) continue;
      hist.keys.add(key);
      hist.events.push({ ...ev, eventDate: day.eventDate });
    }
  }
  hist.events.sort((a, b) => (toDate(b.eventTime || b.eventDate)?.getTime() || 0) - (toDate(a.eventTime || a.eventDate)?.getTime() || 0));
}

async function loadOlder() {
  if (hist.loading) return;
  const to = new Date(hist.oldest || Date.now());
  if (hist.oldest) to.setDate(to.getDate() - 1);
  const from = new Date(to);
  from.setDate(from.getDate() - HISTORY_DAYS);
  hist.loading = true; hist.error = '';
  drawHistory();
  const res = await ask(() => api.cvHistory(ymd(from), ymd(to)));
  hist.loading = false;
  if (res?.error) hist.error = res.needsLogin ? 'Sign in to PTS to see more history.' : `Couldn't get older history (${res.message}).`;
  else {
    hist.oldest = from;
    if (res.modules && Object.keys(res.modules).length) hist.modules = res.modules;
    addEvents(res.list);
  }
  drawHistory();
}

function eventRow(ev) {
  const dtcs = (ev.dtcs || []).filter((x) => x?.dtcCode);
  const alerts = (ev.alerts || []).filter(Boolean);
  const volts = num(ev.batteryVoltage);
  const soc = num(ev.batterySoc);
  const facts = [
    ev.odometer != null && ev.odometer !== '' ? `${dist(Number(ev.odometer))}` : null,
    volts != null || soc != null ? `12V ${[volts != null ? `${volts.toFixed(1)} V` : null, soc != null ? `${Math.floor(soc)}%` : null].filter(Boolean).join(' · ')}` : null,
  ].filter(Boolean);
  return el('div', { class: 'cv-event' },
    el('div', { class: 'cv-event-head' },
      el('span', { class: 'when', text: fmtTime(ev.eventTime || ev.eventDate) }),
      ...facts.map((f) => el('span', { class: 'fact', text: f })),
      ev.otaChange ? el('span', { class: 'tag', text: 'Software changed' }) : null),
    dtcs.length || alerts.length ? el('div', { class: 'cv-event-body' }, ...dtcs.map(dtcRow), ...alerts.map(alertRow)) : null);
}

function drawHistory() {
  drawRecent();
  describeNew(); // new DTCs in the history: ask what they mean
  const box = $('cv-history');
  if (!box) return;
  const shown = hist.all ? hist.events : hist.events.filter((e) => (e.dtcs || []).some((x) => x?.dtcCode) || (e.alerts || []).length);
  const back = hist.oldest ? Math.round((Date.now() - hist.oldest) / 86400000) : null;
  const canOlder = !hist.oldest || back < HISTORY_MAX;
  box.replaceChildren(...[
    el('h2', {}, 'Health history (DTCs & alerts)',
      el('label', { class: 'cv-check' }, el('input', { type: 'checkbox', checked: hist.all ? '' : null, onchange: (e) => { hist.all = e.target.checked; drawHistory(); } }), 'Show every report')),
    back ? el('div', { class: 'cv-range', text: `Reports from the last ${back} days · ${hist.events.length} in all` } ) : null,
    shown.length ? el('div', { class: 'o-card cv-events' }, ...shown.map(eventRow))
      : !hist.loading ? msg(hist.events.length ? 'No DTCs or alerts in these reports.' : 'No health reports in this time.') : null,
    hist.error ? msg(hist.error) : null,
    hist.loading ? waiting('Asking Ford for older history…')
      : canOlder ? el('button', { class: 'ghost cv-older', text: `Show older (${HISTORY_DAYS} days more)`, onclick: loadOlder }) : null,
  ].filter(Boolean));
}

// ---------- Home ----------
async function showHome(fresh) {
  const body = $('cv-body');
  if (fresh || !cvRes) {
    body.replaceChildren(el('div', { class: 'state' }, el('div', { class: 'spinner' }), 'Asking Ford for the vehicle’s latest data…'));
    cvRes = await ask(() => api.cv());
    Object.assign(hist, { events: [], keys: new Set(), oldest: null, error: '' });
    arts = null; artsFailed = '';
    if (!cvRes?.error) {
      miles = (cvRes.country || 'USA') === 'USA';
      const h = cvRes.data?.diagHistoryData;
      addEvents(h?.vhDataList);
      if (h?.vhDataList?.length) { const d = new Date(); d.setDate(d.getDate() - HISTORY_DAYS); hist.oldest = toDate(h.fromDate) || d; }
    }
  }
  if (!cvRes?.error && !Object.keys(hist.modules).length) {
    ask(() => api.cvModules()).then((r) => { if (r && !r.error && Object.keys(r).length) { hist.modules = r; if (state.view === 'home') showHome(); } });
  }
  if (!cvRes?.error && arts == null && !loadArticles.running) {
    loadArticles.running = true;
    loadArticles().finally(() => { loadArticles.running = false; });
  }
  if (state.view !== 'home') return;
  if (cvRes?.error) { body.replaceChildren(failed(cvRes, 'connected vehicle data', () => showHome(true))); return; }
  try {
    body.replaceChildren(renderStatus(cvRes), el('section', { class: 'o-section', id: 'cv-recent' }), el('section', { class: 'o-section', id: 'cv-history' }));
    drawHistory();
    drawPickBar();
    if (!hist.oldest && !hist.loading) loadOlder(); // Ford's status had no history: ask for the last 60 days
  } catch (e) {
    api.log(`connected vehicle home: ${e && e.stack || e}`);
    body.replaceChildren(failed({ message: String(e?.message || e) }, 'connected vehicle data', () => showHome(true)));
  }
}
