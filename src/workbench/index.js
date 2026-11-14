// One window = one profile (one person): its own Xtime / PTS login, settings, open vehicles and hidden ROs.
// Its code is split into parts by what they do; each part adds what the others use to the window object (my), so
// "my.settings", "my.pushState()" and so on are the same thing in every part. The parts, in the order they're set up:
//   settings          this window's settings and files
//   tracking          hours, flags and the shift clock (shared by every window), and sync between computers
//   window-state      the window's state (what's showing, open ROs, Xtime status…), hot reloads, the title
//   logins            saved logins for this window (encrypted with the Windows account)
//   workspaces-store  saving and restoring open vehicles, and open ROs synced between computers
//   feedback          bug reports and feature ideas
//   ui-state          everything the app's screen shows, sent to it (pushState)
//   layout            where each page goes in the window, the find bar (Ctrl+F), printing
//   web-views         Ford / Xtime / ProDemand pages as tabs, Ford's PDFs
//   manual-pages      the app's own pages (Workshop Manual, PC/ED, OASIS, TSBs, Wiring, Labor Times, Diagnostics,
//                     Xtime View) and everything they ask for (manualCall)
//   notifications     the 🔔 list and Windows notifications (assigned, approved / declined, parts, messages)
//   xtime-signin      signing in to Xtime from the app's own form
//   xtime             reading Xtime: the background page, your ROs (polling), RO lookups
//   tabs              vehicles (workspaces) and their tabs, sleeping, loading ahead, opening a vehicle
//   chat              the Dealership chat panel
//   handlers          what the app's screen asks for (buttons, settings, tabs…)
//   window            opening the window itself, and closing it
'use strict';

// in this order: each part adds what the others use to the window object (my)
const PARTS = [
  require('./settings'),
  require('./tracking'),
  require('./window-state'),
  require('./logins'),
  require('./workspaces-store'),
  require('./feedback'),
  require('./ui-state'),
  require('./layout'),
  require('./web-views'),
  require('./manual-pages'),
  require('./notifications'),
  require('./xtime-signin'),
  require('./xtime'),
  require('./tabs'),
  require('./chat'),
  require('./handlers'),
  require('./window'),
];

module.exports = function createWorkbench(profile) {
  const my = { profile };
  for (const install of PARTS) install(my);
  return {
    open: my.open,
    closeOut: my.closeOut,
    partition: my.PARTITION,
    getLogin: my.getLogin, saveLogin: my.saveLogin, rememberUsername: my.rememberUsername,
    pushState: my.pushState,
    handlers: my.handlers,
    displayName: my.displayName,
    keepROs: () => [...my.workspaces.keys()],
    owns: (sender) => my.chrome && sender === my.chrome.webContents,
    manualCall: my.manualCall, manualSave: my.manualSave,
    // Ctrl+B pressed inside one of this window's pages: show or hide the Workbench sidebar
    findKey: my.findKey,
    hotReload: my.hotReload, noteUpdated: my.noteUpdated,
    findClose: (wc) => { if (!my.findWc || my.findWc !== wc) return false; my.closeFind(); return true; },
    sidebarKey: (wc) => {
      if (!my.allContentViews().some((v) => v.webContents === wc) || my.mode !== 'manual') return false;
      my.handlers['toggle-sidebar']();
      return true;
    },
    focus: () => { if (my.win && !my.win.isDestroyed()) { if (my.win.isMinimized()) my.win.restore(); my.win.focus(); } },
  };
};
