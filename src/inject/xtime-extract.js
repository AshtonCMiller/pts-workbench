// Runs inside the Xtime Inspect frame (inspect.app.coxautoinc.com).
// Fetches the same dashboard data Xtime's own page loads, using your existing login,
// and returns ONLY: RO, tag, year/make/model, VIN, mode and waiting flags for ONE technician.
// Customer information never leaves the page.
(async (TECH_ID, API_PATH, EMPLOYEES_PATH) => {
  const get = (obj, name) => {
    if (!obj || typeof obj !== 'object') return undefined;
    const key = Object.keys(obj).find((k) => k.toLowerCase() === name.toLowerCase());
    return key === undefined ? undefined : obj[key];
  };

  let data;
  try {
    const res = await fetch(API_PATH, { credentials: 'include', cache: 'no-store', headers: { Accept: 'application/json' } });
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

  // Dashboard columns for the app's own Xtime dashboard. Field names are matched loosely (Xtime's exact
  // names aren't documented); the list of names (no values) comes back too, to fix any that don't match.
  const pick = (obj, ...patterns) => {
    if (!obj || typeof obj !== 'object') return undefined;
    for (const re of patterns) {
      const key = Object.keys(obj).find((k) => re.test(k) && obj[k] !== null && obj[k] !== '' && typeof obj[k] !== 'object');
      if (key !== undefined) return obj[key];
    }
    return undefined;
  };
  const sub = (obj, re) => {
    if (!obj || typeof obj !== 'object') return null;
    const key = Object.keys(obj).find((k) => re.test(k) && obj[k] && typeof obj[k] === 'object' && !Array.isArray(obj[k]));
    return key ? obj[key] : null;
  };
  const flag = (v) => v === true || v === 1 || /^(true|y|yes|1)$/i.test(String(v ?? ''));
  // Advisor names, as Xtime shows them: number (or ID) → "First Last", from the store's employee list
  // (loaded once per page; only names are used)
  let staff = window.__ptsEmployees;
  if (!Array.isArray(staff) && Array.isArray(window.Employees)) staff = window.Employees;
  if (!Array.isArray(staff) && EMPLOYEES_PATH) {
    try {
      const res = await fetch(EMPLOYEES_PATH, { credentials: 'include' });
      const text = res.ok ? await res.text() : '';
      const a = text.indexOf('['), b = text.lastIndexOf(']');
      if (a >= 0 && b > a) staff = JSON.parse(text.slice(a, b + 1));
    } catch { /* numbers only, then */ }
    if (Array.isArray(staff)) window.__ptsEmployees = staff;
  }
  const staffName = new Map();
  for (const e of Array.isArray(staff) ? staff : []) {
    const full = [e.FirstName, e.LastName].filter(Boolean).join(' ').trim();
    if (!full) continue;
    if (e.Number != null && !staffName.has(String(e.Number))) staffName.set(String(e.Number), full);
    if (e.ID != null) staffName.set(`id:${e.ID}`, full);
  }
  const advisorName = (ro, adv, num) => {
    const named = pick(ro, /^(service)?advisor_?(full_?)?name$/i) ?? pick(adv, /^(full_?|display_?)?name$/i);
    if (named) return String(named);
    const first = pick(adv, /^first_?name$/i), last = pick(adv, /^last_?name$/i);
    if (first || last) return [first, last].filter(Boolean).join(' ');
    if (num == null || num === '') return '';
    return staffName.get(String(num)) || staffName.get(String(num).replace(/^0+(?=\d)/, '')) || staffName.get(`id:${num}`) || '';
  };
  const dashOf = (ro) => {
    const cust = sub(ro, /^customer$/i);
    const adv = sub(ro, /^(service)?advisor$/i);
    return {
      promised: pick(ro, /^promise(d)?(date)?(time)?$/i, /promise/i) ?? null,
      media: pick(ro, /^media(type|sent|status)?$/i, /media/i) ?? '',
      quote: pick(ro, /^quote(type|sent|status)?$/i, /quote/i) ?? '',
      // Xtime shows the customer's last name; only that is kept, and only on this computer
      customer: pick(ro, /^customer_?last_?name$/i) ?? pick(cust, /^last_?name$/i) ?? pick(ro, /^customer_?name$/i, /^last_?name$/i) ?? pick(cust, /name/i) ?? '',
      advisor: pick(ro, /^(service)?advisor_?(number|id|num|no)$/i) ?? pick(adv, /^(number|id)$/i) ?? '',
      advisorName: advisorName(ro, adv, pick(ro, /^(service)?advisor_?(number|id|num|no)$/i) ?? pick(adv, /^(number|id)$/i)),
      modeSince: pick(ro, /^(current)?mode_?(start|change[d]?|updated?)?_?(date|time|datetime|on)$/i, /mode.*(date|time)/i) ?? null,
      modeTime: pick(ro, /^mode_?time$/i, /time_?in_?mode/i) ?? null,
      partsSince: pick(ro, /^parts?_?(start|request(ed)?|sent)?_?(date|time|datetime)$/i, /parts?.*(date|time)/i) ?? null,
      partsTime: pick(ro, /^parts?_?time$/i) ?? null,
      partsStatus: pick(ro, /^parts?_?status$/i, /parts?_?status/i) ?? '',
      express: flag(pick(ro, /^is_?express$/i, /^express(ro)?$/i)),
      waiter: flag(pick(ro, /^is_?waiter$/i, /^waiter$/i, /customer_?(is_?)?waiting/i)),
      onHold: flag(pick(ro, /^(is_?)?on_?hold$/i, /hold/i)),
      notInStock: flag(pick(ro, /not_?in_?stock/i, /parts?_?not_?(in_?)?stock/i)),
      isNew: flag(pick(ro, /^is_?new$/i, /new_?in_?mode/i)),
    };
  };
  let fieldNames = null;

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
      dash: dashOf(ro),
    });
    if (!fieldNames) {
      fieldNames = Object.keys(ro).map((k) => (ro[k] && typeof ro[k] === 'object' && !Array.isArray(ro[k])) ? `${k}{${Object.keys(ro[k]).join(',')}}` : k);
    }
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
  return { ok: true, total: all.size, mine, techName, fieldNames };
})