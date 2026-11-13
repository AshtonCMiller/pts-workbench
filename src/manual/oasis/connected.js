// OASIS, connected vehicle data: the vehicle's latest reports through Ford's connected vehicle service (odometer,
// fuel, oil life, 12V and high-voltage battery, tire pressure, DEF…). Its odometer is also used for warranty coverage.
'use strict';

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
