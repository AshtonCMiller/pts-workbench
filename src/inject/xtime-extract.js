// Runs inside the Xtime Inspect frame (inspect.app.coxautoinc.com).
// Fetches the same dashboard data Xtime's own page loads, using your existing login,
// and returns ONLY: RO, tag, year/make/model, VIN, mode and waiting flags for ONE technician.
// Customer information never leaves the page.
(async (TECH_ID, API_PATH) => {
  const get = (obj, name) => {
    if (!obj || typeof obj !== 'object') return undefined;
    const key = Object.keys(obj).find((k) => k.toLowerCase() === name.toLowerCase());
    return key === undefined ? undefined : obj[key];
  };

  let data;
  try {
    const res = await fetch(API_PATH, { credentials: 'include', headers: { Accept: 'application/json' } });
    if (res.status === 401 || res.status === 403) return { ok: false, expired: true, error: 'Xtime session expired' };
    if (!res.ok) return { ok: false, error: `Xtime returned HTTP ${res.status}` };
    // An expired session usually comes back as a redirect to the sign-in page (HTML, not data)
    const type = res.headers.get('content-type') || '';
    if (res.redirected || !/json/i.test(type)) return { ok: false, expired: true, error: 'Xtime session expired' };
    data = await res.json();
  } catch (e) {
    return { ok: false, error: `Xtime request failed: ${e.message}` };
  }

  // ROs are grouped under DashboardModes; find them at any depth and de-duplicate.
  const all = new Map();
  const walk = (node) => {
    if (!node || typeof node !== 'object') return;
    if (Array.isArray(node)) { node.forEach(walk); return; }
    const roNum = get(node, 'RONumber');
    if (roNum !== undefined && get(node, 'Vehicle') !== undefined) {
      if (!all.has(String(roNum))) all.set(String(roNum), node);
      return;
    }
    for (const v of Object.values(node)) if (v && typeof v === 'object') walk(v);
  };
  walk(data);

  const mine = [];
  for (const ro of all.values()) {
    if (String(get(ro, 'TechnicianID')) !== String(TECH_ID)) continue;
    let v = get(ro, 'Vehicle');
    if (Array.isArray(v)) v = v[0];
    const mode = Number(get(ro, 'CurrentMode'));
    mine.push({
      ro: String(get(ro, 'RONumber') ?? ''),
      tag: String(get(ro, 'TagNumber') ?? ''),
      year: String(get(v, 'year') ?? ''),
      make: String(get(v, 'make') ?? ''),
      model: String(get(v, 'model') ?? ''),
      vin: String(get(v, 'VIN') ?? '').toUpperCase(),
      mode: Number.isFinite(mode) ? mode : null,
      waitingParts: get(ro, 'WaitingForParts') === true,
      waitingCustomer: get(ro, 'WaitingForCustomer') === true,
      waitingWarranty: get(ro, 'WaitingForWarranty') === true,
    });
  }
  // Technician's name for the window title, from the first of your ROs that carries one
  // (TechnicianName / TechName / Technician: { FirstName, LastName } …).
  const nameFrom = (ro) => {
    for (const [k, v] of Object.entries(ro)) {
      if (/^tech(nician)?_?(full_?)?name$/i.test(k) && typeof v === 'string' && v.trim()) return v.trim();
    }
    const t = get(ro, 'Technician');
    if (t && typeof t === 'object') {
      const full = get(t, 'FullName') || get(t, 'Name') || get(t, 'DisplayName');
      if (typeof full === 'string' && full.trim()) return full.trim();
      const parts = [get(t, 'FirstName'), get(t, 'LastName')].filter((x) => typeof x === 'string' && x.trim());
      if (parts.length) return parts.join(' ').trim();
    }
    return '';
  };
  let techName = '';
  for (const ro of all.values()) {
    if (String(get(ro, 'TechnicianID')) !== String(TECH_ID)) continue;
    techName = nameFrom(ro);
    if (techName) break;
  }
  return { ok: true, total: all.size, mine, techName };
})