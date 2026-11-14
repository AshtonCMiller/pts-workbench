// Where each page goes in the window (top bar, tabs, the Service Manual's tab row, the sidebar, chat), the find bar
// (Ctrl+F) for whatever page is showing, and printing the page that's showing.
'use strict';
const { CHAT_W, DRAWER, SUB_STRIP, TAB_STRIP, TOP_BAR } = require('../app/config');
const { printPage } = require('../app/web');

module.exports = function install(my) {
  Object.assign(my, { allContentViews, layout, visibleKind, openFind, closeFind, findKey, printVisible }); // used by the window's other parts

  // ---------- layout ----------
  function allContentViews() {
    const views = [my.xtimeView];
    for (const ws of my.workspaces.values()) for (const t of ws.tabs) if (t.view) views.push(t.view);
    return views;
  }

  function layout() {
    if (!my.win || my.win.isDestroyed()) return;
    my.touchActive();
    // The vehicle on screen: fetch its tabs' data ahead if that hasn't happened in the last 15 minutes
    // (covers switching to Workbench, the app starting on a vehicle, and waking one up)
    const shown = my.mode === 'manual' ? my.workspaces.get(my.activeRO) : null;
    if (shown && (!shown.aheadAt || Date.now() - shown.aheadAt > 15 * 60 * 1000)) my.prefetchVehicle(shown);
    const { width, height } = my.win.getContentBounds();
    my.chrome.setBounds({ x: 0, y: 0, width, height });
    let visible = null;
    if (!my.overlay) {
      if (my.mode === 'xtime' && my.settings.xtimeDashboard) {
        visible = null; // the app's own dashboard is drawn in the app's screen
      } else if (my.mode === 'xtime') {
        my.xtimeView.setBounds({ x: 0, y: TOP_BAR, width: Math.max(0, width - (my.chatOpen ? CHAT_W : my.drawerOpen ? DRAWER : 0)), height: Math.max(0, height - TOP_BAR) });
        visible = my.xtimeView;
      } else if (my.activeRO && my.workspaces.has(my.activeRO)) {
        const ws = my.workspaces.get(my.activeRO);
        const tab = my.currentTab(ws);
        if (tab && tab.kind !== 'service') {
          my.ensureView(ws, tab);
          const side = my.settings.sidebarHidden ? 0 : my.sidebarWidth(); // the Workbench sidebar can be tucked away
          const top = TOP_BAR + TAB_STRIP + (my.inService(tab.kind) ? SUB_STRIP : 0);
          tab.view.setBounds({ x: side, y: top, width: Math.max(0, width - side - (my.chatOpen ? CHAT_W : 0)), height: Math.max(0, height - top) });
          visible = tab.view;
        }
      }
    }
    for (const v of allContentViews()) v.setVisible(v === visible);
    my.visibleView = visible;
    if (my.findWc && visible?.webContents !== my.findWc) closeFind(); // another page now: the find bar closes
    // app pages left on the old files by a hot update: reloaded once you've moved off them
    for (const v of my.staleViews) if (v !== visible) { my.staleViews.delete(v); my.reloadAppPage(v); }
  }
  my.visibleView = null;

  // ---------- find in page (Ctrl+F): the app's find bar, for whatever page is showing ----------
  // (Wiring Diagrams has its own, which finds labels in the drawing)
  my.findWc = null;
  const sendFind = (data) => { if (my.chrome && !my.chrome.webContents.isDestroyed()) my.chrome.webContents.send('find', data); };
  my.sendFind = sendFind;
  function visibleKind() {
    if (my.mode !== 'manual' || !my.activeRO || !my.workspaces.has(my.activeRO)) return '';
    return my.currentTab(my.workspaces.get(my.activeRO))?.kind || '';
  }
  function openFind() {
    const wc = my.visibleView && !my.visibleView.webContents.isDestroyed() ? my.visibleView.webContents : null;
    if (!wc) return false;
    if (my.findWc && my.findWc !== wc && !my.findWc.isDestroyed()) my.findWc.stopFindInPage('clearSelection');
    my.findWc = wc;
    if (!wc.findHooked) {
      wc.findHooked = true;
      wc.on('found-in-page', (_e, r) => { if (my.findWc === wc) sendFind({ result: true, active: r.activeMatchOrdinal || 0, matches: r.matches || 0 }); });
    }
    my.chrome.webContents.focus();
    sendFind({ open: true });
    return true;
  }
  function closeFind({ quiet = false } = {}) {
    const wc = my.findWc;
    my.findWc = null;
    if (wc && !wc.isDestroyed()) wc.stopFindInPage('keepSelection');
    if (!quiet) sendFind({ close: true });
    return wc && !wc.isDestroyed() ? wc : null;
  }
  // Ctrl+F in a page: true when the find bar opened (Wiring Diagrams keeps the key for its own find)
  function findKey(wc) {
    if (!my.visibleView || my.visibleView.webContents !== wc) return false;
    if (visibleKind() === 'wiring') return false;
    return openFind();
  }
  // Print whatever page is showing (Xtime, or the open tab of a vehicle)
  function printVisible() {
    if (my.visibleView && !my.visibleView.webContents.isDestroyed()) printPage(my.visibleView.webContents);
    else my.notice('Open a page first, then print it');
  }
};
