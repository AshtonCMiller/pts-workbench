// Opening the window itself (the app's screen, Xtime's page, the background page, the timers) and closing it.
'use strict';
const { BaseWindow, WebContentsView } = require('electron');
const path = require('path');
const S = require('../app/state'); // shared app state (see app/state.js)
const { appUiContents, isPrintKey, prepareSession } = require('../app/web');
const { setKeepAwake } = require('../app/keep-awake');
const { pushAll, saveProfiles, windows } = S;
const SRC = path.join(__dirname, '..'); // the app's src folder

module.exports = function install(my) {
  Object.assign(my, { open }); // used by the window's other parts

  // ---------- window ----------
  function open() {
    my.loadWorkspaces();
    my.syncOpenList(); // what's open here goes into the first sync (and isn't mistaken for closed)
    my.tracker.prune(60, [...my.workspaces.keys()]);
    // Sync: "forgot to clock out" waits for the first sync (at most 15 s when offline)
    if (my.syncOn()) {
      let checked = false;
      const check = () => { if (checked) return; checked = true; my.tracker.startupCheck(); my.pushState(); };
      my.sync.start().finally(check);
      setTimeout(check, 15 * 1000);
    }
    // Heartbeat lets a crash/power loss end an open shift at the right time, and refreshes totals.
    my.heartbeatTimer = setInterval(() => { my.tracker.heartbeat(); my.pushState(); }, 60 * 1000);
    prepareSession(my.profile.id);
    my.win = new BaseWindow({ width: 1500, height: 950, minWidth: 900, minHeight: 600, title: 'PTS Workbench', autoHideMenuBar: true });
    my.updateTitle();

    my.chrome = new WebContentsView({
      webPreferences: { preload: path.join(SRC, 'preload.js'), sandbox: true, contextIsolation: true, nodeIntegration: false },
    });
    my.win.contentView.addChildView(my.chrome);
    my.chrome.webContents.loadFile(path.join(S.PAGES, 'ui', 'index.html'));
    my.chrome.webContents.setWindowOpenHandler(() => ({ action: 'deny' }));
    appUiContents.add(my.chrome.webContents);
    my.chrome.webContents.on('before-input-event', (ev, input) => {
      if (isPrintKey(input)) { ev.preventDefault(); my.printVisible(); }
      // Ctrl+F with the app's own bar focused: find in the page that's showing
      if (input.type === 'keyDown' && input.control && !input.alt && !input.shift && input.key.toLowerCase() === 'f' && my.visibleView) {
        ev.preventDefault();
        if (my.visibleKind() === 'wiring') { // Wiring Diagrams' own find
          const wc = my.visibleView.webContents;
          wc.focus();
          wc.sendInputEvent({ type: 'keyDown', keyCode: 'F', modifiers: ['control'] });
          wc.sendInputEvent({ type: 'keyUp', keyCode: 'F', modifiers: ['control'] });
        } else my.openFind();
      }
    });

    // The app's dashboard starts with "Connecting to Xtime…" until the first ROs arrive (or Xtime asks to sign in)
    if (my.settings.xtimeDashboard) my.xtimeSignin = { step: 'starting', since: Date.now(), busy: false, error: '' };
    my.xtimeView = my.makeWebView();
    my.xtimeView.webContents.loadURL(my.settings.xtimeUrl); // signs in (and shows Xtime) when needed; put away once the app works without it
    my.startLite();
    let soon = null;
    my.xtimeView.webContents.on('did-frame-finish-load', () => {
      clearTimeout(soon);
      soon = setTimeout(my.pollXtime, 2500); // e.g. right after you log in
    });
    my.xtimeView.webContents.on('did-finish-load', () => { my.xtimeLoadedAt = Date.now(); });
    // The Xtime page itself failed to load (network blip, expired sign-in step): try again shortly
    my.xtimeView.webContents.on('did-fail-load', (_e, code, _desc, _url, isMainFrame) => {
      if (!isMainFrame || code === -3) return; // -3 = cancelled by a redirect, which is normal
      my.xtimeStatus = 'Xtime page failed to load — retrying shortly';
      my.pushState();
      setTimeout(() => { if (Date.now() - my.lastRecovery > 2 * 60 * 1000) my.reloadXtime('Reloading Xtime…'); }, 20 * 1000);
    });
    // Watch which RO page is open in Xtime, so a hidden RO can be unhidden from there.
    my.watchTimer = setInterval(() => {
      my.parkCheck();
      // The background page landed somewhere else (e.g. sent to sign in before Xtime was signed in): try again once Inspect works
      if (!my.liteReady() && !my.liteLoading && my.findInspectFrame() && Date.now() - my.liteTriedAt > 30 * 1000) { my.liteTriedAt = Date.now(); my.startLite(); }
      const ro = my.viewingROFromXtime();
      if (ro !== my.xtimeViewingRO) { my.xtimeViewingRO = ro; my.pushState(); }
      // Refresh Xtime before its session runs out — only while you're not looking at the Xtime tab
      const due = Date.now() - my.xtimeLoadedAt > Number(my.settings.xtimeRefreshMinutes || 90) * 60 * 1000;
      if (due && !my.xtimeParked && (my.mode !== 'xtime' || my.overlay) && my.findInspectFrame() && Date.now() - my.lastRecovery > 5 * 60 * 1000) {
        my.reloadXtime('');
      }
    }, 1500);

    my.win.on('resize', my.layout);
    my.win.on('closed', () => {
      setKeepAwake(my.profile.id, false);
      clearInterval(my.pollTimer);
      clearInterval(my.signinTimer);
      clearInterval(my.msgTimer);
      clearInterval(my.chatTimer);
      clearInterval(my.watchTimer);
      clearInterval(my.heartbeatTimer);
      if (!S.closingOut) my.closeOut().finally(() => my.sync.stop()); // closing just this window
      else my.sync.stop();
      // Web pages inside the window aren't closed automatically; close them so nothing keeps running
      for (const ws of my.workspaces.values()) for (const t of ws.tabs) { t.vinToken = -1; try { t.view?.webContents.close(); } catch {} }
      try { my.xtimeView.webContents.close(); } catch {}
      try { my.xtimeLite?.webContents.close(); } catch {}
      try { my.chrome.webContents.close(); } catch {}
      windows.delete(my.profile.id);
      if (!S.quitting) { saveProfiles(); pushAll(); } // other windows' profile lists update
      my.win = null;
    });
    my.layout();
    my.startPolling();
  }
};
