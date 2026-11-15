// Connected Vehicle › Over-the-air updates: the vehicle's latest OTA status, its OTA settings (automatic updates,
// schedule), the updates Ford has lined up for it, updates in progress, and finished ones with their modules, software
// part numbers, release notes and each step Ford recorded. Vehicles without OTA (or without any updates yet) say so.
'use strict';

let otaRes = null;
const OTA_DONE = /^(success|failure|failed|expired|cancel|canceled|cancelled|rejected|completed?)$/i;

const latestStep = (d) => (Array.isArray(d?.statusHistory) && d.statusHistory[0]) || null;
// Ford's wording for a step; some steps only carry a code (OTAM_S0023), so its status word is used instead
const stepText = (s) => {
  if (!s) return '';
  const t = s.translatedVehicleStatus;
  const tidy = (v) => (v && /^[a-z0-9_]+$/.test(v) && v.includes('_') || /^[a-z]+$/.test(v || '') ? v.replace(/_/g, ' ').replace(/^./, (c) => c.toUpperCase()) : v);
  return tidy((t && !/^(OTAM|DCM)_/i.test(t) ? t : '') || words(s.deploymentStatus)) || t || s.rawVehicleStatus || '';
};
// Ford records the same step again every few minutes while waiting: show each change once, with how many times it repeated
function stepRuns(list) {
  const runs = [];
  for (const s of list || []) {
    const key = `${stepText(s)}|${s.rawVehicleStatus || ''}`;
    const last = runs[runs.length - 1];
    if (last && last.key === key) { last.count++; last.first = s; } else runs.push({ key, step: s, first: s, count: 1 });
  }
  return runs;
}
// still waiting on the vehicle (not finished, and not past its expiry date)
function inProcess(d) {
  const last = latestStep(d);
  if (OTA_DONE.test(String(last?.deploymentStatus || '')) || /expired|cancel/i.test(last?.translatedVehicleStatus || '')) return false;
  const exp = toDate(d.deploymentExpirationTime);
  return !(exp && exp < new Date());
}
const stepTime = (s) => s?.timestamp || s?.vehicleTimeStamp || '';
function stepClass(s) {
  const st = String(s?.deploymentStatus || '').toLowerCase();
  if (st === 'success' || s?.rawVehicleStatus === 'OTAM_S1010') return 'ok';
  if (st === 'failure' || s?.rawVehicleStatus === 'OTAM_E1016') return 'bad';
  if (/cancel|expired/i.test(`${st} ${s?.translatedVehicleStatus || ''}`)) return 'gone';
  return 'pending';
}
const yesNo = (v) => (v == null || v === '' ? null : v === true || /^(y|yes|true|on)$/i.test(String(v)) ? 'Yes' : 'No');

// the modules an update changes, with their software part numbers
function updateModules(d) {
  const rows = [];
  for (const g of d?.updateDetails?.groups || []) {
    for (const m of g?.modules || []) {
      const name = m?.moduleInformation?.name || m?.name || '';
      const sw = (m?.softwares || []).map((s) => [s.fromSoftwarePartNumber, s.toSoftwarePartNumber].filter(Boolean).join(' → ')).filter(Boolean);
      rows.push({ group: g.groupId, name, sw });
    }
  }
  return rows;
}

function notesBox(n) {
  if (!n) return null;
  const parts = [];
  if (n.header) parts.push(el('div', { class: 'cv-note-head', text: n.header }));
  if (n.text) parts.push(el('div', { class: 'cv-note-text', text: n.text }));
  for (const g of n.groups || []) {
    parts.push(el('div', { class: 'cv-note-group' }, g.header ? el('b', { text: g.header }) : null, g.text ? el('div', { class: 'cv-note-text', text: g.text }) : null));
  }
  if (n.disclaimer) parts.push(el('div', { class: 'cv-note-text muted', text: n.disclaimer }));
  return parts.length ? el('details', { class: 'cv-notes' }, el('summary', { text: n.header ? `Release notes: ${n.header}` : 'Release notes' }), ...parts) : null;
}

