// The window's state: the window and its views, what's showing (Xtime or the Workbench, the drawer, chat), open ROs and
// vehicles, Xtime's status, search and lookups. Hot updates (reloading the app's pages), short notices, the title.
'use strict';
const path = require('path');
const S = require('../app/state'); // shared app state (see app/state.js)
const { DEFAULT_SETTINGS } = require('../app/config');

module.exports = function install(my) {
  Object.assign(my, { reloadAppPage, hotReload, notice }); // used by the window's other parts

  my.heartbeatTimer = null;
  if (my.settings.prodemandUrl === 'https://www1.prodemand.com/') my.settings.prodemandUrl = DEFAULT_SETTINGS.prodemandUrl; // old wrong default
  const sidebarWidth = () => Math.min(700, Math.max(260, Number(my.settings.sidebarWidth) || 380));
  my.sidebarWidth = sidebarWidth;

  my.win = undefined;
  my.chrome = undefined;
  my.xtimeView = undefined;
  my.mode = 'xtime';           // 'xtime' | 'manual' (the Workbench)
  my.drawerOpen = true;
  my.chatOpen = false;          // the app's Dealership chat panel
  const loggedChatFields = new Set();
  my.loggedChatFields = loggedChatFields;
  my.loggedXtimeFields = false;
  my.loggedLineFields = false;
  my.overlay = false;          // a dialog is open in the app UI; hide web views (a still picture of the page stays behind it)
  my.overlaySeq = 0;
  // ---------- hot updates: the app's own pages reloaded from new app files, without restarting ----------
  const staleViews = new Set(); // app pages still showing old files (the one you're looking at waits until you leave it)
  my.staleViews = staleViews;
  function reloadAppPage(v) {
    if (v && !v.webContents.isDestroyed() && v.ptsPage) v.webContents.loadFile(path.join(S.PAGES, 'manual', v.ptsPage));
  }
  function hotReload() {
    my.overlay = false; // a dialog open in the top bar closes with the reload
    if (my.chrome && !my.chrome.webContents.isDestroyed()) my.chrome.webContents.loadFile(path.join(S.PAGES, 'ui', 'index.html'));
    for (const { tab } of my.manualViews.values()) {
      const v = tab.view;
      if (!v || v.webContents.isDestroyed()) continue;
      if (v === my.visibleView) staleViews.add(v); else reloadAppPage(v);
    }
    my.layout();
  }
  my.myROs = [];
  my.xtimeStatus = 'Log in to Xtime to load your ROs';
  my.lastPoll = null;
  my.pollTimer = null;
  my.watchTimer = null;
  my.nextTabId = 1;
  const workspaces = new Map(); // ro -> { ro, vin, ymm, tabs: [], activeTab }
  my.workspaces = workspaces;
  my.activeRO = null;
  const hiddenROs = new Set();  // hidden from My ROs; unhidden only from that RO's page in Xtime
  my.hiddenROs = hiddenROs;
  my.xtimeViewingRO = null;
  my.xtimeName = '';           // technician name read from Xtime, for the title bar
  my.searchQuery = '';
  my.periodOffset = 0;
  my.lookup = null;
  my.addLookup = null;         // "Add vehicle" with no VIN: { ro, status: 'loading'|'error', message }

  // Xtime connection health (its session expires after about 2 hours)
  my.xtimeFailures = 0;
  my.xtimeLoadedAt = Date.now();
  my.lastRecovery = 0;

  function notice(text, extra = {}) {
    if (my.chrome && !my.chrome.webContents.isDestroyed()) my.chrome.webContents.send('notice', { text, ...extra, at: Date.now() });
  }

  const displayName = () => my.xtimeName || my.profile.name || 'Main';
  my.displayName = displayName;
  const updateTitle = () => { if (my.win && !my.win.isDestroyed()) my.win.setTitle(`PTS Workbench - ${displayName()}`); };
  my.updateTitle = updateTitle;
};
