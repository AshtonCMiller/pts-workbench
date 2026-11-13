// Diagnostics, "DTCs from the vehicle": the DTCs the vehicle last reported (connected vehicle data), as buttons that
// search them (one, or all together).
'use strict';

async function loadVehicleDtcs() {
  const btn = $('btn-cv');
  btn.disabled = true; btn.textContent = 'Asking Ford…';
  const res = await api.cv();
  btn.disabled = false; btn.textContent = 'DTCs from the vehicle';
  const chips = $('d-chips');
  if (!res || res.error) { chips.hidden = false; chips.replaceChildren(el('span', { class: 'lbl', text: res?.needsLogin ? 'Sign in to PTS to get the vehicle\'s DTCs.' : `Couldn't get them (${res?.message || 'no answer'}).` })); return; }
  const dtcs = (res.data?.diagCurrentData?.Metadata?.dtcs || []).filter((x) => x && x.dtcCode);
  chips.hidden = false;
  if (!dtcs.length) { chips.replaceChildren(el('span', { class: 'lbl', text: 'The vehicle didn\'t report any DTCs.' })); return; }
  chips.replaceChildren(el('span', { class: 'lbl', text: 'Reported by the vehicle:' }),
    ...dtcs.map((x) => el('button', { class: 'd-chip', type: 'button', title: x.dtcDescription || x.description || 'Search this DTC',
      onclick: () => { $('d-q').value = x.dtcCode; run(x.dtcCode); } }, x.dtcCode, x.ecuName || x.ecuId ? el('small', { text: x.ecuName || x.ecuId }) : null)),
    dtcs.length > 1 ? el('button', { class: 'd-chip', type: 'button', title: 'Search all of them together',
      onclick: () => { const q = [...new Set(dtcs.map((x) => x.dtcCode))].join(' '); $('d-q').value = q; run(q); } }, 'All') : null);
}