function updateCard(d) {
  const last = latestStep(d);
  const det = d.updateDetails || {};
  const mods = updateModules(d);
  const groups = [...new Set(mods.map((x) => x.group).filter((g) => g != null && g !== ''))];
  const fact = (k, v) => (v == null || v === '' ? null : el('div', { class: 'cv-fact' }, el('span', { class: 'k', text: k }), el('span', { class: 'v', text: String(v) })));
  return el('div', { class: 'o-card cv-update' },
    el('div', { class: 'cv-update-head' },
      el('span', { class: `cv-status ${stepClass(last)}`, text: stepText(last) || 'Status unknown' }),
      el('span', { class: 'when', text: fmtTime(stepTime(last) || d.creationDate) }),
      d.deploymentType ? el('span', { class: 'tag', text: words(d.deploymentType) }) : null),
    el('div', { class: 'cv-facts' },
      fact('Software ID', d.oemCorrelationId || d.deploymentId),
      fact('Group', groups.join(', ')),
      fact('Created', d.creationDate ? fmtTime(d.creationDate) : null),
      fact('Expires', d.deploymentExpirationTime ? fmtTime(d.deploymentExpirationTime) : null),
      fact('Vehicle inhibit', yesNo(d.inhibitRequired)),
      fact('Wi-Fi required', yesNo(det.wifiRequired)),
      fact('Ford Power-Up', d.fordPowerUp),
      fact('Recall', d.recallNumber)),
    mods.length ? el('div', { class: 'cv-mods' }, ...mods.map((x) => el('div', { class: 'cv-mod' },
      el('span', { class: 'mod', text: x.name || '—' }), el('span', { class: 'sw', text: x.sw.join(' · ') || '' })))) : null,
    notesBox(otaRes.notes?.[d.oemCorrelationId]),
    (d.statusHistory || []).length > 1 ? stepsBox(d.statusHistory) : null);
}

function stepsBox(list) {
  const runs = stepRuns(list);
  return el('details', { class: 'cv-steps' }, el('summary', { text: `Every step (${runs.length})` }),
    ...runs.map((r) => el('div', { class: 'cv-step' },
      el('span', { class: 'when', text: fmtTime(stepTime(r.step)) }),
      el('span', { class: `cv-status small ${stepClass(r.step)}`, text: stepText(r.step) || '—' }),
      r.step.rawVehicleStatus ? el('span', { class: 'code', text: r.step.rawVehicleStatus }) : null,
      r.count > 1 ? el('span', { class: 'code', text: `×${r.count} since ${fmtTime(stepTime(r.first))}` }) : null)));
}

function settingsCard() {
  const cfg = cvRes?.data?.tmcData?.metrics?.configurations || {};
  const auto = cfg.automaticSoftwareUpdateOptInSetting;
  const sched = cfg.automaticSoftwareUpdateScheduleSetting;
  if (!auto && !sched) return null;
  const autoV = auto?.value;
  const on = autoV == null ? null : /^(on|true|yes|opt_?in|enabled?)$/i.test(String(autoV)) || autoV === true;
  const type = sched?.value?.scheduleType || '';
  return el('div', { class: 'o-card' }, el('h3', { text: 'Vehicle OTA settings' }),
    statusRow('Automatic software updates', on == null ? null : on ? 'On' : 'Off', auto?.updateTime, on === false ? 'warn' : ''),
    statusRow('Recurring schedule set', sched ? (/recurring/i.test(type) ? 'Yes' : 'No') : null, sched?.updateTime));
}

