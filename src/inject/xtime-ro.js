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
  // { type: 'view' }: the whole RO for the app's own RO page — the header, vehicle and every line with its notes
  // and parts. The customer (name, phone, email, address) is left out.
  if (ACTION && ACTION.type === 'view') {
    const day = (v) => { const t = text(v); return !t || /^0001-01-01/.test(t) ? '' : t; };
    const v = ro.Vehicle || {};
    const partOf = (p) => ({
      number: text(p.PartNumber) || text(p.Number) || text(p.PartNo),
      description: text(p.Description) || text(p.PartDescription) || text(p.Name),
      quantity: Number(p.Quantity ?? p.QuantityRequested ?? p.Qty) || null,
      onHand: p.QuantityOnHand ?? p.OnHand ?? null,
      price: money(p.Price ?? p.PricePerItem ?? p.UnitPrice),
      status: text(p.Status?.Name || p.Status) || text(p.PartStatus?.Name || p.PartStatus) || (p.IsInStock === true ? 'In stock' : p.IsInStock === false ? 'Not in stock' : ''),
      pickedUp: !!day(p.PickedUpDate || p.PartsPickedUpDate),
    });
    // the store's staff (number → name), as Xtime's RO page has them: for the parts employee (the RO only has the number)
    let staff = window.__ptsEmployees || (Array.isArray(window.Employees) ? window.Employees : null);
    if (!Array.isArray(staff)) {
      try {
        const res = await fetch(`/JS/Employees?storeID=${encodeURIComponent(STORE)}`, { credentials: 'include' });
        const t = res.ok ? await res.text() : '';
        const a = t.indexOf('['), b = t.lastIndexOf(']');
        if (a >= 0 && b > a) staff = JSON.parse(t.slice(a, b + 1));
        if (Array.isArray(staff)) window.__ptsEmployees = staff;
      } catch { /* the number only, then */ }
    }
    const byNumber = (n) => {
      if (!n) return '';
      const e = (Array.isArray(staff) ? staff : []).find((x) => String(x.Number) === String(n));
      return e ? `${[e.FirstName, e.LastName].filter(Boolean).join(' ').trim()} (${n})` : String(n);
    };
    // Xtime's RO address sends only the advisor's / technician's ID (the page fills in the names from its staff list)
    const staffBy = (id, num) => (Array.isArray(staff) ? staff : []).find((x) => (id != null && String(x.ID) === String(id)) || (num && String(x.Number) === String(num))) || null;
    const person = (obj, id) => {
      const e = (obj && (obj.FirstName || obj.LastName)) ? obj : staffBy(id ?? obj?.ID, obj?.Number);
      return { name: e ? [e.FirstName, e.LastName].filter(Boolean).join(' ').trim() : nameOf(obj), number: text(e?.Number ?? obj?.Number) };
    };
    const adv = person(ro.Advisor, ro.AdvisorID), tech = person(ro.Technician, ro.TechnicianID);
    const lineOf = (l) => ({
      id: l.ID, rolid: l.ROLID ?? null, parentId: l.ParentLineId ?? null, groupId: l.GroupID || null,
      // for editing the line (the app's Edit Service window starts from these)
      sgcid: l.SGCID ?? null, sgid: l.SGID ?? null, serviceName: text(l.ServiceName), details: text(l.ServiceGuideSpecialItemName),
      payTypeId: l.PayTypeCode && typeof l.PayTypeCode === 'object' ? l.PayTypeCode.ID ?? null : null,
      dms: text(l.DMSNumber),
      name: text(l.ServiceGuideSpecialItemName) || text(l.ServiceName).replace(/^\[Primary\]:\s*/i, ''),
      opCode: text(l.OpCodeDms) || text(l.OpCode),
      primary: l.IsPrimary === true,
      approved: l.Approved === true, declined: l.Declined === true, completed: l.Completed === true,
      future: l.IsFutureRecommendation === true,
      payType: payOf(l.PayTypeCode) || (l.IsWarranty ? 'W' : l.IsInternal ? 'I' : ''),
      color: text(l.Color),
      hours: money(l.Hours), labor: money(l.PriceLabor), parts: money(l.PriceParts), total: money(l.Price),
      concern: text(l.Complaint), cause: text(l.Cause), correction: text(l.Correction),
      techNote: text(l.TechnicianNote), advisorNote: text(l.AdvisorNote), managerNote: text(l.ManagerNote), partsNote: text(l.PartsNote),
      partsNotNeeded: l.PartsNotNeeded === true, partsPickedUp: !!day(l.PartsPickedUpDate),
      dispatch: text(l.DispatchCode), estimate: money(l.EstimatedDuration),
      techStart: day(l.TechnicianStartTime), techEnd: day(l.TechnicianEndTime),
      parts_: (Array.isArray(l.Parts) ? l.Parts : []).map(partOf),
      subLines: (Array.isArray(l.SubLines) ? l.SubLines : []).map((x) => text(x.Description) || text(x.Name) || text(x.OpCode)).filter(Boolean),
      images: Array.isArray(l.Images) ? l.Images.length : 0,
      techs: (Array.isArray(l.LineTechnicians) ? l.LineTechnicians : []).map((t) => nameOf(t.Technician || t)).filter(Boolean),
      addedBy: nameOf(l.CreateUser), order: Number(l.DisplayOrder) || 0,
    });
    const out = {
      ro: text(ro.RONumber), tag: text(ro.TagNumber), mode: Number(ro.CurrentMode) || 0, modeSince: day(ro.ModeStartTime),
      waiter: !!ro.IsWaiter, express: !!ro.IsExpress, internal: !!ro.IsInternal, warranty: !!ro.IsWarranty, cpo: !!ro.IsCPO, onHold: !!ro.IsOnHold,
      waitingParts: !!ro.WaitingForParts, waitingCustomer: !!ro.WaitingForCustomer, waitingWarranty: !!ro.WaitingForWarranty,
      allPartsInStock: !!ro.AllPartsInStock, partStatus: Number(ro.PartStatusID) || 0,
      promisedDate: text(ro.PromisedDate), promisedTime: text(ro.PromisedTime),
      opened: day(ro.DMSOpenDate) || day(ro.CreateDate), notes: text(ro.Notes),
      partsEmployee: byNumber(text(ro.PartsNumber)),
      advisor: adv.name, advisorNumber: adv.number, technician: tech.name, technicianNumber: tech.number,
      flags: (Array.isArray(ro.CustomFlags) ? ro.CustomFlags : []).map((f) => text(f.Name) || text(f.Description) || text(f.Acronym)).filter(Boolean),
      vehicle: { year: text(v.Year), make: text(v.Make), model: text(v.Model), vin: text(v.VIN).toUpperCase(), mileage: Number(v.Mileage) || null, color: text(v.Color), license: text(v.License) },
      lines: raw.filter((l) => l && !l.IsDeleted).map(lineOf).sort((a, b) => a.order - b.order),
      tax: money(ro.Tax),
      people: { advisorId: ro.AdvisorID ?? null, technicianId: ro.TechnicianID ?? null, advisorSent: !!ro.Advisor, technicianSent: !!ro.Technician, staff: Array.isArray(staff) ? staff.length : 0 },
    };
    return { ok: true, view: out };
  }

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
