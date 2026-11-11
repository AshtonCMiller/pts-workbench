// Runs inside the Xtime Inspect frame. Loads one RO's details page with your existing login
// and returns ONLY its repair lines (line letter, concern, hours, approved/declined/completed).
// Customer information never leaves the page.
// Multipoint inspection (Q99PX) and tire/battery/brake measurement lines (GTIRE, GBATT, GBK) are dropped.
(async (RO, PATH_TEMPLATE, API_PATH) => {
  const IGNORE = /(?:^|[^A-Z0-9])(?:Q99PX|GTIRE|GBATT|GBK)/i;
  const get = (obj, name) => {
    if (!obj || typeof obj !== 'object') return undefined;
    const key = Object.keys(obj).find((k) => k.toLowerCase() === name.toLowerCase());
    return key === undefined ? undefined : obj[key];
  };

  const load = async (id) => {
    try {
      const res = await fetch(PATH_TEMPLATE.replace('{RO}', encodeURIComponent(id)), { credentials: 'include', cache: 'no-store' });
      if (!res.ok) return { error: `HTTP ${res.status}` };
      return { text: await res.text() };
    } catch (e) {
      return { error: e.message };
    }
  };

  // Find a JSON array that follows "RepairOrderLines" (pure JSON response, or JSON embedded in the HTML page).
  const findLines = (src) => {
    try {
      const whole = JSON.parse(src);
      let found;
      const walk = (n) => {
        if (found || !n || typeof n !== 'object') return;
        if (Array.isArray(n)) { n.forEach(walk); return; }
        const v = get(n, 'RepairOrderLines');
        if (Array.isArray(v)) { found = v; return; }
        Object.values(n).forEach(walk);
      };
      walk(whole);
      if (found) return found;
    } catch { /* not plain JSON — look inside the page */ }

    const re = /["']?RepairOrderLines["']?\s*:\s*\[/g;
    let m;
    while ((m = re.exec(src))) {
      const start = m.index + m[0].length - 1; // the '['
      let depth = 0, inStr = null, esc = false;
      for (let i = start; i < src.length; i++) {
        const c = src[i];
        if (inStr) {
          if (esc) esc = false;
          else if (c === '\\') esc = true;
          else if (c === inStr) inStr = null;
          continue;
        }
        if (c === '"') inStr = c;
        else if (c === '[' || c === '{') depth++;
        else if (c === ']' || c === '}') {
          depth--;
          if (depth === 0) {
            try { return JSON.parse(src.slice(start, i + 1)); } catch { break; }
          }
        }
      }
    }
    return null;
  };

  const parse = (text) => {
    let raw = findLines(text);
    if (!raw && text.includes('&quot;')) {
      // JSON stored in an HTML attribute arrives entity-encoded
      raw = findLines(text.replace(/&quot;/g, '"').replace(/&#39;/g, "'").replace(/&amp;/g, '&'));
    }
    return raw;
  };

  // 1) the details page by RO number
  let first = await load(RO);
  let raw = first.text ? parse(first.text) : null;
  let vehicle = null;

  // 2) not found: look the RO up on the dashboard (it lists every tech's ROs) and try its internal ID
  if (!raw && API_PATH) {
    try {
      const res = await fetch(API_PATH, { credentials: 'include', cache: 'no-store', headers: { Accept: 'application/json' } });
      const data = res.ok ? await res.json() : null;
      let match = null;
      const walk = (n) => {
        if (match || !n || typeof n !== 'object') return;
        if (Array.isArray(n)) { n.forEach(walk); return; }
        if (String(get(n, 'RONumber') ?? '') === String(RO) && get(n, 'Vehicle') !== undefined) { match = n; return; }
        Object.values(n).forEach(walk);
      };
      walk(data);
      if (match) {
        let v = get(match, 'Vehicle');
        if (Array.isArray(v)) v = v[0];
        vehicle = {
          ymm: [get(v, 'year'), get(v, 'make'), get(v, 'model')].filter(Boolean).join(' '),
          vin: String(get(v, 'VIN') ?? '').toUpperCase(),
        };
        const id = get(match, 'ID') ?? get(match, 'Id') ?? get(match, 'RepairOrderId');
        if (id != null && String(id) !== String(RO)) {
          const second = await load(id);
          if (second.text) raw = parse(second.text);
        }
        // Some dashboard entries carry the lines themselves
        if (!raw && Array.isArray(get(match, 'RepairOrderLines'))) raw = get(match, 'RepairOrderLines');
      }
    } catch { /* fall through to the error below */ }
  }
  if (!raw) {
    return { ok: false, error: first.error ? first.error : 'RepairOrderLines not found', vehicle,
      notOnDashboard: !!API_PATH && !vehicle };
  }

  const lines = [];
  raw.forEach((l, i) => {
    const service = String(get(l, 'ServiceName') ?? '').trim();
    const complaint = String(get(l, 'Complaint') ?? '').trim();
    const codes = [service, complaint, ...Object.values(l).filter((v) => typeof v === 'string')].join(' ');
    if (IGNORE.test(codes)) return;
    const dms = String(get(l, 'DMSNumber') ?? '').trim();
    const hours = Number(get(l, 'Hours'));
    lines.push({
      key: dms || `#${i + 1}`, // made unique below when two lines share a letter
      dms,
      complaint,
      service,
      hours: Number.isFinite(hours) ? hours : 0,
      approved: get(l, 'Approved') === true,
      declined: get(l, 'Declined') === true,
      completed: get(l, 'Completed') === true,
      primary: get(l, 'IsPrimary') === true,
    });
  });
  // Some jobs have two lines under one letter: the primary (usually the diag) and the repair.
  // A letter used once keeps its plain key; shared letters get "B:diag" for the primary and
  // "B", "B:2"… for the rest, so each line can be flagged on its own and keys stay stable.
  const byLetter = new Map();
  for (const l of lines) {
    if (!byLetter.has(l.key)) byLetter.set(l.key, []);
    byLetter.get(l.key).push(l);
  }
  for (const [letter, group] of byLetter) {
    if (group.length < 2) continue;
    const ordered = [...group.filter((l) => l.primary), ...group.filter((l) => !l.primary)];
    let diag = 0, other = 0;
    for (const l of ordered) {
      if (l.primary) { diag++; l.key = diag === 1 ? `${letter}:diag` : `${letter}:diag${diag}`; l.diag = true; }
      else { other++; l.key = other === 1 ? letter : `${letter}:${other}`; }
    }
  }
  // Within a letter, show the diag first, then the repair
  const firstIndex = new Map();
  lines.forEach((l, i) => { const k = l.dms || l.key; if (!firstIndex.has(k)) firstIndex.set(k, i); });
  lines.sort((a, b) => {
    const ga = firstIndex.get(a.dms || a.key), gb = firstIndex.get(b.dms || b.key);
    return ga - gb || (b.primary ? 1 : 0) - (a.primary ? 1 : 0);
  });
  return { ok: true, lines, vehicle };
})