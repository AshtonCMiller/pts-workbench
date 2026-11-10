// Runs inside the Xtime Inspect frame, with your existing login.
//   { type: 'get' }                  → one RO's lines (GET /api/Stores/{store}/RepairOrder/{RO}) (approval, done, labor type, hours, labor/parts price, who added it)
//   { type: 'done', id, value }      → marks a line done / not done, exactly like Xtime's "Done" button
// Uses the same addresses as Xtime's RO page. Customer information is not returned.
(async (STORE, RO, ACTION) => {
  const base = `/api/Stores/${encodeURIComponent(STORE)}/RepairOrders/${encodeURIComponent(RO)}`;
  // Xtime's page (AngularJS) sends this header on every change when the cookie exists
  const xsrf = (document.cookie.match(/(?:^|;\s*)XSRF-TOKEN=([^;]*)/) || [])[1];
  const headers = { Accept: 'application/json, text/plain, */*', 'Content-Type': 'application/json;charset=UTF-8' };
  if (xsrf) headers['X-XSRF-TOKEN'] = decodeURIComponent(xsrf);
  const call = async (method, url, body) => {
    let res;
    try {
      res = await fetch(url, { method, credentials: 'include', headers, body: body === undefined ? undefined : JSON.stringify(body) });
    } catch (e) { return { ok: false, error: `Xtime request failed: ${e.message}` }; }
    if (res.status === 401 || res.status === 403) return { ok: false, expired: true, error: 'Xtime session expired' };
    const type = res.headers.get('content-type') || '';
    if (res.redirected && !/json/i.test(type)) return { ok: false, expired: true, error: 'Xtime session expired' };
    if (!res.ok) return { ok: false, error: `Xtime returned HTTP ${res.status}` };
    let data = null;
    try { data = /json/i.test(type) ? await res.json() : await res.text(); } catch {}
    return { ok: true, data };
  };

  if (ACTION && ACTION.type === 'done') {
    const r = await call('PUT', `${base}/Lines/${encodeURIComponent(ACTION.id)}/Completed?format=json`, { ID: ACTION.id, Value: !!ACTION.value });
    return r.ok ? { ok: true } : r;
  }

  // Read the RO the way Xtime's RO page does (DataService.retrieveRO); changes use the RepairOrders/… addresses above
  const r = await call('GET', `/api/Stores/${encodeURIComponent(STORE)}/RepairOrder/${encodeURIComponent(RO)}`);
  if (!r.ok) return r;
  let data = r.data;
  if (typeof data === 'string') { try { data = JSON.parse(data); } catch { data = null; } }
  const ro = data && typeof data === 'object' ? (data.Data || data) : null;
  const raw = ro && (ro.RepairOrderLines || ro.repairOrderLines);
  if (!Array.isArray(raw)) return { ok: false, error: "Xtime didn't send this RO's lines" };
  const text = (v) => (v == null ? '' : typeof v === 'string' || typeof v === 'number' ? String(v).trim() : '');
  const nameOf = (u) => {
    if (!u) return '';
    if (typeof u === 'string') return u.trim();
    const full = text(u.FullName) || text(u.DisplayName) || text(u.Name);
    if (full) return full;
    return [text(u.FirstName), text(u.LastName)].filter(Boolean).join(' ') || text(u.UserName) || text(u.Number);
  };
  const payOf = (p) => {
    if (!p) return '';
    if (typeof p === 'string') return p;
    return text(p.Description) || text(p.Name) || text(p.Code) || text(p.PayTypeCode) || '';
  };
  const money = (v) => { const n = Number(v); return Number.isFinite(n) && n >= 0 ? n : null; };
  const lines = raw.filter((l) => l && !l.IsDeleted).map((l) => ({
    id: l.ID,
    dms: text(l.DMSNumber),
    concern: text(l.Complaint) || text(l.ServiceName),
    service: text(l.ServiceName),
    primary: l.IsPrimary === true,
    approved: l.Approved === true,
    declined: l.Declined === true,
    completed: l.Completed === true,
    payType: payOf(l.PayTypeCode) || (l.IsWarranty ? 'Warranty' : ''),
    hours: money(l.Hours),
    labor: money(l.PriceLabor),
    parts: money(l.PriceParts),
    total: money(l.Price),
    addedBy: nameOf(l.CreateUser),
  }));
  const first = raw[0] || {};
  const fieldNames = Object.keys(first).map((k) => (first[k] && typeof first[k] === 'object' && !Array.isArray(first[k])) ? `${k}{${Object.keys(first[k]).join(',')}}` : k);
  return { ok: true, lines, fieldNames };
})