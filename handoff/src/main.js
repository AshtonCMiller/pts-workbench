// PTS Workbench: the move from the Electron app to the new one.
//
// Each release of the new app also carries this, shaped as the Electron app's "app files" update (app-bundle.asar),
// so computers still on the Electron app pick it up the way they get any update (betas only with Beta updates on).
// Started by the Electron app's boot.js in place of its own code, it:
//   1. starts the new app straight away when it's installed already (an old shortcut or taskbar pin still opening
//      the Electron app ends up in the new one);
//   2. otherwise downloads the new app's installer from this release (checked against its SHA-256), runs it silently
//      (per-user, no admin: it puts the app in %LOCALAPPDATA%\Programs\PTS Workbench, replaces the desktop and
//      Start menu shortcuts, and starts the new app), points taskbar pins at the new app, and closes;
//   3. if any of that fails, sets these files aside and restarts into the Electron app as it was, whose updater
//      then offers this release's installer instead (and the move is tried again that way).
// Settings, hours and the rest stay where they are (%APPDATA%\PTS Workbench): the new app reads them there.
'use strict';
const { app, BrowserWindow, net, shell } = require('electron');
const fs = require('fs');
const path = require('path');
const crypto = require('crypto');
const { spawn } = require('child_process');

const HANDOFF = require('../handoff.json'); // { version, tag, repo, setup: { name, size, sha256 } }
const boot = global.ptsBoot || null;
const INSTALL_DIR = path.join(process.env.LOCALAPPDATA || path.join(app.getPath('home'), 'AppData', 'Local'), 'Programs', 'PTS Workbench');
const LAUNCHER = path.join(INSTALL_DIR, 'pts-workbench.exe');
const LOG = path.join(app.getPath('userData'), 'handoff.log');

function log(line) {
  try { fs.appendFileSync(LOG, `${new Date().toISOString()} v${HANDOFF.version} ${line}\n`); } catch { /* best effort */ }
}

// Starts the new app on its own (it outlives this one)
function startNewApp() {
  const child = spawn(LAUNCHER, process.argv.slice(1).filter((a) => !a.startsWith('--')), { cwd: INSTALL_DIR, detached: true, stdio: 'ignore' });
  child.unref();
}

// Taskbar pins of the Electron app open the new one (desktop and Start menu shortcuts are the installer's)
function movePins() {
  const dir = path.join(process.env.APPDATA || '', 'Microsoft', 'Internet Explorer', 'Quick Launch', 'User Pinned', 'TaskBar');
  let moved = 0;
  try {
    for (const f of fs.readdirSync(dir)) {
      if (!f.toLowerCase().endsWith('.lnk')) continue;
      const p = path.join(dir, f);
      try {
        const link = shell.readShortcutLink(p);
        if (path.resolve(link.target || '').toLowerCase() !== path.resolve(process.execPath).toLowerCase()) continue;
        if (shell.writeShortcutLink(p, 'update', { target: LAUNCHER, cwd: INSTALL_DIR, icon: LAUNCHER, iconIndex: 0, args: '' })) moved++;
      } catch { /* not a shortcut it can read */ }
    }
  } catch { /* no pins */ }
  return moved;
}

// A small window saying what's happening (the move takes a minute)
let win = null;
function showWindow() {
  win = new BrowserWindow({
    width: 420, height: 150, frame: false, resizable: false, center: true, show: false, alwaysOnTop: true,
    backgroundColor: '#ffffff', skipTaskbar: false, title: 'PTS Workbench',
    webPreferences: { contextIsolation: true, sandbox: true, javascript: true },
  });
  const html = `<!doctype html><meta charset="utf-8"><style>
    body{margin:0;font:14px "Segoe UI",system-ui,sans-serif;color:#1d2126;background:#fff;display:flex;flex-direction:column;
      justify-content:center;height:100vh;padding:0 26px;box-sizing:border-box;border:1px solid #dde1e6;-webkit-app-region:drag}
    h1{font-size:16px;margin:0 0 6px}#t{color:#5d6670}
    .bar{height:4px;background:#e3ecf8;border-radius:2px;margin-top:14px;overflow:hidden}
    .bar i{display:block;height:100%;width:30%;background:#1a5fb4;border-radius:2px;animation:m 1.2s ease-in-out infinite}
    @keyframes m{0%{transform:translateX(-100%)}100%{transform:translateX(340%)}}</style>
    <h1>Moving to the new PTS Workbench</h1><div id="t">Downloading it…</div><div class="bar"><i></i></div>`;
  win.loadURL(`data:text/html;charset=utf-8,${encodeURIComponent(html)}`);
  win.once('ready-to-show', () => win && win.show());
}
function say(text) {
  if (win && !win.isDestroyed()) win.webContents.executeJavaScript(`document.getElementById('t').textContent=${JSON.stringify(text)}`).catch(() => {});
}

async function download() {
  const s = HANDOFF.setup;
  const url = `https://github.com/${HANDOFF.repo}/releases/download/${encodeURIComponent(HANDOFF.tag)}/${encodeURIComponent(s.name)}`;
  const res = await net.fetch(url, { cache: 'no-store' });
  if (!res.ok) throw new Error(`the installer wasn't there (HTTP ${res.status})`);
  const buf = Buffer.from(await res.arrayBuffer());
  if (buf.length !== s.size) throw new Error('the installer didn\'t download whole');
  if (crypto.createHash('sha256').update(buf).digest('hex') !== s.sha256) throw new Error("the installer doesn't match its checksum");
  const file = path.join(app.getPath('temp'), s.name);
  fs.writeFileSync(file, buf);
  return file;
}

function install(file) {
  return new Promise((resolve, reject) => {
    const child = spawn(file, ['/S'], { stdio: 'ignore', windowsHide: true });
    child.on('error', reject);
    child.on('exit', (code) => (code === 0 ? resolve() : reject(new Error(`the installer stopped (code ${code})`))));
  });
}

// It didn't work: these files are set aside, and the Electron app starts as it was
function giveUp(e) {
  log(`move failed: ${e && e.message ? e.message : e}`);
  try {
    const c = boot && boot.readCur();
    if (c) { c.bad = true; c.error = `move to the new app: ${String(e && e.message ? e.message : e).slice(0, 200)}`; boot.writeCur(c); }
  } catch { /* nothing to set aside */ }
  app.relaunch();
  app.exit(0);
}

if (!app.requestSingleInstanceLock()) {
  app.quit();
} else if (fs.existsSync(LAUNCHER)) {
  // moved already
  if (boot) boot.ok();
  app.whenReady().then(() => {
    try { movePins(); } catch { /* best effort */ }
    startNewApp();
    app.quit();
  });
} else {
  app.whenReady().then(async () => {
    if (boot) boot.ok(); // (it started: a failed move isn't counted as these files not starting)
    showWindow();
    log('moving to the new app');
    try {
      const file = await download();
      say('Installing it…');
      await install(file);
      try { fs.rmSync(file, { force: true }); } catch { /* temp */ }
      if (!fs.existsSync(LAUNCHER)) throw new Error('the installer finished, but the new app isn\'t there');
      const pins = movePins();
      log(`moved: installed in ${INSTALL_DIR}${pins ? `, ${pins} taskbar pin(s) moved` : ''}`);
      say('Done: starting it…');
      // (the installer starts the new app itself)
      setTimeout(() => app.quit(), 1500);
    } catch (e) {
      giveUp(e);
    }
  });
}
app.on('window-all-closed', () => { /* closes itself when done */ });
