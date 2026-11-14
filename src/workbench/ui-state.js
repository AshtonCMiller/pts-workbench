// Everything the app's own screen (src/ui) shows, sent to it whenever something changes: pushState.
'use strict';
const { app } = require('electron');
const S = require('../app/state'); // shared app state (see app/state.js)
const { SYNC_URL } = require('../app/config');
const { IS_BETA, update, updatePrefs } = require('../app/updates');
const { windows } = S;

module.exports = function install(my) {
  Object.assign(my, { pushState }); // used by the window's other parts

  // ---------- state to the UI ----------
  function pushState() {
    if (!my.chrome || my.chrome.webContents.isDestroyed()) return;
    const roInfo = {};
    const statusLists = my.tracker?.statusLists() ?? { review: [], completed: [] };
    for (const ro of new Set([...my.myROs.map((r) => r.ro), ...my.workspaces.keys(), ...(my.lookup ? [my.lookup.ro] : []),
      ...statusLists.review.map((r) => r.ro), ...statusLists.completed.map((r) => r.ro)])) {
      const s = my.tracker?.roSummary(ro);
      if (s) roInfo[ro] = s;
    }
    my.chrome.webContents.send('state', {
      feedback: { on: !!SYNC_URL, admin: my.isFeedbackAdmin(), newCount: my.feedbackNew },
      mode: my.mode, drawerOpen: my.drawerOpen, chatOpen: my.chatOpen, settings: my.settings, xtimeStatus: my.xtimeStatus, lastPoll: my.lastPoll, activeRO: my.activeRO, xtimeSignin: my.xtimeSignin, msgCount: my.msgCount, notes: my.notes,
      myROs: my.myROs, roInfo,
      tracking: my.tracker?.summary() ?? null,
      period: my.tracker?.period(my.periodOffset) ?? null,
      search: my.searchQuery && my.tracker ? { query: my.searchQuery, results: my.tracker.searchRO(my.searchQuery) } : null,
      currentPeriodTotal: my.tracker?.period(0).total ?? 0,
      lookup: my.lookup,
      employeeStatus: my.employeeStatus,
      statusLists,
      addLookup: my.addLookup,
      hidden: [...my.hiddenROs],
      xtimeViewingRO: my.xtimeViewingRO,
      logins: my.readLogins().map((l) => ({ host: l.host, username: l.username })), // never the passwords
      canSavePasswords: my.canEncrypt(),
      sync: { ...my.syncStatus, configured: !!SYNC_URL, enabled: !!my.settings.syncEnabled, isMain: true, choice: my.syncChoice },
      app: { version: app.getVersion(), update: { ...update }, beta: IS_BETA, betaUpdates: !!updatePrefs.beta, whatsNew: S.whatsNew },
      profile: { id: my.profile.id, name: my.profile.name, xtimeName: my.xtimeName, displayName: my.displayName() },
      profiles: S.profiles.map((p) => ({
        id: p.id, name: p.name, open: windows.has(p.id), current: p.id === my.profile.id,
        displayName: windows.get(p.id)?.displayName() || p.name,
      })),
      // Each vehicle's top row of tabs, and the Service Manual's own tabs
      workspaces: [...my.workspaces.values()].map((ws) => {
        const pick = (t) => ({ id: t.id, kind: t.kind, title: t.title, status: t.status, fixed: my.isFixedTab(ws, t) });
        return {
          ro: ws.ro, vin: ws.vin, ymm: ws.ymm, activeTab: my.topTab(ws)?.id ?? null, viewTab: my.currentTab(ws)?.id ?? null, asleep: !!ws.asleep || !ws.tabs.some((t) => t.view), /* not opened since the app started: asleep too */ fetching: ws.fetching || '',
          tabs: ws.tabs.filter((t) => !my.inService(t.kind)).map(pick),
          sub: my.serviceTab(ws) ? { active: my.subTab(ws)?.id ?? null, tabs: ws.tabs.filter((t) => my.inService(t.kind)).map(pick) } : null,
        };
      }),
    });
  }
};
