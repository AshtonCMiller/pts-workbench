// PTS Workbench — main process: starting the app, its windows, and the messages between them and their pages.
// Each window runs one profile: its own Xtime/PTS login, settings, open vehicles and hidden ROs.
// The shift clock and the flag sheet are shared by every window, so all flags land in one set of hours.
// Every page is loaded from Ford/Cox as normal, so access is recorded under that window's own login.
//
// The rest of the main process:
//   app/          app-wide: config.js (settings and fixed values), files.js, state.js (shared state, "S"),
//                 updates.js (updates and What's new), web.js (printing, sessions, staying signed in),
//                 keep-awake.js, snapshot.js ("+ Manual")
//   workbench/    one window (index.js lists its parts: settings, Xtime, tabs, the app's pages, notifications…)
//   manual.js     everything fetched from Ford (Workshop Manual, PC/ED, OASIS, TSBs, Wiring, SLTS, Diagnostics)
//   tracker.js    hours, flags and the shift clock;  sync.js  sync between computers;  boot.js  picks the app files
//
// Started directly (package.json "main" pointing here rather than at boot.js): let boot.js choose the app files
// first. If it starts downloaded ones, they're the app and this copy stops here.
if (!global.ptsBoot) {
  require('./boot.js');
  if (global.ptsBoot?.quick) return; // eslint-disable-line no-unreachable
}
const { app, ipcMain, session, shell, clipboard, protocol, Menu } = require('electron');
const S = require('./app/state'); // shared app state (see app/state.js)
const { pushAll, saveProfiles, windows } = S;
const { readJsonAt, userFile } = require('./app/files');
const { initUpdater, initWhatsNew } = require('./app/updates');
const { appUiContents, isPrintKey, printPage } = require('./app/web');
const createWorkbench = require('./workbench');

// Pictures in the app's own manual viewer come through ptsasset://, fetched with the window's PTS login
protocol.registerSchemesAsPrivileged([{ scheme: 'ptsasset', privileges: { standard: true, secure: true, supportFetchAPI: true, stream: true } }]);

// Before the app closes: every window records when it closed and sends unsynced changes (a few seconds at most)
S.closingOut = false;
async function closeOutAll() {
  S.closingOut = true;
  await Promise.all([...windows.values()].map((wb) => wb.closeOut().catch(() => {})));
  // write sign-ins to disk now (Chromium otherwise saves them every half minute or so)
  await Promise.all([...windows.values()].map((wb) => session.fromPartition(wb.partition).cookies.flushStore().catch(() => {})));
}
S.closeOutAll = closeOutAll;

// =====================================================================================
// Windows
// =====================================================================================
function openProfileWindow(id) {
  if (windows.has(id)) { windows.get(id).focus(); return; }
  const profile = S.profiles.find((p) => p.id === id);
  if (!profile) return;
  const wb = createWorkbench(profile);
  windows.set(id, wb);
  wb.open();
  saveProfiles();
  pushAll();
}
S.openProfileWindow = openProfileWindow;

// Only the app's own UI may drive it; each message goes to the window it came from.
ipcMain.on('action', (event, name, payload) => {
  for (const wb of windows.values()) {
    if (!wb.owns(event.sender)) continue;
    const fn = wb.handlers[name];
    if (fn) { try { fn(payload); } catch (e) { console.error(name, e); } }
    return;
  }
});

// The app's own manual viewer
ipcMain.handle('manual', async (event, name, payload) => {
  for (const wb of windows.values()) {
    const res = await wb.manualCall(event.sender, name, payload);
    if (res !== undefined) return res;
  }
  return { error: true, message: 'This manual tab is no longer open' };
});
ipcMain.on('manual-save', (event, state) => {
  for (const wb of windows.values()) if (wb.manualSave(event.sender, state)) return;
});

// Login helper messages from inside Xtime/PTS/ProDemand pages.
function loginContext(event) {
  let host = null;
  try {
    const u = new URL(event.senderFrame?.url || '');
    if (u.protocol === 'https:') host = u.host;
  } catch { /* not a web page */ }
  if (!host) return null;
  for (const wb of windows.values()) {
    if (event.sender.session === session.fromPartition(wb.partition)) return { wb, host };
  }
  return null;
}
ipcMain.handle('cred-get', (event, opts) => {
  const ctx = loginContext(event);
  return ctx ? ctx.wb.getLogin(ctx.host, !!opts?.identifierOnly) : null;
});
ipcMain.on('cred-save', (event, data) => {
  const ctx = loginContext(event);
  if (ctx && typeof data?.password === 'string') ctx.wb.saveLogin(ctx.host, String(data.username || '').slice(0, 200), data.password.slice(0, 500));
});
ipcMain.on('cred-username', (event, data) => {
  const ctx = loginContext(event);
  if (ctx) ctx.wb.rememberUsername(ctx.host, data?.username);
});

