// Runs inside the Xtime Inspect frame, with your existing login: an RO's inspection form, read and changed
// exactly the way Xtime's RO page does it (no customer information is read or returned).
//   { type: 'get' }                          → the form (categories → items with colour / value / sub-items),
//                                              the forms the store has, and which one this RO uses
//   { type: 'save', iiid, color, value }     → PUT …/RepairOrders/{RO}/InspectionItems  (Xtime's saveItem); a Red item then
//                                              goes to Xtime's HandleRedInspectionFormItem, as Xtime's page does when the
//                                              store uses "common ops"
//   { type: 'switch', formId }               → PUT …/RepairOrders/{RO}/InspectionForm  (Xtime's form drop-down)
// Where the form comes from: the RO's details page names its scripts (InspectionFormData = the form, AvailableForms,
// StoreData) and holds the RO's answers (InspectionItem_ROSpecific); …/InspectionItems has the latest answers.
(async (STORE, RO, DETAILS_PATH, ACTION) => {
  const xsrf = (document.cookie.match(/(?:^|;\s*)XSRF-TOKEN=([^;]*)/) || [])[1];
  const headers = { Accept: 'application/json, text/plain, */*', 'Content-Type': 'application/json;charset=UTF-8' };
  if (xsrf) headers['X-XSRF-TOKEN'] = decodeURIComponent(xsrf);
  const api = `/api/Stores/${encodeURIComponent(STORE)}/RepairOrders/${encodeURIComponent(RO)}`;
  const call = async (method, url, body) => {
    let res;
    try { res = await fetch(url, { method, credentials: 'include', cache: 'no-store', headers, body: body === undefined ? undefined : JSON.stringify(body) }); }
    catch (e) { return { ok: false, error: `Xtime request failed: ${e.message}` }; }
    if (res.status === 401 || res.status === 403) return { ok: false, expired: true, error: 'Xtime session expired' };
    const type = res.headers.get('content-type') || '';
    if (res.redirected && !/json|javascript/i.test(type)) return { ok: false, expired: true, error: 'Xtime session expired' };
    if (!res.ok) {
      // Xtime explains refusals (e.g. a service added more times than allowed) in ExceptionMessage / Message
      let why = '';
      try { const b = await res.json(); why = b?.ExceptionMessage || b?.Message || b?.message || ''; } catch {}
      return { ok: false, status: res.status, error: why ? String(why) : `Xtime returned HTTP ${res.status}` };
    }
    let data = null;
    try { data = /json/i.test(type) ? await res.json() : await res.text(); } catch {}
    return { ok: true, data };
  };
  // "var Name = {…};" (a script Xtime writes its data into) → the value
  const varValue = (text, name) => {
    const src = String(text || '');
    const at = name ? src.search(new RegExp(`\\b${name}\\s*=\\s*[\\[{]`)) : 0;
    if (at < 0) return undefined;
    const start = src.slice(at).search(/[\[{]/) + at;
    let depth = 0, inStr = null, esc = false;
    for (let i = start; i < src.length; i++) {
      const c = src[i];
      if (inStr) { if (esc) esc = false; else if (c === '\\') esc = true; else if (c === inStr) inStr = null; continue; }
      if (c === '"') inStr = c;
      else if (c === '[' || c === '{') depth++;
      else if (c === ']' || c === '}') { depth--; if (depth === 0) { try { return JSON.parse(src.slice(start, i + 1)); } catch { return undefined; } } }
    }
    return undefined;
  };
  const unwrap = (d) => { if (typeof d === 'string') { try { d = JSON.parse(d); } catch { return d; } } return d && typeof d === 'object' && 'Data' in d && !Array.isArray(d) ? d.Data : d; };

  // The RO's details page, and the scripts it names (cached for this page: they only change with the form)
  window.__ptsInspect ||= {};
  const cache = window.__ptsInspect;
  const details = async () => {
    const r = await call('GET', DETAILS_PATH.replace('{RO}', encodeURIComponent(RO)));
    if (!r.ok) return r;
    const html = String(r.data || '');
    const script = (name) => {
      const m = html.match(new RegExp(`<script[^>]+src=["']([^"']*${name}[^"']*)["']`, 'i'));
      return m ? new URL(m[1].replace(/&amp;/g, '&'), location.href).href : '';
    };
    return { ok: true, html, formUrl: script('InspectionFormData'), formsUrl: script('AvailableForms'), storeUrl: script('StoreData'),
      servicesUrl: script('AvailableServices(?![A-Za-z])'), payUrl: script('AvailablePayTypeCodes') };
  };
  const loadVar = async (url, name) => {
    if (!url) return undefined;
    if (cache[url] !== undefined) return cache[url];
    const r = await call('GET', url);
    const v = r.ok ? varValue(r.data, name) : undefined;
    if (v !== undefined) cache[url] = v;
    return v;
  };

  if (ACTION?.type === 'save') {
    const body = { IIID: ACTION.iiid, Color: ACTION.color, Value: ACTION.value ?? '' };
    const r = await call('PUT', `${api}/InspectionItems/?format=json`, body);
    if (!r.ok) return r;
    // A Red item: Xtime's page asks Xtime to add its service (stores using "common ops", for ROs opened since then)
    let red = '';
    if (ACTION.color === 'Red') {
      try {
        const d = await details();
        const store = d.ok ? await loadVar(d.storeUrl, 'StoreData') : null;
        const roR = await call('GET', `/api/Stores/${encodeURIComponent(STORE)}/RepairOrder/${encodeURIComponent(RO)}`);
        const ro = roR.ok ? unwrap(roR.data) : null;
        const toDate = (v) => (v && v.substr ? (v.substr(0, 6) === '/Date(' ? new Date(parseInt(v.substr(6), 10)) : new Date(v)) : v);
        const since = toDate(store?.IntegrationXtime?.CommonOpsEnableDate), created = toDate(ro?.CreateDate);
        if (ro && since && created && created >= since) {
          const h = await call('POST', '/api/ServiceStudio/HandleRedInspectionFormItem', {
            StoreId: store?.ID ?? Number(STORE), RoNumber: ro.RONumber, IsInternal: ro.IsInternal, IiId: ACTION.iiid, AsrIiId: unwrap(r.data),
            Make: ro.Vehicle && ro.Vehicle.MakeNormalized && ro.Vehicle.MakeNormalized.length ? ro.Vehicle.MakeNormalized : ro.Vehicle?.Make,
            Vin: ro.Vehicle ? ro.Vehicle.VIN : null, RoTechnician: ro.Technician ? ro.Technician.ID : (ro.TechnicianID ?? null),
          });
          red = h.ok ? 'service added' : `service not added (${h.error})`;
        }
      } catch (e) { red = `service not added (${e.message})`; }
    }
    return { ok: true, red };
  }

  // ---- service lines: Xtime's Add Service and Delete Service ----
  if (ACTION?.type === 'catalog') { // what the Add Service window offers: the store's services and labor types
    const d = await details();
    if (!d.ok) return d;
    const services = (await loadVar(d.servicesUrl, 'AvailableServices')) || [];
    const pay = (await loadVar(d.payUrl, 'AvailablePayTypeCodes')) || [];
    const store = (await loadVar(d.storeUrl, 'StoreData')) || {};
    const roR = await call('GET', `/api/Stores/${encodeURIComponent(STORE)}/RepairOrder/${encodeURIComponent(RO)}`);
    const ro = roR.ok ? unwrap(roR.data) : null;
    if (!services.length) return { ok: false, error: "Couldn't read the store's services from Xtime" };
    return { ok: true, catalog: {
      services: services.filter((s) => s && s.SGCID).map((s) => ({ sgcid: s.SGCID, sgid: s.SGID, name: String(s.ServiceName || ''), opCode: String(s.OpCode || ''),
        price: Number(s.Price) || 0, labor: Number(s.PriceLabor) || 0 })),
      payTypes: pay.map((p) => ({ id: p.ID, code: String(p.Code || ''), payTypeId: p.PayTypeID, isDefault: !!p.IsDefault })),
      defaultColor: String(store.DefaultLineColor || 'Red'), internal: !!ro?.IsInternal,
    } };
  }
  if (ACTION?.type === 'addLine') { // Xtime's EditFormService.saveLine for a new line (ID 0)
    const d = await details();
    if (!d.ok) return d;
    const pay = (await loadVar(d.payUrl, 'AvailablePayTypeCodes')) || [];
    const store = (await loadVar(d.storeUrl, 'StoreData')) || {};
    const a = ACTION.line || {};
    const num = (v) => (v === '' || v == null || Number.isNaN(Number(v)) ? -1 : Number(v)); // as Xtime: empty = use the service's own
    const line = {
      ID: 0, SGCID: a.sgcid, SGID: a.sgid, Color: a.color || store.DefaultLineColor || 'Red',
      PayTypeCode: pay.find((p) => String(p.ID) === String(a.payTypeId)) || pay.find((p) => p.IsDefault) || null,
      Hours: num(a.hours), PriceLabor: num(a.labor), PriceParts: num(a.parts), Price: num(a.total),
      ServiceGuideSpecialItemName: a.details || '', Complaint: a.complaint || '', Cause: a.cause || '', Correction: a.correction || '',
      TechnicianNote: a.techNote || '', IsGbb: false, ServiceType: null,
      EnableInternalLinePrice: store?.Labor?.InternalLinePriceDefaultValue ?? null, EnableWarrantyLinePrice: store?.Labor?.WarrantyLinePriceDefaultValue ?? null,
    };
    if (!line.SGCID) return { ok: false, error: 'Pick a service first' };
    const r = await call('PUT', `${api}/Lines/0?format=json`, { Data: line });
    return r.ok ? { ok: true } : r;
  }
  if (ACTION?.type === 'deleteLine') { // Xtime's RODetailsService.deleteLine
    const r = await call('DELETE', `${api}/Lines/${encodeURIComponent(ACTION.id)}?format=json`);
    return r.ok ? { ok: true } : r;
  }

  if (ACTION?.type === 'switch') {
    const r = await call('PUT', `${api}/InspectionForm/?format=json`, { Data: { ID: ACTION.formId } });
    if (r.ok) for (const k of Object.keys(cache)) delete cache[k]; // another form now
    return r.ok ? { ok: true } : r;
  }

  // get
  const d = await details();
  if (!d.ok) return d;
  if (!d.formUrl) return { ok: false, error: "Xtime's RO page didn't name its inspection form" };
  delete cache[d.formUrl]; // the form's own address changes with the form; read it fresh each time the tab loads it
  const form = await loadVar(d.formUrl, 'InspectionFormData');
  if (!form || !Array.isArray(form.Categories)) return { ok: false, error: "Couldn't read the inspection form from Xtime" };
  const forms = (await loadVar(d.formsUrl, 'AvailableForms')) || [];
  // the RO's answers: the latest from Xtime, or the ones written into its page
  let answers = null;
  const a = await call('GET', `${api}/InspectionItems`);
  if (a.ok) { const v = unwrap(a.data); if (Array.isArray(v)) answers = v; }
  if (!answers) answers = varValue(d.html, 'InspectionItem_ROSpecific') || [];
  const byItem = new Map(answers.map((x) => [String(x.ItemID ?? x.IIID ?? x.ID), x]));
  const text = (v) => (v == null ? '' : String(v));
  const categories = form.Categories.filter((c) => c && c.Active !== false).sort((x, y) => (x.DisplayOrder || 0) - (y.DisplayOrder || 0)).map((c) => ({
    id: c.ID, name: text(c.Name),
    items: (c.Items || []).filter((i) => i && i.Active !== false).sort((x, y) => (x.DisplayOrder || 0) - (y.DisplayOrder || 0)).map((i) => {
      const ans = byItem.get(String(i.ID));
      return {
        id: i.ID, name: text(i.Name), required: !!i.Required, showValue: !!i.ShowValueField,
        // as Xtime's page: an item not answered yet shows its default colour and no value
        color: ans ? text(ans.Color) : text(i.DefaultColor), value: ans ? text(ans.Value) : '', answered: !!ans,
        services: Array.isArray(i.SelectedServices) ? i.SelectedServices : [], // the services Xtime offers for it (Add Service)
        subItems: text(i.SubItems), subRed: text(i.SubItemsRed), subYellow: text(i.SubItemsYellow), subGreen: text(i.SubItemsGreen),
      };
    }),
  })).filter((c) => c.items.length);
  return { ok: true, form: { id: form.ID ?? form.IFID, name: text(form.Name), categories,
    forms: (Array.isArray(forms) ? forms : []).map((f) => ({ id: f.Key, name: text(f.Value) })) } };
})