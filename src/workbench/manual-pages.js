// The app's own pages (Workshop Manual, PC/ED, OASIS, TSBs, Wiring Diagrams, Labor Times, Diagnostics, Xtime View):
// their views, the PTS sign-in window when Ford asks for it, and everything the pages ask for (manualCall), through
// src/manual.js (Ford) and the Xtime scripts (src/inject).
'use strict';
const { BrowserWindow, clipboard, session, shell, WebContentsView } = require('electron');
const { ManualService, PTS: PTS_ORIGIN } = require('../manual');
const path = require('path');
const S = require('../app/state'); // shared app state (see app/state.js)
const { profileFile } = require('../app/files');
const { cleanDealerCode } = require('../app/config');
const SRC = path.join(__dirname, '..'); // the app's src folder

module.exports = function install(my) {
  Object.assign(my, { makeManualView, manualCall, manualSave, destroyTab }); // used by the window's other parts

  // ---------- the app's own workshop manual ----------
  const manualSes = session.fromPartition(my.PARTITION);
  my.manualSes = manualSes;
  const manualService = new ManualService(manualSes, profileFile(my.profile.id, 'manual-cache'), profileFile(my.profile.id, 'manual.log'));
  my.manualService = manualService;
  // Pictures on Ford's document server need its sign-on cookies: one sign-on at a time, at most every 30 s
  let deniedRun = null, deniedAt = 0;
  manualService.onDenied = () => {
    if (deniedRun) return deniedRun;
    if (Date.now() - deniedAt < 30 * 1000) return Promise.resolve();
    deniedAt = Date.now();
    deniedRun = my.ensureContentCookies(true).finally(() => { deniedRun = null; });
    return deniedRun;
  };
  try {
    if (!manualSes.protocol.isProtocolHandled('ptsasset')) {
      manualSes.protocol.handle('ptsasset', (req) => {
        let target = '';
        try { target = new URL(req.url).searchParams.get('u') || ''; } catch {}
        return manualService.asset(target);
      });
    }
  } catch (e) { console.error('ptsasset', e); }
  const manualViews = new Map(); // webContents id -> { ws, tab }
  my.manualViews = manualViews;

  function makeManualView(ws, tab, page = 'viewer.html') {
    const view = new WebContentsView({
      webPreferences: {
        partition: my.PARTITION, sandbox: true, contextIsolation: true, nodeIntegration: false,
        preload: path.join(SRC, 'manual', 'viewer-preload.js'),
      },
    });
    my.win.contentView.addChildView(view);
    view.setVisible(false);
    my.hardenWebContents(view.webContents); // links to Ford pages open in a window with your login
    const id = view.webContents.id;
    manualViews.set(id, { ws, tab });
    view.webContents.once('destroyed', () => manualViews.delete(id));
    // F12: the page inspector, for working out problems with the viewer
    view.webContents.on('before-input-event', (ev, input) => {
      if (input.type === 'keyDown' && input.key === 'F12') { ev.preventDefault(); view.webContents.openDevTools({ mode: 'detach' }); }
    });
    tab.status = '';
    view.ptsPage = page; // to load it again from new app files (hot update)
    view.webContents.loadFile(path.join(S.PAGES, 'manual', page));
    return view;
  }

  // Sign in to PTS in a small window; resolves once PTS itself has loaded (or the window is closed)
  let signInWindow = null;
  function signInToPts() {
    if (signInWindow && !signInWindow.isDestroyed()) { signInWindow.focus(); return signInWindow.donePromise; }
    const w = new BrowserWindow({
      width: 980, height: 760, autoHideMenuBar: true, title: 'Sign in to PTS', parent: undefined,
      webPreferences: { partition: my.PARTITION, sandbox: true, contextIsolation: true,
        preload: path.join(SRC, 'web-preload.js'), nodeIntegrationInSubFrames: true },
    });
    signInWindow = w;
    w.donePromise = new Promise((resolve) => {
      let seenLogin = false;
      const check = () => {
        if (w.isDestroyed()) return;
        const host = my.hostOf(w.webContents.getURL());
        if (host && host !== my.hostOf(PTS_ORIGIN)) { seenLogin = true; return; }
        if (host === my.hostOf(PTS_ORIGIN)) setTimeout(() => { if (!w.isDestroyed()) w.close(); }, seenLogin ? 800 : 300);
      };
      w.webContents.on('did-finish-load', check);
      w.on('closed', () => { signInWindow = null; resolve(true); });
    });
    w.loadURL(my.settings.ptsUrl);
    return w.donePromise;
  }

  async function manualCall(sender, name, payload) {
    const entry = manualViews.get(sender.id);
    if (!entry) return undefined;
    const { ws, tab } = entry;
    const fail = (e) => ({ error: true, needsLogin: !!e.needsLogin, message: e.message || String(e), detail: e.detail || '' });
    const bookKind = tab.kind === 'pced' ? 'pced' : 'workshop'; // which of the vehicle's books this viewer shows
    switch (name) {
      case 'init': {
        const open = tab.pendingOpen || null; // a procedure sent here from the Diagnostics tab before this tab existed
        tab.pendingOpen = null;
        return { vin: ws.vin, ro: ws.ro, ymm: ws.ymm, state: tab.manualState || null, open, book: bookKind };
      }
      case 'diag': // Diagnostics tab: Ford's search for DTCs / words on this vehicle
        try {
          const q = payload || {};
          const dtcs = (Array.isArray(q.dtcs) ? q.dtcs : []).map(String).filter((x) => /^[PBUC][0-9A-F]{4,6}(?:[:|][0-9A-Z]{2,12}){0,2}$/i.test(x)).slice(0, 20);
          const sources = Array.isArray(q.sources) ? q.sources.map(String) : undefined;
          return await manualService.diagSearch(ws.vin, { text: String(q.text || '').slice(0, 200), dtcs, sources,
            rows: Math.min(200, Math.max(1, Number(q.rows) || 50)), start: Math.max(0, Number(q.start) || 0), exact: !!q.exact });
        } catch (e) { return fail(e); }
      case 'open-diag': { // from the manual's search: search this DTC in the vehicle's Diagnostics tab
        const query = String(payload || '').slice(0, 200);
        const dt = ws.tabs.find((t) => t.kind === 'diag') || my.addTab(ws, 'diag', { activate: false });
        my.setActive(ws, dt); my.mode = 'manual';
        dt.manualState = { ...(dt.manualState || {}), query };
        if (dt.view && !dt.view.webContents.isDestroyed() && !dt.view.webContents.isLoading()) dt.view.webContents.send('manual-open', { query });
        my.saveWorkspaces(); my.layout(); my.pushState();
        return true;
      }
      case 'tsb-list': { // TSBs tab: the vehicle's TSBs / GSBs / SSMs
        const pacode = String(my.settings.dealerCode || '').replace(/^[A-Z]+/i, '');
        if (!pacode) return { needsDealer: true };
        try { return await manualService.tsbList(ws.vin, pacode); } catch (e) { return fail(e); }
      }
      case 'wiring-toc': // Wiring Diagrams tab: the vehicle's wiring book
        try { return await manualService.wiringToc(ws.vin); } catch (e) { return fail(e); }
      case 'wiring-pages':
        try { return await manualService.wiringPages(ws.vin, String(payload?.cell || '')); } catch (e) { return fail(e); }
      case 'wiring-svg':
        try { return await manualService.wiringSvg(ws.vin, String(payload?.cell || ''), Number(payload?.page) || 1); } catch (e) { return fail(e); }
      // ---- the app's own Xtime RO page (Xtime View tab): read through the signed-in Xtime page, like the dashboard ----
      case 'xview': {
        const frame = my.findXtimeDataFrame();
        if (!frame) return { error: true, message: 'Xtime is still loading. Try again in a moment.' };
        try {
          const r = await frame.executeJavaScript(`${S.XTIME_RO}(${JSON.stringify(my.settings.storeId)}, ${JSON.stringify(ws.ro)}, {"type":"view"})`, true);
          if (!r?.ok) return { error: true, expired: !!r?.expired, message: r?.error || "Couldn't read this RO from Xtime" };
          if (!r.view.advisor || !r.view.technician) manualService.log(`xtime view RO ${ws.ro}: advisor "${r.view.advisor}", technician "${r.view.technician}" (${JSON.stringify(r.view.people || {})})`);
          return { view: r.view, at: Date.now() };
        } catch (e) { return fail(e); }
      }
      case 'xview-done': { // a line's Done box: the same change as Xtime's Done button
        const frame = my.findXtimeDataFrame();
        const id = payload?.id;
        if (!frame || id == null) return { error: true, message: 'Xtime is still loading' };
        try {
          const r = await frame.executeJavaScript(`${S.XTIME_RO}(${JSON.stringify(my.settings.storeId)}, ${JSON.stringify(ws.ro)}, ${JSON.stringify({ type: 'done', id, value: !!payload.value })})`, true);
          return r?.ok ? { ok: true } : { error: true, message: r?.error || "Couldn't update the line in Xtime" };
        } catch (e) { return fail(e); }
      }
      case 'xinspect': { // the RO's inspection form: read it, change an item, or switch forms (as Xtime's page does)
        const frame = my.findXtimeDataFrame();
        if (!frame) return { error: true, message: 'Xtime is still loading. Try again in a moment.' };
        const q = payload || {};
        const action = q.type === 'save' ? { type: 'save', iiid: q.iiid, color: String(q.color || ''), value: String(q.value ?? '').slice(0, 255) }
          : q.type === 'switch' ? { type: 'switch', formId: q.formId }
          : q.type === 'catalog' ? { type: 'catalog' }
          : q.type === 'addLine' ? { type: 'addLine', line: q.line || {} }
          : q.type === 'deleteLine' ? { type: 'deleteLine', id: q.id }
          : { type: 'get' };
        try {
          const r = await frame.executeJavaScript(`${S.XTIME_INSPECT}(${JSON.stringify(my.settings.storeId)}, ${JSON.stringify(ws.ro)}, ` +
            `${JSON.stringify(my.settings.xtimeDetailsPath || '/RepairOrder/Details/{RO}')}, ${JSON.stringify(action)})`, true);
          if (!r?.ok) { manualService.log(`xtime inspection RO ${ws.ro} ${action.type}: ${r?.error || 'failed'}`); return { error: true, expired: !!r?.expired, message: r?.error || "Couldn't reach Xtime's inspection form" }; }
          if (action.type === 'save' && r.red) manualService.log(`xtime inspection RO ${ws.ro}: red item ${action.iiid}: ${r.red}`);
          return r;
        } catch (e) { return fail(e); }
      }
      case 'open-xtime': { // Xtime's own page for this RO, in its tab
        const t = ws.tabs.find((x) => x.kind === 'xtime') || my.addTab(ws, 'xtime', { activate: false, front: true });
        my.setActive(ws, t); my.mode = 'manual';
        my.saveWorkspaces(); my.layout(); my.pushState();
        return true;
      }
      case 'open-chat': // the chat panel, on this RO's conversation
        if (my.chrome && !my.chrome.webContents.isDestroyed()) my.chrome.webContents.send('chat', { kind: 'open', ro: ws.ro });
        return true;
      case 'wiring-search':
        try { return await manualService.wiringSearch(ws.vin, String(payload?.text || '')); } catch (e) { return fail(e); }
      case 'wiring-locations':
        try { return await manualService.wiringLocations(ws.vin, String(payload?.item || '').replace(/[^A-Za-z0-9_-]/g, '').slice(0, 20)); } catch (e) { return fail(e); }
      case 'wiring-ford': { // Ford's own wiring page (connector faces, the connector list…) as a tab in Service Manual
        try {
          const info = await manualService.wiringInfo(ws.vin);
          const q = payload || {};
          const kind = ['face', 'Connectors', 'LocIndex', 'page', 'TableOfContents'].includes(q.kind) ? q.kind : 'page';
          const url = kind === 'TableOfContents' ? info.tocUrl : manualService.wiringFordUrl(info, ws.vin, kind,
            { cell: String(q.cell || '').replace(/\D/g, ''), page: q.page === '' ? '' : Number(q.page) || 1, item: String(q.item || '').slice(0, 40), title: String(q.title || '').slice(0, 120) });
          return await manualCall(sender, 'open-article', { url, title: String(q.title || 'Wiring').slice(0, 40) });
        } catch (e) { return fail(e); }
      }
      case 'open-wiring': { // Diagnostics: show this wiring page in the vehicle's Wiring Diagrams tab
        const q = payload || {};
        const wiring = { cell: String(q.cell || '').replace(/\D/g, '').slice(0, 4), page: Number(q.page) || 1, item: String(q.item || '').slice(0, 40), title: String(q.title || '').slice(0, 120) };
        if (!wiring.cell) return false;
        const wt = ws.tabs.find((t) => t.kind === 'wiring') || my.addTab(ws, 'wiring', { activate: false });
        my.setActive(ws, wt); my.mode = 'manual';
        if (wt.view && !wt.view.webContents.isDestroyed() && !wt.view.webContents.isLoading()) wt.view.webContents.send('manual-open', { wiring });
        else wt.pendingOpen = { wiring };
        my.saveWorkspaces(); my.layout(); my.pushState();
        return true;
      }
      case 'tsb-article':
        try { return await manualService.tsbArticle(String(payload?.url || '')); } catch (e) { return fail(e); }
      case 'open-tsb': { // Diagnostics: show this TSB / GSB / SSM in the vehicle's TSBs tab
        const q = payload || {};
        const tsb = { id: String(q.id || '').slice(0, 40), type: String(q.type || '').toUpperCase().slice(0, 4), title: String(q.title || '').slice(0, 200), url: String(q.url || '').slice(0, 600) };
        if (!tsb.id) return false;
        const tt = ws.tabs.find((t) => t.kind === 'tsb') || my.addTab(ws, 'tsb', { activate: false });
        my.setActive(ws, tt); my.mode = 'manual';
        if (tt.view && !tt.view.webContents.isDestroyed() && !tt.view.webContents.isLoading()) tt.view.webContents.send('manual-open', { tsb });
        else tt.pendingOpen = { tsb };
        my.saveWorkspaces(); my.layout(); my.pushState();
        return true;
      }
      case 'open-proc': { // Diagnostics tab: show a procedure / pinpoint test in this vehicle's Workshop Manual (or PC/ED) tab
        const q = payload || {};
        const link = { searchNumber: String(q.searchNumber || ''), procUid: String(q.procUid || ''), linkType: 'Procedure',
          title: String(q.title || '').slice(0, 200), ppt: String(q.ppt || ''), section: 'Diagnostics' };
        if (!link.searchNumber && !link.procUid) return false;
        const kind = q.book === 'pced' ? 'pced' : 'manual';
        const mt = ws.tabs.find((t) => t.kind === kind) || (kind === 'pced' ? my.addPcedTab(ws) : my.addTab(ws, 'manual', { activate: false }));
        my.setActive(ws, mt); my.mode = 'manual';
        if (mt.view && !mt.view.webContents.isDestroyed() && !mt.view.webContents.isLoading()) mt.view.webContents.send('manual-open', link);
        else mt.pendingOpen = link;
        my.saveWorkspaces(); my.layout(); my.pushState();
        return true;
      }
      case 'book':
        try {
          const b = await manualService.book(ws.vin, bookKind);
          return { title: b.title, vehicle: b.vehicle, kind: b.kind };
        } catch (e) { return fail(e); }
      case 'tree':
        try {
          const html = await manualService.tree(ws.vin, bookKind);
          return { html, features: await manualService.vehicleFeatures(ws.vin) }; // to show only what applies to this vehicle
        } catch (e) { return fail(e); }
      case 'proc':
        try {
          const link = payload || {};
          return await manualService.proc(ws.vin, {
            searchNumber: String(link.searchNumber || ''), procUid: String(link.procUid || ''),
            linkType: link.linkType === 'PublicationContent' ? 'PublicationContent' : 'Procedure',
          }, bookKind);
        } catch (e) { return fail(e); }
      case 'oasis':
        try { return await manualService.oasis(ws.vin); } catch (e) { return fail(e); }
      case 'cv':
        try { return await manualService.connectedVehicle(ws.vin); } catch (e) { return fail(e); }
      case 'slts': // Labor Times tab: Ford's SLTS pages for this vehicle
        try {
          const q = payload || {};
          if (q.op === 'start') return await manualService.sltsStart(ws.vin);
          return await manualService.sltsRequest({ url: String(q.url || ''), method: q.method === 'POST' ? 'POST' : 'GET',
            fields: q.fields && typeof q.fields === 'object' ? q.fields : {} });
        } catch (e) { return fail(e); }
      case 'svc-list': // which FSAs need a software validation code, and codes already retrieved
        try {
          const form = await manualService.svcForm(ws.vin);
          return { fsas: form.fsas, jv: form.jv, saved: manualService.svcSaved(form.vin), dealerCode: my.settings.dealerCode || '' };
        } catch (e) { return fail(e); }
      case 'svc': {
        const fsa = String(payload?.fsa || '').toUpperCase().replace(/[^A-Z0-9]/g, '').slice(0, 12);
        if (!fsa) return { status: 'error', message: 'No FSA number' };
        if (!my.settings.dealerCode) return { status: 'needs-dealer' };
        try { return await manualService.svc(ws.vin, fsa, my.settings.dealerCode); } catch (e) { return fail(e); }
      }
      case 'set-dealer-code': {
        const code = cleanDealerCode(payload);
        if (!code) return false;
        my.settings = { ...my.settings, dealerCode: code };
        my.writeJson('settings.json', my.settings);
        my.pushState();
        return true;
      }
      case 'copy': try { clipboard.writeText(String(payload || '').slice(0, 500)); } catch {} return true;
      case 'spa':
        try { return await manualService.spaSteps(ws.vin, { file: String(payload?.file || ''), procUid: String(payload?.procUid || '') }, bookKind); } catch (e) { return fail(e); }
      case 'report':
        try { return { ok: await manualService.reportPinpoint(ws.vin, payload || {}, bookKind) }; } catch (e) { return fail(e); }
      case 'signin': await signInToPts(); return true;
      case 'open-log': shell.openPath(profileFile(my.profile.id, 'manual.log')).catch(() => {}); return true;
      case 'log': manualService.log(`viewer: ${String(payload || '').slice(0, 500)}`); return true;
      case 'open-pts': {
        const t = my.addTab(ws, 'pts');
        my.saveWorkspaces(); my.layout(); my.pushState();
        return t.id;
      }
      // A Ford article (FSA, notice) from OASIS: its own tab in Service Manual instead of a window
      case 'open-article': {
        let url = String(payload?.url || '');
        try { url = new URL(url, PTS_ORIGIN).href; } catch { return false; }
        if (!/^https:/i.test(url)) return false;
        let t = ws.tabs.find((x) => x.kind === 'article' && x.url === url);
        if (!t) {
          t = my.addTab(ws, 'article', { activate: false });
          t.url = url;
          t.title = String(payload?.title || '').replace(/\s+/g, ' ').trim().slice(0, 40) || 'Article';
          // next to the other Service Manual tabs (after the last one), not at the end of the top row
          ws.tabs.splice(ws.tabs.indexOf(t), 1);
          const lastSvc = ws.tabs.map((x) => my.inService(x.kind)).lastIndexOf(true);
          ws.tabs.splice(lastSvc + 1, 0, t);
        }
        my.setActive(ws, t);
        my.saveWorkspaces(); my.layout(); my.pushState();
        return true;
      }
      case 'open-url': {
        let url = String(payload || '');
        try { url = new URL(url, PTS_ORIGIN).href; } catch { return false; }
        if (!/^https:/i.test(url)) return false;
        const w = new BrowserWindow({ width: 1100, height: 800, autoHideMenuBar: true,
          webPreferences: { partition: my.PARTITION, sandbox: true, contextIsolation: true, plugins: true,
            preload: path.join(SRC, 'web-preload.js'), nodeIntegrationInSubFrames: true } });
        w.loadURL(url);
        return true;
      }
      default: return undefined;
    }
  }
  function manualSave(sender, state) {
    const entry = manualViews.get(sender.id);
    if (!entry || !state || typeof state !== 'object') return false;
    const json = JSON.stringify(state);
    if (json.length > 200000) return true; // something's wrong; don't bloat the save file
    entry.tab.manualState = JSON.parse(json);
    clearTimeout(manualSave.timer);
    manualSave.timer = setTimeout(my.saveWorkspaces, 1000);
    return true;
  }

  function destroyTab(tab) {
    if (!tab.view) return;
    tab.vinToken = -1;
    try { my.win.contentView.removeChildView(tab.view); } catch {}
    try { tab.view.webContents.close(); } catch {}
    tab.view = null;
  }
};