function sequenceCard(res) {
  const rows = res.sequence || [];
  const body = rows.length
    ? el('table', { class: 'cv-table' },
      el('thead', {}, el('tr', {}, ...['Sequence', 'Module', 'Software update', 'Status'].map((h) => el('th', { text: h })))),
      el('tbody', {}, ...rows.map((x) => el('tr', {},
        el('td', { text: String(x.sequenceId ?? '') }),
        el('td', { text: String(x.ecuNodes || '').replace(/\([^)]*\)/g, '').split(',').map((s) => s.trim()).filter(Boolean).join(', ') }),
        el('td', { class: 'code', text: x.fpu || x.softwareUpdate || '' }),
        el('td', { class: /^qualified$/i.test(x.status) ? 'ok' : 'pending', text: /^qualified$/i.test(x.status) ? 'Qualified' : 'Pending qualification' })))))
    : el('div', { class: 'cv-empty', text: res.sequenceFailed ? `Couldn't get the update sequence (${res.sequenceFailed}).` : 'No upcoming updates lined up for this vehicle.' });
  return el('div', { class: 'o-card' }, el('h3', { text: 'Upcoming updates sequence' }), body);
}

function renderOta(res) {
  const updates = [...(res.updates || [])].sort((a, b) =>
    (toDate(stepTime(latestStep(b)) || b.creationDate)?.getTime() || 0) - (toDate(stepTime(latestStep(a)) || a.creationDate)?.getTime() || 0));
  const inProgress = updates.filter(inProcess);
  const done = updates.filter((d) => !inProgress.includes(d));
  const newest = updates[0];
  const last = latestStep(newest);

  const latestCard = el('div', { class: 'o-card' }, el('h3', { text: 'Latest status' }),
    ...(newest
      ? [statusRow('Status', stepText(last) || 'Unknown', null, stepClass(last)),
        statusRow('Date / time', fmtTime(stepTime(last) || newest.creationDate)),
        statusRow('Update ID', newest.oemCorrelationId || newest.deploymentId),
        statusRow('Release notes', res.notes?.[newest.oemCorrelationId]?.header || null)]
      : [el('div', { class: 'cv-empty', text: res.updatesFailed ? `Couldn't get OTA history (${res.updatesFailed}).` : 'No OTA history for this VIN.' })]));

  const top = [latestCard, settingsCard(), sequenceCard(res)].filter(Boolean);
  return [
    el('section', { class: 'o-section' }, el('h2', { text: 'Over-the-air updates' }), el('div', { class: 'o-grid' }, ...top)),
    el('section', { class: 'o-section' }, el('h2', { text: `Currently in process${inProgress.length ? ` (${inProgress.length})` : ''}` }),
      inProgress.length ? el('div', { class: 'cv-list' }, ...inProgress.map(updateCard)) : msg('No updates in process.')),
    el('section', { class: 'o-section' }, el('h2', { text: `History${done.length ? ` (${done.length})` : ''}` }),
      done.length ? el('div', { class: 'cv-list' }, ...done.map(updateCard))
        : msg(res.updatesFailed ? 'OTA history couldn’t be loaded.' : 'No OTA update attempts for this vehicle in the last 180 days.')),
  ];
}

async function showOta(fresh) {
  const body = $('cv-body');
  if (fresh || !otaRes) {
    body.replaceChildren(el('div', { class: 'state' }, el('div', { class: 'spinner' }), 'Asking Ford for the vehicle’s over-the-air updates…'));
    if (!cvRes) cvRes = await ask(() => api.cv()); // for the vehicle's OTA settings
    otaRes = await ask(() => api.cvOta());
  }
  if (state.view !== 'ota') return;
  if (otaRes?.error) { body.replaceChildren(failed(otaRes, 'over-the-air updates', () => showOta(true))); return; }
  try { body.replaceChildren(...renderOta(otaRes)); } catch (e) {
    api.log(`connected vehicle ota: ${e && e.stack || e}`);
    body.replaceChildren(failed({ message: String(e?.message || e) }, 'over-the-air updates', () => showOta(true)));
  }
}
