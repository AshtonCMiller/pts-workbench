// Vehicles (workspaces) and their tabs: the Service Manual and its tabs (OASIS, Diagnostics, TSBs, Workshop Manual,
// PC/ED, Wiring Diagrams, Labor Times), vehicles put to sleep when not used, loading ahead, opening a vehicle.
'use strict';
const { FORD_LINKS, isFordVehicle } = require('../app/config');
const { snapshotFrames } = require('../app/snapshot');

module.exports = function install(my) {
  Object.assign(my, { topTab, subTab, currentTab, setActive, upgradeTabs, addTab, addPcedTab, touchActive, prefetchVehicle, openWorkspace }); // used by the window's other parts

  // ---------- workspaces ----------
  // A vehicle's tabs: Xtime | PTS | Service Manual (+ any added). "Service Manual" holds its own tabs:
  // OASIS | Diagnostics | Workshop Manual | Labor Times (the app's own pages).
  const SVC_KINDS = new Set(['oasisview', 'diag', 'tsb', 'manual', 'pced', 'wiring', 'labor', 'article', 'doc']); // article: Ford's page for an FSA etc. · doc: one of its PDFs
  const SVC_DEFAULTS = ['oasisview', 'diag', 'tsb', 'manual', 'wiring', 'labor'];
  my.SVC_DEFAULTS = SVC_DEFAULTS;
  const inService = (kind) => SVC_KINDS.has(kind);
  my.inService = inService;
  // The Service Manual's main tabs (OASIS, Diagnostics, TSBs, Workshop Manual, Wiring Diagrams, Labor Times) can't be
  // closed (nor Service Manual itself, which would close them); extra ones (a second Workshop Manual, an FSA, a TSB…) can
  // PC/ED (only for vehicles that have one) counts as a main tab too once it's there
  const isFixedTab = (ws, t) => t.kind === 'service' || ([...SVC_DEFAULTS, 'pced'].includes(t.kind) && ws.tabs.find((x) => x.kind === t.kind) === t);
  my.isFixedTab = isFixedTab;
  const serviceTab = (ws) => ws.tabs.find((t) => t.kind === 'service') || null;
  my.serviceTab = serviceTab;
  // The vehicle's chosen tab in the top row
  function topTab(ws) {
    return ws.tabs.find((t) => t.id === ws.activeTab && !inService(t.kind)) || ws.tabs.find((t) => !inService(t.kind)) || null;
  }
  // The Service Manual's chosen tab
  function subTab(ws) {
    return ws.tabs.find((t) => t.id === ws.svcTab && inService(t.kind)) || ws.tabs.find((t) => inService(t.kind)) || null;
  }
  // The page showing for this vehicle
  function currentTab(ws) {
    const top = topTab(ws);
    return top?.kind === 'service' ? subTab(ws) : top;
  }
  function ensureService(ws) {
    let svc = serviceTab(ws);
    if (svc) return svc;
    svc = { id: my.nextTabId++, kind: 'service', title: 'Service Manual', view: null, status: '' };
    // after Xtime and PTS
    const after = Math.max(...['xtime', 'pts'].map((k) => ws.tabs.map((t) => t.kind).lastIndexOf(k)));
    ws.tabs.splice(after + 1, 0, svc);
    return svc;
  }
  function setActive(ws, tab) {
    if (inService(tab.kind)) { ws.svcTab = tab.id; ws.activeTab = ensureService(ws).id; }
    else ws.activeTab = tab.id;
  }
  // A new vehicle: Xtime | PTS | Service Manual (OASIS | Diagnostics | Workshop Manual | Labor Times);
  // anything else: Xtime | ProDemand
  function addDefaultTabs(ws) {
    addTab(ws, 'xtime', { activate: false });
    addTab(ws, 'xview', { activate: false }); // the app's own view of the Xtime RO (trial: next to Xtime's page for now)
    if (isFordVehicle(ws.vin, ws.ymm)) {
      addTab(ws, 'pts', { activate: false });
      SVC_DEFAULTS.forEach((k, i) => addTab(ws, k, { activate: i === 0 })); // opens on Service Manual › OASIS
    } else {
      addTab(ws, 'prodemand');
    }
  }
  // Saved before this layout: Xtime and PTS first, the app's pages gathered into Service Manual, the rest after
  function upgradeTabs(ws, wasActive) {
    ws.tabs = ws.tabs.filter((t) => t.kind !== 'service');
    if (!ws.tabs.some((t) => t.kind === 'xtime')) addTab(ws, 'xtime', { activate: false, front: true });
    if (isFordVehicle(ws.vin, ws.ymm)) {
      if (!ws.tabs.some((t) => t.kind === 'pts')) addTab(ws, 'pts', { activate: false });
      for (const k of SVC_DEFAULTS) if (!ws.tabs.some((t) => t.kind === k)) addTab(ws, k, { activate: false });
    }
    const first = ['xtime', 'pts'].map((k) => ws.tabs.find((t) => t.kind === k)).filter(Boolean);
    const svc = [...SVC_DEFAULTS.map((k) => ws.tabs.find((t) => t.kind === k)).filter(Boolean), ...ws.tabs.filter((t) => inService(t.kind) && !SVC_DEFAULTS.includes(t.kind))];
    const rest = ws.tabs.filter((t) => !first.includes(t) && !svc.includes(t));
    const unique = (list) => list.filter((t, i) => list.indexOf(t) === i);
    ws.tabs = unique([...first, ...svc, ...rest]);
    if (svc.length) ensureService(ws);
    const was = ws.tabs.find((t) => t.id === wasActive);
    if (was) setActive(ws, was); else ws.activeTab = topTab(ws)?.id ?? null;
  }

  const TAB_NAMES = { xview: 'Xtime View', tsb: 'TSBs', article: 'Article', doc: 'Document', service: 'Service Manual', labor: 'Labor Times', diag: 'Diagnostics', xtime: 'Xtime', oasisview: 'OASIS', manual: 'Workshop Manual', pced: 'PC/ED', wiring: 'Wiring Diagrams', pts: 'PTS', ota: 'OTA Updates', cvd: 'Connected Data', prodemand: 'ProDemand', oasis: 'OASIS' };
  my.TAB_NAMES = TAB_NAMES;
  function addTab(ws, kind, { activate = true, front = false } = {}) {
    const n = ws.tabs.filter((t) => t.kind === kind).length;
    const base = TAB_NAMES[kind] || 'Manual';
    const tab = { id: my.nextTabId++, kind, title: n ? `${base} ${n + 1}` : base, view: null, status: '' };
    if (front) ws.tabs.unshift(tab); else ws.tabs.push(tab);
    if (inService(kind)) ensureService(ws);
    if (activate) setActive(ws, tab);
    return tab;
  }

  // PC/ED tab: only for vehicles that have a PC/ED manual (mostly older ones), right after Workshop Manual
  function addPcedTab(ws) {
    const t = addTab(ws, 'pced', { activate: false });
    ws.tabs.splice(ws.tabs.indexOf(t), 1);
    const man = ws.tabs.findIndex((x) => x.kind === 'manual');
    const after = man >= 0 ? man : ws.tabs.map((x) => inService(x.kind)).lastIndexOf(true);
    ws.tabs.splice(after + 1, 0, t);
    return t;
  }
  async function checkPced(ws) {
    if (ws.tabs.some((t) => t.kind === 'pced') || !serviceTab(ws) || ws.pcedChecked === ws.vin) return;
    try {
      const has = await my.manualService.hasPced(ws.vin);
      ws.pcedChecked = ws.vin;
      if (!has || my.workspaces.get(ws.ro) !== ws || ws.tabs.some((t) => t.kind === 'pced')) return;
      addPcedTab(ws);
      my.manualService.log(`RO ${ws.ro}: has a PC/ED manual, tab added`);
      my.saveWorkspaces(); my.layout(); my.pushState();
    } catch { /* checked again next time it's opened */ }
  }

  // quiet: opened by sync from another computer, so don't switch what you're looking at
  // ---------- vehicles asleep, and loading ahead ----------
  // A vehicle you haven't looked at for 10 minutes goes to sleep: its pages are closed to free memory
  // (PTS pages keep a snapshot of where they were, Xtime its address, the app's pages their saved
  // state) and come back when you open it again. Its Workshop Manual stays open, so it never loses
  // its place. Sleeping vehicles show greyed out in the list.
  const SLEEP_AFTER = 10 * 60 * 1000;
  function touchActive() {
    const ws = my.mode === 'manual' ? my.workspaces.get(my.activeRO) : null;
    if (ws) ws.lastUsed = Date.now();
  }
  async function sleepWorkspace(ws) {
    const tabs = ws.tabs.filter((t) => t.view && t.kind !== 'manual');
    if (!tabs.length) { ws.asleep = true; return; }
    for (const t of tabs) {
      const wc = t.view.webContents;
      if (wc.isDestroyed()) { t.view = null; continue; }
      try {
        if (FORD_LINKS[t.kind]) t.cloneSnapshot = await snapshotFrames(wc); // the PTS page, scroll and all
        else if (t.kind === 'xtime' && /^https:/i.test(wc.getURL())) t.resumeUrl = wc.getURL();
      } catch { /* it starts fresh instead */ }
      if (my.workspaces.get(ws.ro) !== ws || (ws.ro === my.activeRO && my.mode === 'manual')) return; // looked at meanwhile
      my.destroyTab(t);
    }
    ws.asleep = true;
    ws.aheadAt = 0; // fetch ahead again when it wakes
    my.pushState();
  }
  setInterval(() => {
    touchActive();
    const now = Date.now();
    for (const ws of my.workspaces.values()) {
      if (!ws.lastUsed) ws.lastUsed = now; // just loaded: give it the full 10 minutes
      if (ws.asleep || (ws.ro === my.activeRO && my.mode === 'manual') || now - ws.lastUsed < SLEEP_AFTER) continue;
      sleepWorkspace(ws).catch(() => {});
    }
  }, 60 * 1000);

  // When a vehicle is opened or clicked, fetch what its Service Manual tabs show (manual contents, OASIS,
  // connected vehicle data, TSBs, the SVC list, SLTS) in the background, so the tabs open straight away.
  // Tabs that are already loaded are skipped, and so is anything fetched ahead that hasn't been used yet.
  // The vehicle just clicked goes first; one vehicle is fetched at a time.
  let prefetchQueue = [];
  let prefetchBusy = false;
  function prefetchVehicle(ws) {
    if (!ws) return;
    ws.aheadAt = Date.now();
    if (!isFordVehicle(ws.vin, ws.ymm)) { my.manualService.log(`ahead RO ${ws.ro}: not a Ford/Lincoln, nothing to fetch`); return; }
    prefetchQueue = [ws, ...prefetchQueue.filter((w) => w !== ws)];
    if (ws.fetching !== 'loading') { ws.fetching = 'queued'; my.pushState(); } // shown next to the RO
    if (!prefetchBusy) runPrefetch();
  }
  async function runPrefetch() {
    prefetchBusy = true;
    while (prefetchQueue.length) {
      const ws = prefetchQueue.shift();
      if (my.workspaces.get(ws.ro) !== ws) { ws.fetching = ''; continue; } // closed meanwhile
      const loaded = new Set(ws.tabs.filter((t) => t.view).map((t) => t.kind));
      const pacode = String(my.settings.dealerCode || '').replace(/^[A-Z]+/i, '');
      my.manualService.log(`ahead RO ${ws.ro}: starting${loaded.size ? ` (already open: ${[...loaded].join(', ')})` : ''}`);
      ws.fetching = 'loading'; my.pushState();
      try { await my.manualService.prefetch(ws.vin, { pacode, skip: loaded }); } catch (e) { my.manualService.log(`ahead RO ${ws.ro} failed: ${e.message || e}`); }
      await checkPced(ws);
      ws.fetching = prefetchQueue.includes(ws) ? 'queued' : ''; my.pushState();
    }
    prefetchBusy = false;
  }

  function openWorkspace({ ro, vin, ymm, tab, quiet = false }) {
    ro = String(ro).trim();
    vin = String(vin).trim().toUpperCase();
    if (!ro || vin.length !== 17) return;
    let ws = my.workspaces.get(ro);
    if (!ws) {
      ws = { ro, vin, ymm: ymm || '', tabs: [], activeTab: null, svcTab: null, lastUsed: Date.now() };
      my.workspaces.set(ro, ws);
      addDefaultTabs(ws);
      setTimeout(() => prefetchVehicle(ws), 300);
    } else if (ws.vin !== vin) {
      ws.vin = vin; // VIN corrected in Xtime: re-enter it in open manual tabs
      for (const t of ws.tabs) if (FORD_LINKS[t.kind] && t.view) my.reopenFordTab(ws, t);
      for (const t of ws.tabs) {
        if (t.kind === 'manual' || t.kind === 'pced') t.manualState = null; // the open procedures were for the old VIN
        if (t.kind === 'diag') t.manualState = null;
        if (t.kind === 'tsb') t.manualState = null;
        if ((t.kind === 'manual' || t.kind === 'pced' || t.kind === 'wiring' || t.kind === 'oasisview' || t.kind === 'diag' || t.kind === 'tsb') && t.view) my.destroyTab(t); // reopen for the right vehicle
      }
    }
    else if (!quiet) prefetchVehicle(ws); // already open: fetch what its tabs haven't loaded yet
    if (tab) { // e.g. from the dashboard: show the RO's Xtime page
      const t = ws.tabs.find((x) => x.kind === tab) || (tab === 'xtime' ? addTab(ws, 'xtime', { activate: false }) : null);
      if (t) setActive(ws, t);
    }
    if (!quiet || !my.activeRO) my.activeRO = ro;
    if (!quiet) my.mode = 'manual';
    my.saveWorkspaces();
    my.layout();
    my.pushState();
  }
};
