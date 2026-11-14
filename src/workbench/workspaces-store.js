// Open vehicles saved with the window (only RO / VIN / vehicle and the tab layout, no customer data) and restored on
// start, and open ROs synced between computers (RO number and year / make / model only).
'use strict';
const S = require('../app/state'); // shared app state (see app/state.js)

module.exports = function install(my) {
  Object.assign(my, { saveWorkspaces, syncOpenList, reconcileOpenROs, retrySyncedROs, loadWorkspaces }); // used by the window's other parts

  // ---------- persistence (only RO/VIN/vehicle + tab layout; no customer data) ----------
  function saveWorkspaces() {
    my.writeJson('workspaces.json', {
      activeRO: my.activeRO,
      hidden: [...my.hiddenROs],
      xtimeName: my.xtimeName,
      list: [...my.workspaces.values()].map((ws) => {
        const kept = ws.tabs.filter((t) => t.kind !== 'doc'); // downloaded PDFs aren't kept
        return {
        ro: ws.ro, vin: ws.vin, ymm: ws.ymm,
        tabs: kept.map((t) => ({ kind: t.kind, title: t.title, ...(t.url ? { url: t.url } : {}), ...(t.manualState ? { state: t.manualState } : {}) })),
        activeIndex: Math.max(0, kept.findIndex((t) => t.id === ws.activeTab)),
        svcIndex: kept.findIndex((t) => t.id === ws.svcTab),
        layout: 6,
        };
      }),
    });
    syncOpenList();
  }

  // ---------- open ROs, synced between computers (RO number + year/make/model only) ----------
  // The tracker keeps the list of open ROs so sync carries it; the VIN never syncs, so a computer
  // that gets an RO from elsewhere finds the VIN itself (this computer's records, your Xtime list,
  // or an Xtime lookup). ROs it can't open yet wait here and are retried after each Xtime refresh.
  const syncWaiting = new Map(); // ro → ymm
  my.syncWaiting = syncWaiting;
  const syncOpening = new Set();
  function syncOpenList() {
    let t;
    try { t = my.tracker; } catch { return; } // not created yet
    if (!t) return;
    const next = {};
    for (const ws of my.workspaces.values()) next[ws.ro] = { ymm: ws.ymm || '' };
    for (const [ro, ymm] of syncWaiting) if (!next[ro]) next[ro] = { ymm };
    if (JSON.stringify(next) !== JSON.stringify(t.data.openROs || {})) { t.data.openROs = next; t.save(); }
  }
  function reconcileOpenROs() {
    const want = my.tracker.data.openROs || {};
    let changed = false;
    // closed on another computer
    let keep = false;
    for (const ro of [...my.workspaces.keys()]) {
      if (ro in want) continue;
      if (my.tracker.data.openClosedBy?.[ro] === 'app') { my.handlers['close-ro'](ro); changed = true; }
      else keep = true; // dropped by an older app version (or never known there): keep it open, and say so again
    }
    if (keep) syncOpenList();
    for (const ro of [...syncWaiting.keys()]) if (!(ro in want)) syncWaiting.delete(ro);
    // opened on another computer
    for (const [ro, w] of Object.entries(want)) {
      if (my.workspaces.has(ro) || syncOpening.has(ro)) continue;
      syncWaiting.set(ro, w?.ymm || '');
      openSyncedRO(ro, w?.ymm || '');
      changed = true;
    }
    if (changed) my.pushState();
  }
  async function openSyncedRO(ro, ymm) {
    const mine = my.myROs.find((r) => r.ro === ro);
    let vin = my.tracker.data.ros?.[ro]?.vin || mine?.vin || '';
    if (!ymm && mine) ymm = [mine.year, mine.make, mine.model].filter(Boolean).join(' ');
    if (!vin) {
      const frame = my.findXtimeDataFrame();
      if (!frame) return; // waits for Xtime
      syncOpening.add(ro);
      try {
        const res = await frame.executeJavaScript(
          `${S.XTIME_VEHICLE}(${JSON.stringify(ro)}, ${JSON.stringify(my.settings.xtimeApiPath)}, ${JSON.stringify(my.settings.xtimeDetailsPath)})`, true);
        if (res?.ok && res.vin) { vin = res.vin; ymm = ymm || res.ymm || ''; }
      } catch { /* tried again after the next Xtime refresh */ } finally { syncOpening.delete(ro); }
    }
    if (!vin || !(ro in (my.tracker.data.openROs || {})) || my.workspaces.has(ro)) return;
    syncWaiting.delete(ro);
    my.openWorkspace({ ro, vin, ymm, quiet: true });
  }
  function retrySyncedROs() { for (const [ro, ymm] of syncWaiting) if (!my.workspaces.has(ro) && !syncOpening.has(ro)) openSyncedRO(ro, ymm); }

  function loadWorkspaces() {
    const saved = my.readJson('workspaces.json', null);
    for (const ro of saved?.hidden || []) my.hiddenROs.add(String(ro));
    if (saved?.xtimeName) my.xtimeName = String(saved.xtimeName);
    if (!saved?.list) return;
    for (const w of saved.list) {
      const tabs = (w.tabs?.length ? w.tabs : [{ kind: 'pts', title: 'PTS' }])
        .map((t) => ({ id: my.nextTabId++, kind: t.kind, title: t.title, view: null, status: '', manualState: t.state || null, ...(t.url ? { url: String(t.url) } : {}) }))
        .filter((t) => t.kind !== 'doc' && (t.kind !== 'article' || /^https:/i.test(t.url || ''))); // a Ford article (FSA…) opened from OASIS
      const ws = { ro: w.ro, vin: w.vin, ymm: w.ymm || '', tabs, activeTab: tabs[w.activeIndex || 0]?.id ?? tabs[0].id,
        svcTab: w.svcIndex >= 0 ? tabs[w.svcIndex]?.id ?? null : null };
      if (!(w.layout >= 3)) my.upgradeTabs(ws, ws.activeTab); // saved before Xtime | PTS | Service Manual
      else if (w.layout < 4 && my.serviceTab(ws) && !ws.tabs.some((t) => t.kind === 'tsb')) { // saved before the TSBs tab
        const t = my.addTab(ws, 'tsb', { activate: false });
        ws.tabs.splice(ws.tabs.indexOf(t), 1);
        const diag = ws.tabs.findIndex((x) => x.kind === 'diag');
        ws.tabs.splice(diag >= 0 ? diag + 1 : ws.tabs.length, 0, t);
      }
      if (w.layout >= 3 && w.layout < 5 && my.serviceTab(ws)) { // saved before Wiring Diagrams was in the Service Manual
        const t = ws.tabs.find((x) => x.kind === 'wiring') || my.addTab(ws, 'wiring', { activate: false });
        if (t.title === 'Wiring') t.title = my.TAB_NAMES.wiring;
        ws.tabs.splice(ws.tabs.indexOf(t), 1);
        const man = ws.tabs.findIndex((x) => x.kind === 'manual');
        const lab = ws.tabs.findIndex((x) => x.kind === 'labor');
        ws.tabs.splice(man >= 0 ? man + 1 : lab >= 0 ? lab : ws.tabs.length, 0, t);
      }
      if (!(w.layout >= 6) && !ws.tabs.some((x) => x.kind === 'xview')) { // saved before the app's own Xtime RO page
        const t = my.addTab(ws, 'xview', { activate: false });
        ws.tabs.splice(ws.tabs.indexOf(t), 1);
        const xt = ws.tabs.findIndex((x) => x.kind === 'xtime');
        ws.tabs.splice(xt >= 0 ? xt + 1 : 0, 0, t);
      }
      my.workspaces.set(w.ro, ws);
    }
    if (saved.activeRO && my.workspaces.has(saved.activeRO)) my.activeRO = saved.activeRO;
  }
};
