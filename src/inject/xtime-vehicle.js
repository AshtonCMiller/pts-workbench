// Runs inside the Xtime Inspect frame. Finds one RO's vehicle (VIN, year, make, model) with your
// existing login: first on the dashboard (it lists every tech's ROs), then on the RO's own page.
// Only the vehicle comes back; customer information never leaves the page.
(async (RO, API_PATH, PATH_TEMPLATE) => {
  const get = (obj, name) => {
    if (!obj || typeof obj !== 'object') return undefined;
    const key = Object.keys(obj).find((k) => k.toLowerCase() === name.toLowerCase());
    return key === undefined ? undefined : obj[key];
  };
  const VAL = { A:1,B:2,C:3,D:4,E:5,F:6,G:7,H:8,J:1,K:2,L:3,M:4,N:5,P:7,R:9,S:2,T:3,U:4,V:5,W:6,X:7,Y:8,Z:9 };
  const W = [8,7,6,5,4,3,2,10,0,9,8,7,6,5,4,3,2];
  const validVin = (v) => {
    if (!/^[A-HJ-NPR-Z0-9]{17}$/.test(v)) return false;
    let sum = 0;
    for (let i = 0; i < 17; i++) sum += (/\d/.test(v[i]) ? +v[i] : VAL[v[i]]) * W[i];
    return v[8] === (sum % 11 === 10 ? 'X' : String(sum % 11));
  };
  const fromVehicle = (v) => {
    if (Array.isArray(v)) v = v[0];
    const vin = String(get(v, 'VIN') ?? '').trim().toUpperCase();
    return {
      vin,
      ymm: [get(v, 'year'), get(v, 'make'), get(v, 'model')].filter(Boolean).join(' '),
    };
  };

  // 1) the dashboard
  try {
    const res = await fetch(API_PATH, { credentials: 'include', cache: 'no-store', headers: { Accept: 'application/json' } });
    const type = res.headers.get('content-type') || '';
    if (res.status === 401 || res.status === 403 || res.redirected || !/json/i.test(type)) {
      return { ok: false, error: 'Xtime session expired' };
    }
    const data = await res.json();
    let match = null;
    const walk = (n) => {
      if (match || !n || typeof n !== 'object') return;
      if (Array.isArray(n)) { n.forEach(walk); return; }
      if (String(get(n, 'RONumber') ?? '') === String(RO) && get(n, 'Vehicle') !== undefined) { match = n; return; }
      Object.values(n).forEach(walk);
    };
    walk(data);
    if (match) {
      const v = fromVehicle(get(match, 'Vehicle'));
      if (v.vin.length === 17) return { ok: true, ...v, from: 'dashboard' };
    }
  } catch { /* try the RO page */ }

  // 2) the RO's own page (works for ROs that have left the dashboard, while Xtime still has them)
  try {
    const res = await fetch(PATH_TEMPLATE.replace('{RO}', encodeURIComponent(RO)), { credentials: 'include' });
    if (res.ok) {
      const text = (await res.text()).replace(/&quot;/g, '"');
      // the VIN from a "VIN": "…" field, else any valid 17-character VIN on the page
      const field = text.match(/["']?vin["']?\s*:\s*["']([A-HJ-NPR-Z0-9]{17})["']/i);
      // A field labelled VIN is trusted as-is (non-North-American VINs have no check digit)
      let vin = field ? field[1].toUpperCase() : '';
      if (!vin) vin = (text.toUpperCase().match(/\b[A-HJ-NPR-Z0-9]{17}\b/g) || []).find(validVin) || '';
      if (vin) {
        const pick = (name) => (text.match(new RegExp(`["']?${name}["']?\\s*:\\s*["']?([^"',}\\]]{1,40})`, 'i')) || [])[1] || '';
        const ymm = [pick('year'), pick('make'), pick('model')].map((x) => x.trim()).filter(Boolean).join(' ');
        return { ok: true, vin, ymm, from: 'page' };
      }
    }
  } catch { /* fall through */ }

  return { ok: false, error: `RO ${RO} wasn't found in Xtime` };
})