if (!app.requestSingleInstanceLock()) {
  app.quit();
} else {
  // Starting the app again just brings an open window forward; use Settings → Open another window.
  app.on('second-instance', () => { windows.values().next().value?.focus(); });

  app.whenReady().then(() => {
    try { app.setAppUserModelId('local.ptsworkbench'); } catch {} // Windows shows notifications under this app
    const saved = readJsonAt(userFile('profiles.json'), null);
    S.profiles = Array.isArray(saved?.profiles) && saved.profiles.length ? saved.profiles : [{ id: 'main', name: 'Main' }];
    if (!S.profiles.some((p) => p.id === 'main')) S.profiles.unshift({ id: 'main', name: 'Main' });

    initWhatsNew();
    initUpdater();

    // Reopen the windows that were open last time (at least the main one)
    const reopen = (saved?.open || []).filter((id) => S.profiles.some((p) => p.id === id));
    for (const id of reopen.length ? reopen : ['main']) openProfileWindow(id);
  });

  app.on('before-quit', (event) => {
    S.quitting = true;
    saveProfiles(); // remember which windows were open
    // Closing the app doesn't clock anyone out; each window records when it closed (for "forgot to
    // clock out") and sends unsynced changes first, a few seconds at most.
    if (S.closingOut) return;
    event.preventDefault();
    closeOutAll().finally(() => app.quit());
  });

  app.on('web-contents-created', (_e, wc) => {
    // Right-click: Copy / Paste where it makes sense, and Print (the page that's showing; the app's own bar prints that page too)
    wc.on('context-menu', (_ev, p) => {
      const items = [];
      if (p.isEditable) items.push({ role: 'cut', enabled: p.editFlags.canCut }, { role: 'copy', enabled: p.editFlags.canCopy }, { role: 'paste', enabled: p.editFlags.canPaste }, { type: 'separator' });
      else if (p.selectionText && p.selectionText.trim()) items.push({ role: 'copy' }, { type: 'separator' });
      if (p.linkURL && /^https?:/i.test(p.linkURL)) items.push({ label: 'Copy link address', click: () => clipboard.writeText(p.linkURL) }, { type: 'separator' });
      items.push({ label: 'Print…', accelerator: 'Ctrl+P', click: () => {
        if (appUiContents.has(wc)) { for (const wb of windows.values()) if (wb.owns?.(wc)) { wb.handlers['print-page']?.(); return; } }
        printPage(wc);
      } });
      try { Menu.buildFromTemplate(items).popup(); } catch { /* no menu */ }
    });
    // Ctrl+P prints the page (the app's own screen handles it itself, to print the page that's showing)
    wc.on('before-input-event', (ev, input) => {
      if (appUiContents.has(wc)) return;
      // Ctrl+F: the app's find bar (Esc closes it)
      if (input.type === 'keyDown' && input.control && !input.alt && !input.shift && input.key.toLowerCase() === 'f') {
        for (const wb of windows.values()) if (wb.findKey?.(wc)) { ev.preventDefault(); return; }
        return;
      }
      if (input.type === 'keyDown' && input.key === 'Escape') {
        for (const wb of windows.values()) if (wb.findClose?.(wc)) { ev.preventDefault(); return; }
      }
      if (input.type === 'keyDown' && input.control && !input.alt && !input.shift && input.key.toLowerCase() === 'b') {
        for (const wb of windows.values()) if (wb.sidebarKey?.(wc)) { ev.preventDefault(); return; }
        return;
      }
      if (!isPrintKey(input)) return;
      ev.preventDefault();
      printPage(wc);
    });
    wc.on('will-navigate', (ev, url) => { if (!/^https?:|^file:/i.test(url)) { ev.preventDefault(); shell.openExternal(url).catch(() => {}); } });
  });

  app.on('window-all-closed', () => app.quit());
}
