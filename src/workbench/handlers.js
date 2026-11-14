// What the app's own screen asks for: every button, setting, tab and dialog (src/ui/ui.js sends these by name).
'use strict';
const fs = require('fs');
const { app, clipboard, shell } = require('electron');
const S = require('../app/state'); // shared app state (see app/state.js)
const { userFile } = require('../app/files');
const { pushAll, saveProfiles } = S;
const { snapshotFrames } = require('../app/snapshot');
const { BOOLEAN_SETTINGS, DEFAULT_SETTINGS, FIXED_SETTINGS, FORD_LINKS, NUMBER_SETTINGS, cleanDealerCode } = require('../app/config');
const { installUpdate, markWhatsNewSeen, setBetaUpdates, update } = require('../app/updates');
const { setKeepAwake } = require('../app/keep-awake');

module.exports = function install(my) {
  // Shared (tracker) changes refresh every window; window-only changes refresh just this one.
  const handlers = {
    find: ({ text, next, back } = {}) => { // from the find bar: typing starts a new search, Enter / arrows go to the next match
      if (!my.findWc || my.findWc.isDestroyed()) return;
      text = String(text || '').slice(0, 200);
      if (!text) { my.findWc.stopFindInPage('clearSelection'); my.sendFind({ result: true, active: 0, matches: 0, empty: true }); return; }
      my.findWc.findInPage(text, { forward: !back, findNext: !next, matchCase: false });
    },
    'find-close': () => { const wc = my.closeFind({ quiet: true }); if (wc) wc.focus(); },
    'ui-ready': () => { my.pushState(); global.ptsBoot?.ok?.(); }, // the app is up: these app files work
    'set-mode': (m) => { my.mode = m === 'manual' ? 'manual' : 'xtime'; my.layout(); my.pushState(); },
    // The messages counter: Xtime's chat is on Xtime's own page for now
    'open-messages': () => {
      my.mode = 'xtime';
      if (my.settings.xtimeDashboard) { my.settings = { ...my.settings, xtimeDashboard: false }; my.writeJson('settings.json', my.settings); }
      my.unparkXtime('Opening Xtime…');
      my.chatOpen = false;
      my.layout(); my.pushState();
    },
    // The app's own read-only Dealership chat (Xtime's chat gateway, read through the Inspect page).
    // Messages are shown and forgotten: never saved to a file, never synced, nothing is sent or marked read.
    'chat-toggle': ({ open } = {}) => { my.chatOpen = open == null ? !my.chatOpen : !!open; my.layout(); my.pushState(); },
    // your messages from the last 2 weeks, plus everything (about 4 months back) on ROs assigned to you
    'chat-list': async () => my.chatRun({ type: 'list', days: 14, roDays: 120, ros: [...new Set([...my.myROs.map((r) => r.ro), ...my.workspaces.keys()])] }, 'list'),
    'chat-history': async ({ channelId } = {}) => { if (channelId) my.chatRun({ type: 'history', channelId: String(channelId) }, 'history'); },
    'chat-roster': async () => my.chatRun({ type: 'roster' }, 'roster'),
    // Sends only what you typed, only when you press Send, to a conversation that already exists
    'chat-send': async ({ channelId, ro, text } = {}) => {
      text = String(text || '').slice(0, 2000);
      if (!channelId || !text.trim()) return;
      await my.chatRun({ type: 'send', channelId: String(channelId), ro: String(ro || ''), text }, 'send');
      setTimeout(() => my.chatRun({ type: 'history', channelId: String(channelId) }, 'history'), 1500);
    },
    // The dashboard's expanded RO: its lines, and the Done button
    'ro-lines': async ({ ro } = {}) => {
      ro = String(ro || '');
      const reply = (r) => { if (my.chrome && !my.chrome.webContents.isDestroyed()) my.chrome.webContents.send('ro-lines', { ro, ...r }); };
      const frame = my.findXtimeDataFrame();
      if (!frame) return reply({ ok: false, error: 'Xtime is still loading' });
      try {
        const r = await frame.executeJavaScript(`${S.XTIME_RO}(${JSON.stringify(my.settings.storeId)}, ${JSON.stringify(ro)}, {"type":"get"})`, true);
        if (r?.fieldNames && !my.loggedLineFields) {
          my.loggedLineFields = true;
          try { fs.writeFileSync(userFile('xtime-line-fields.txt'), `${new Date().toISOString()}\n${r.fieldNames.join('\n')}\n`); } catch {}
        }
        reply(r?.ok ? { ok: true, lines: r.lines } : { ok: false, error: r?.error || 'Could not read Xtime' });
      } catch (e) { reply({ ok: false, error: e.message || String(e) }); }
    },
    'ro-line-done': async ({ ro, id, value } = {}) => {
      const frame = my.findXtimeDataFrame();
      if (!frame || id == null) return;
      try {
        const r = await frame.executeJavaScript(
          `${S.XTIME_RO}(${JSON.stringify(my.settings.storeId)}, ${JSON.stringify(String(ro))}, ${JSON.stringify({ type: 'done', id, value: !!value })})`, true);
        if (!r?.ok) my.notice(`Couldn't update the line in Xtime: ${r?.error || 'error'}`);
      } catch (e) { my.notice(`Couldn't update the line in Xtime: ${e.message || e}`); }
      handlers['ro-lines']({ ro });
    },
    // The app's "Sign in to Xtime" form: username and password (both steps), a code, a choice, or a page button
    'xtime-signin': async ({ username, password, remember, code, choose, press } = {}) => {
      if (!my.xtimeSignin || my.xtimeSignin.busy || my.xtimeSignin.step === 'starting' || my.xtimeSignin.step === 'loading') return;
      my.signinActedAt = Date.now();
      my.xtimeSignin = { ...my.xtimeSignin, busy: true, error: '' }; my.pushState();
      let res = null;
      try {
        if (password != null) {
          // Left blank with a saved login: use the saved password (it never goes to the screen)
          if (!password && my.xtimeSignin?.host) {
            const saved = my.getLogin(my.xtimeSignin.host);
            if (saved?.password && (!username || !saved.username || saved.username.toLowerCase() === String(username).toLowerCase())) {
              password = saved.password; username = username || saved.username; remember = false;
            }
          }
          if (!password) { res = { ok: false, error: 'Enter your password' }; return; }
          let st = await my.signinRun({ type: 'state' });
          if (st?.step === 'username') {
            res = await my.signinRun({ type: 'username', value: String(username || '') });
            if (res?.ok !== false) st = await my.waitForStep(['password', 'code', 'choose', 'other']);
          }
          if (st?.step === 'password' && !st.error) {
            res = await my.signinRun({ type: 'password', value: String(password) });
            if (remember && res?.ok !== false && my.xtimeSignin?.host) my.saveLogin(my.xtimeSignin.host, String(username || st.username || ''), String(password));
            await my.waitForStep(['code', 'choose', 'eula', 'duplicate', 'other'], 6000);
          } else if (st?.error) res = st;
        } else if (code != null) {
          res = await my.signinRun({ type: 'code', value: String(code).replace(/\s+/g, '') });
          await my.waitForStep(['other', 'eula', 'duplicate'], 6000);
        } else if (choose != null) {
          res = await my.signinRun({ type: 'choose', index: Number(choose) });
          await my.waitForStep(['code'], 6000);
        } else if (press) {
          res = await my.signinRun({ type: 'press', text: String(press) });
        }
      } finally {
        if (my.xtimeSignin) my.xtimeSignin = { ...my.xtimeSignin, busy: false, error: res?.ok === false ? res.error : (res?.error || '') };
        await my.checkSignin();
        my.pushState();
      }
    },
    'toggle-dashboard': () => {
      my.settings = { ...my.settings, xtimeDashboard: !my.settings.xtimeDashboard };
      my.writeJson('settings.json', my.settings);
      if (!my.settings.xtimeDashboard) my.unparkXtime('Opening Xtime…');
      my.layout(); my.pushState();
    },
    'toggle-drawer': () => { my.drawerOpen = !my.drawerOpen; my.layout(); my.pushState(); },
    // A dialog opens / closes in the app's screen. The live page can't sit under it (it would cover the dialog), so a
    // picture of it is put in its place first — the dialog then dims it instead of the page going black.
    'set-overlay': async (on) => {
      const seq = ++my.overlaySeq;
      if (on && !my.overlay && my.visibleView && !my.visibleView.webContents.isDestroyed()) {
        try {
          const b = my.visibleView.getBounds();
          const img = await my.visibleView.webContents.capturePage();
          if (seq !== my.overlaySeq) return; // closed again meanwhile
          if (!img.isEmpty() && my.chrome && !my.chrome.webContents.isDestroyed()) {
            my.chrome.webContents.send('overlay-shot', { url: `data:image/jpeg;base64,${img.toJPEG(85).toString('base64')}`, x: b.x, y: b.y, width: b.width, height: b.height });
          }
        } catch { /* no picture: the area is just empty */ }
      }
      if (seq !== my.overlaySeq) return;
      my.overlay = !!on;
      if (!my.overlay && my.chrome && !my.chrome.webContents.isDestroyed()) my.chrome.webContents.send('overlay-shot', null);
      my.layout();
    },
    // ↻: refresh the list; if Xtime has been failing, start the Xtime page fresh instead
    'refresh-ros': () => {
      if (my.xtimeFailures > 0 || (!my.findXtimeDataFrame() && !my.onLoginPage())) { my.reloadXtime('Reloading Xtime…'); my.startLite(); }
      else my.pollXtime();
    },
    // Review / Completed
    'mark-completed': (ro) => {
      ro = String(ro);
      my.tracker.markCompleted(ro);
      if (my.workspaces.has(ro)) handlers['close-ro'](ro); // frees its tabs; it can be opened again from Completed
      my.notice(`RO ${ro} moved to Completed`);
      pushAll();
    },
    'move-back': (ro) => { my.tracker.moveBack(String(ro)); my.notice(`RO ${ro} moved back to Active`); pushAll(); },
    'open-any': async (ro) => {
      ro = String(ro);
      if (my.workspaces.has(ro)) { handlers['select-ro'](ro); return; }
      const rec = my.tracker.data.ros[ro];
      await handlers['add-vehicle']({ ro, vin: rec?.vin || '', ymm: rec?.ymm || '' });
      if (my.addLookup?.ro === ro && my.addLookup.status === 'error') { my.notice(my.addLookup.message); my.addLookup = null; my.pushState(); }
    },
    'open-ro': (payload) => my.openWorkspace(payload),
    // "Add vehicle" with the VIN left blank: get the VIN (and vehicle) from Xtime, then open it
    'add-vehicle': async ({ ro, vin, ymm }) => {
      ro = String(ro || '').trim();
      vin = String(vin || '').trim().toUpperCase();
      if (!/^\w{1,20}$/.test(ro)) return;
      if (vin) { my.addLookup = null; my.openWorkspace({ ro, vin, ymm }); return; }
      const known = my.myROs.find((r) => r.ro === ro) || (my.workspaces.get(ro) && { vin: my.workspaces.get(ro).vin, ymm: my.workspaces.get(ro).ymm });
      if (known?.vin) { my.addLookup = null; my.openWorkspace({ ro, vin: known.vin, ymm: ymm || known.ymm || [known.year, known.make, known.model].filter(Boolean).join(' ') }); return; }
      const frame = my.findXtimeDataFrame();
      if (!frame) {
        my.addLookup = { ro, status: 'error', message: "Xtime isn't logged in, so the VIN can't be looked up. Log in to Xtime or type the VIN." };
        my.pushState(); return;
      }
      my.addLookup = { ro, status: 'loading', message: `Looking up RO ${ro} in Xtime…` };
      my.pushState();
      let res = null;
      try {
        res = await frame.executeJavaScript(
          `${S.XTIME_VEHICLE}(${JSON.stringify(ro)}, ${JSON.stringify(my.settings.xtimeApiPath)}, ${JSON.stringify(my.settings.xtimeDetailsPath)})`, true);
      } catch { res = null; }
      if (my.addLookup?.ro !== ro) return; // dialog closed or another RO asked for meanwhile
      if (!res?.ok) {
        my.addLookup = { ro, status: 'error', message: `${res?.error || "Couldn't reach Xtime"}. Type the VIN to open it anyway.` };
        my.pushState(); return;
      }
      my.addLookup = null;
      my.openWorkspace({ ro, vin: res.vin, ymm: ymm || res.ymm });
      // and load its lines straight away
      try {
        const d = await frame.executeJavaScript(
          `${S.XTIME_DETAILS}(${JSON.stringify(ro)}, ${JSON.stringify(my.settings.xtimeDetailsPath)}, ${JSON.stringify(my.settings.xtimeApiPath)})`, true);
        if (d?.ok) { my.tracker.updateLines(ro, d.lines, ymm || res.ymm); pushAll(); }
      } catch { /* the next refresh will try again */ }
    },
    'cancel-add': () => { my.addLookup = null; my.pushState(); },
    'hide-ro': (ro) => { my.hiddenROs.add(String(ro)); my.saveWorkspaces(); my.pushState(); },
    'unhide-ro': (ro) => { my.hiddenROs.delete(String(ro)); my.saveWorkspaces(); my.pushState(); },
    'unhide-all': () => { my.hiddenROs.clear(); my.saveWorkspaces(); my.pushState(); },
    'reorder-ws': (order) => {
      if (!Array.isArray(order)) return;
      const keep = order.map(String);
      const entries = [
        ...keep.filter((ro) => my.workspaces.has(ro)).map((ro) => [ro, my.workspaces.get(ro)]),
        ...[...my.workspaces].filter(([ro]) => !keep.includes(ro)),
      ];
      my.workspaces.clear();
      for (const [ro, ws] of entries) my.workspaces.set(ro, ws);
      my.saveWorkspaces(); my.pushState();
    },
    'select-ro': (ro) => {
      if (!my.workspaces.has(ro)) return;
      my.activeRO = ro; my.mode = 'manual';
      my.prefetchVehicle(my.workspaces.get(ro)); // whatever its tabs haven't loaded yet
      my.saveWorkspaces(); my.layout(); my.pushState();
    },
    // Shift clock and flags are shared by every window
    'clock-in': () => { my.tracker.clockIn(); pushAll(); },
    'clock-out': () => { my.tracker.clockOut(); pushAll(); },
    'set-line-hours': ({ ro, key, hours }) => { my.tracker.setOverride(String(ro), String(key), hours); pushAll(); },
    'flag-line': ({ ro, key }) => { my.tracker.flagLine(String(ro), String(key)); pushAll(); },
    'rename-line': ({ ro, key, name }) => { my.tracker.renameLine(String(ro), String(key), name); pushAll(); },
    'unflag-line': ({ ro, key }) => { my.tracker.unflagLine(String(ro), String(key)); pushAll(); },
    'update-flag': ({ id, changes }) => { my.tracker.updateFlag(Number(id), changes || {}); pushAll(); },
    'delete-flag': (id) => { my.tracker.deleteFlag(Number(id)); pushAll(); },
    'search-flags': (q) => { my.searchQuery = String(q || '').replace(/[^\w-]/g, '').slice(0, 20); my.pushState(); },
    'set-period': (offset) => { my.periodOffset = Math.min(0, Math.round(Number(offset) || 0)); my.pushState(); },
    'lookup-ro': (ro) => my.lookupRO(String(ro).trim()),
    'clear-lookup': () => { my.lookup = null; my.pushState(); },
    'add-manual-line': ({ ro, dms, complaint, hours, date }) => {
      ro = String(ro).trim();
      if (!ro) return;
      const key = my.tracker.addManualLine(ro, { dms, complaint, hours });
      // A date in the past is logged at noon that day, so it lands in the right pay period
      let when = null;
      if (date) {
        const [y, m, d] = String(date).split('-').map(Number);
        const picked = new Date(y, m - 1, d, 12).getTime();
        const today = new Date(); today.setHours(0, 0, 0, 0);
        if (Number.isFinite(picked) && picked < today.getTime()) when = picked;
      }
      my.tracker.flagLine(ro, key, when);
      if (!my.lookup || my.lookup.ro !== ro) my.lookup = { ro, status: 'ok', message: '' };
      pushAll();
    },
    ...my.feedbackHandlers,
    'toggle-sidebar': (hide) => {
      my.settings = { ...my.settings, sidebarHidden: hide == null ? !my.settings.sidebarHidden : !!hide };
      my.writeJson('settings.json', my.settings);
      my.layout(); my.pushState();
    },
    'set-sidebar': (w) => {
      my.settings.sidebarWidth = Math.round(Number(w) || 380);
      my.layout();
      clearTimeout(handlers.saveSidebarSoon);
      handlers.saveSidebarSoon = setTimeout(() => my.writeJson('settings.json', my.settings), 500);
    },
    'close-ro': (ro) => {
      const ws = my.workspaces.get(ro);
      if (!ws) return;
      ws.tabs.forEach(my.destroyTab);
      my.workspaces.delete(ro);
      if (my.activeRO === ro) my.activeRO = [...my.workspaces.keys()][0] ?? null;
      my.saveWorkspaces(); my.layout(); my.pushState();
    },
    'add-tab': async ({ ro, kind }) => {
      const ws = my.workspaces.get(ro);
      if (!ws) return;
      // "+ Manual" while looking at a PTS tab copies that tab: same vehicle, same page, same scroll.
      const src = my.currentTab(ws);
      let snapshot = null;
      const KINDS = ['service', 'labor', 'diag', 'tsb', 'oasisview', 'manual', 'wiring', 'pts', 'ota', 'cvd', 'prodemand', 'oasis', 'xtime', 'xview'];
      kind = KINDS.includes(kind) ? kind : 'pts';
      if (kind === 'pts' && src?.kind === 'pts' && src.view && !src.view.webContents.isDestroyed()) {
        try { snapshot = await snapshotFrames(src.view.webContents); } catch { snapshot = null; }
      }
      if (kind === 'service') { // Service Manual with its standard tabs (only one per vehicle)
        for (const k of my.SVC_DEFAULTS) if (!ws.tabs.some((t) => t.kind === k)) my.addTab(ws, k, { activate: false });
        my.setActive(ws, my.subTab(ws) || ws.tabs.find((t) => my.inService(t.kind)));
        my.mode = 'manual';
        my.saveWorkspaces(); my.layout(); my.pushState();
        return;
      }
      const tab = my.addTab(ws, kind);
      my.mode = 'manual';
      if (snapshot) {
        tab.cloneSnapshot = snapshot;
        tab.title = `${src.title} copy`.slice(0, 40);
      }
      my.saveWorkspaces(); my.layout(); my.pushState();
    },
    'select-tab': ({ ro, id }) => {
      const ws = my.workspaces.get(ro);
      const t = ws?.tabs.find((x) => x.id === id);
      if (!t) return;
      my.setActive(ws, t);
      my.saveWorkspaces(); my.layout(); my.pushState();
    },
    'close-tab': ({ ro, id }) => {
      const ws = my.workspaces.get(ro);
      if (!ws) return;
      const i = ws.tabs.findIndex((t) => t.id === id);
      if (i < 0) return;
      const closing = ws.tabs[i];
      if (my.isFixedTab(ws, closing)) return; // the Service Manual's main tabs stay
      // Closing Service Manual closes its tabs too; closing its last tab closes Service Manual
      const gone = closing.kind === 'service' ? ws.tabs.filter((t) => t === closing || my.inService(t.kind)) : [closing];
      if (my.inService(closing.kind) && ws.tabs.filter((t) => my.inService(t.kind)).length === 1) gone.push(my.serviceTab(ws));
      const topBefore = ws.tabs.slice(0, ws.tabs.indexOf(gone.find((t) => t && !my.inService(t.kind)) || closing)).filter((t) => !my.inService(t.kind) && !gone.includes(t)).pop();
      for (const t of gone) if (t) my.destroyTab(t);
      ws.tabs = ws.tabs.filter((t) => !gone.includes(t));
      if (!ws.tabs.length) { handlers['close-ro'](ro); return; }
      if (my.inService(closing.kind) && ws.svcTab === id) {
        const before = ws.tabs.slice(0, i).filter((t) => my.inService(t.kind)).pop();
        ws.svcTab = (before || ws.tabs.find((t) => my.inService(t.kind)))?.id ?? null;
      }
      if (!ws.tabs.some((t) => t.id === ws.activeTab)) ws.activeTab = (topBefore || ws.tabs.find((t) => !my.inService(t.kind)))?.id ?? null;
      my.saveWorkspaces(); my.layout(); my.pushState();
    },
    'rename-tab': ({ ro, id, title }) => {
      const tab = my.workspaces.get(ro)?.tabs.find((t) => t.id === id);
      if (!tab) return;
      tab.title = String(title).trim().slice(0, 40) || tab.title;
      my.saveWorkspaces(); my.pushState();
    },
    'reenter-vin': ({ ro, id }) => {
      const ws = my.workspaces.get(ro);
      const tab = ws?.tabs.find((t) => t.id === id);
      if (FORD_LINKS[tab?.kind] && tab.view) my.reopenFordTab(ws, tab);
      if (tab?.kind === 'prodemand' && tab.view) { tab.status = 'Entering VIN…'; my.pushState(); my.fillProDemand(ws, tab); }
    },
    'copy-vin': (ro) => {
      const ws = my.workspaces.get(ro);
      if (!ws) return;
      try { clipboard.writeText(ws.vin); my.notice(`VIN ${ws.vin} copied`); } catch {}
    },
    'remove-login': (host) => {
      my.writeJson('logins.json', my.readLogins().filter((l) => l.host !== String(host)));
      my.notice(`Removed saved login for ${host}`);
      my.pushState();
    },
    'print-page': () => my.printVisible(),
    // the notifications menu (the bell)
    'notes-read': () => { let changed = false; for (const n of my.notes) if (!n.read) { n.read = true; changed = true; } if (changed) { my.saveNotes(); my.pushState(); } },
    'notes-clear': () => { my.notes = []; my.saveNotes(); my.pushState(); },
    'note-open': ({ id } = {}) => {
      const n = my.notes.find((x) => x.id === id);
      if (!n) return;
      n.read = true; my.saveNotes();
      if (n.kind === 'update') { my.pushState(); return; } // the window shows its What's new itself
      my.openNoteRo(n.ro);
    },
    // What's new closed: 'installed' ones count as shown; a ready update's ('Later') shows again after the restart
    'whats-new-done': ({ id } = {}) => {
      if (!S.whatsNew || S.whatsNew.id !== id) return;
      if (S.whatsNew.reason === 'installed') markWhatsNewSeen(S.whatsNew.groups);
      S.whatsNew = null;
      pushAll();
    },
    'note-remove': ({ id } = {}) => { my.notes = my.notes.filter((x) => x.id !== id); my.saveNotes(); my.pushState(); },
    // Tabs dragged into a new order, one row at a time (the top row, or the Service Manual's row):
    // ids = that row's tab ids in the new order. They take the places that row's tabs already had.
    'reorder-tabs': ({ ro, ids }) => {
      const ws = my.workspaces.get(ro);
      if (!ws || !Array.isArray(ids)) return;
      const moving = ids.map((id) => ws.tabs.find((t) => t.id === id));
      if (!moving.length || moving.some((t) => !t) || new Set(moving).size !== moving.length) return;
      const svc = my.inService(moving[0].kind);
      const row = ws.tabs.filter((t) => my.inService(t.kind) === svc);
      if (row.length !== moving.length || moving.some((t) => my.inService(t.kind) !== svc)) return; // must be the whole row
      const spots = ws.tabs.map((t, i) => (row.includes(t) ? i : -1)).filter((i) => i >= 0);
      spots.forEach((spot, i) => { ws.tabs[spot] = moving[i]; });
      my.saveWorkspaces(); my.pushState();
    },
    'reload-tab': ({ ro, id }) => {
      const tab = my.workspaces.get(ro)?.tabs.find((t) => t.id === id);
      tab?.view?.webContents.reload();
    },
    // Start from the Xtime start page rather than re-loading the current address, which may be a
    // one-time sign-in step that fails when loaded twice
    'reload-xtime': () => my.reloadXtime('Reloading Xtime…'),
    'save-settings': (next) => {
      const clean = {};
      for (const k of Object.keys(DEFAULT_SETTINGS)) {
        if (next[k] === undefined || FIXED_SETTINGS.includes(k)) continue;
        if (BOOLEAN_SETTINGS.includes(k)) clean[k] = next[k] === true;
        else if (NUMBER_SETTINGS[k]) {
          const [lo, hi] = NUMBER_SETTINGS[k];
          clean[k] = Math.min(hi, Math.max(lo, Number(next[k]) || DEFAULT_SETTINGS[k]));
        } else clean[k] = String(next[k]).trim();
      }
      if (clean.dealerCode !== undefined) clean.dealerCode = cleanDealerCode(clean.dealerCode);
      const xtimeChanged = clean.xtimeUrl && clean.xtimeUrl !== my.settings.xtimeUrl;
      const syncWas = !!my.settings.syncEnabled;
      const empChanged = clean.employeeNumber !== undefined && clean.employeeNumber !== my.settings.employeeNumber;
      const storeChanged = clean.storeId !== undefined && clean.storeId !== my.settings.storeId;
      // New employee number or store: look the technician ID up again (and don't keep showing the old one's ROs)
      if (empChanged || storeChanged) { clean.techId = ''; clean.techIdFor = ''; my.employeeStatus = ''; my.myROs = []; }
      const techChanged = (clean.techId && clean.techId !== my.settings.techId) || empChanged || storeChanged;
      my.settings = { ...my.settings, ...clean };
      my.writeJson('settings.json', my.settings);
      setKeepAwake(my.profile.id, my.settings.keepAwake !== false);
      if (my.sync) {
        if (my.settings.syncEnabled && !syncWas) { my.syncFlushed = false; my.beginSync(); } // turned on: ask first if the server has a copy
        else if (!my.settings.syncEnabled && syncWas) { my.sync.stop(); Object.assign(my.syncStatus, { state: 'off', message: '' }); }
        else if (my.settings.syncEnabled && (empChanged || storeChanged)) my.sync.syncNow(); // new person: their own data
      }
      if (typeof next.profileName === 'string' && next.profileName.trim()) {
        my.profile.name = next.profileName.trim().slice(0, 40);
        saveProfiles();
      }
      if (techChanged) { my.xtimeName = ''; my.saveWorkspaces(); } // re-read the name for the new tech ID
      my.updateTitle();
      if (xtimeChanged) my.xtimeView.webContents.loadURL(my.settings.xtimeUrl);
      my.startPolling();
      my.pollXtime();
      pushAll();
    },
    // Restart the whole app (reopens the same windows; the shift clock keeps running)
    'restart-app': async () => {
      S.quitting = true;
      saveProfiles();
      await S.closeOutAll();
      app.relaunch();
      app.quit();
    },
    // Sync across computers
    'sync-now': () => { if (my.sync) my.sync.syncNow(); },
    'sync-choice': (choice) => my.chooseSync(['restore', 'overwrite', 'cancel'].includes(choice) ? choice : 'cancel'),
    // Updates
    'check-update': () => { if (S.updater) S.updater.checkForUpdates().catch(() => {}); },
    'set-beta-updates': (on) => setBetaUpdates(on),
    'open-update-log': () => { if (update.logFile) shell.openPath(update.logFile).catch(() => {}); },
    'install-update': () => installUpdate(),
    // Windows / profiles
    'open-profile': (id) => S.openProfileWindow(String(id)),
    'new-profile': (name) => {
      const clean = String(name || '').trim().slice(0, 40);
      if (!clean) return;
      const id = `p${Date.now().toString(36)}`;
      S.profiles.push({ id, name: clean });
      saveProfiles();
      S.openProfileWindow(id);
    },
  };
  my.handlers = handlers;
};
