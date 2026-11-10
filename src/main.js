// PTS Workbench — main process
// Each window runs one profile: its own Xtime/PTS login, settings, open vehicles and hidden ROs.
// The shift clock and the flag sheet are shared by every window, so all flags land in one set of hours.
// Every page is loaded from Ford/Cox as normal, so access is recorded under that window's own login.
const { app, BaseWindow, BrowserWindow, WebContentsView, ipcMain, session, shell, safeStorage, clipboard, net, protocol, Notification } = require('electron');
const path = require('path');
const fs = require('fs');

const XTIME_EXTRACT = fs.readFileSync(path.join(__dirname, 'inject', 'xtime-extract.js'), 'utf8').trim();
const XTIME_RO = fs.readFileSync(path.join(__dirname, 'inject', 'xtime-ro.js'), 'utf8').trim();
const XTIME_CHAT = fs.readFileSync(path.join(__dirname, 'inject', 'xtime-chat.js'), 'utf8').trim();
const COX_SIGNIN = fs.readFileSync(path.join(__dirname, 'inject', 'cox-signin.js'), 'utf8').trim();
const COX_SIGNIN_HOST = /(^|\.)signin\.coxautoinc\.com$/i;
const XTIME_DETAILS = fs.readFileSync(path.join(__dirname, 'inject', 'xtime-details.js'), 'utf8').trim();
const XTIME_VEHICLE = fs.readFileSync(path.join(__dirname, 'inject', 'xtime-vehicle.js'), 'utf8').trim();
const XTIME_EMPLOYEE = fs.readFileSync(path.join(__dirname, 'inject', 'xtime-employee.js'), 'utf8').trim();
const { Tracker } = require('./tracker');
const { ManualService, PTS: PTS_ORIGIN } = require('./manual');

// Pictures in the app's own manual viewer come through ptsasset://, fetched with the window's PTS login
protocol.registerSchemesAsPrivileged([{ scheme: 'ptsasset', privileges: { standard: true, secure: true, supportFetchAPI: true, stream: true } }]);
const { SyncEngine } = require('./sync');
// The sync server's address is set in package.json ("ptsWorkbench": { "syncUrl": … })
const SYNC_URL = (() => { try { return require('../package.json').ptsWorkbench?.syncUrl || ''; } catch { return ''; } })();

const TOP_BAR = 48;
const TAB_STRIP = 38;
const DRAWER = 360;
const CHAT_W = 380;   // the read-only Dealership chat panel on the right

const DEFAULT_SETTINGS = {
  employeeNumber: '',  // your employee number; the Xtime technician ID is looked up from it
  storeId: '3590',
  xtimeEmployeesPath: '/JS/Employees?storeID={STORE}',
  techId: '',          // Xtime's internal technician ID (filled in from the employee number)
  techIdFor: '',       // the employee number techId was looked up for
  setupDone: false,    // first-launch questions answered
  xtimeUrl: 'https://www.inspect.app.coxautoinc.com/',
  xtimeDataHost: 'www.inspect.app.coxautoinc.com',
  xtimeApiPath: '/api/RepairOrderDashboard/?showCustomerROs=true&showInternalROs=true&showExpressROs=&showWaiterROs=',
  ptsUrl: 'https://www.fordtechservice.dealerconnection.com/',
  // Ford's own deep link: opens the workshop manual with the vehicle already loaded
  ptsTransportUrl: 'https://www.fordtechservice.dealerconnection.com/transport?link={LINK}&params={VIN}',
  oasisUrl: 'https://www.fordtechservice.dealerconnection.com/oasis/Index?vin={VIN}',
  xtimeDetailsPath: '/RepairOrder/Details/{RO}',
  xtimeRoUrl: 'https://www.inspect.app.coxautoinc.com/RepairOrder/Details/{RO}',
  prodemandUrl: 'https://prodemand.com/',
  pollSeconds: 60,
  xtimeRefreshMinutes: 90, // reload Xtime in the background before its ~2 h session runs out
  autoLogin: true,         // press "Sign in" with a saved password when a site asks you to log in
  syncEnabled: false,      // keep flags, hours and the clock in sync across computers (main window's setting)
  sidebarWidth: 380,
  sidebarHidden: false,
  xtimeDashboard: false,   // show the app's own Xtime dashboard instead of Xtime's page
  notifications: true,     // Windows pop-ups: RO assigned, line approved, new Xtime messages
};
const BOOLEAN_SETTINGS = ['autoLogin', 'setupDone', 'syncEnabled', 'xtimeDashboard', 'notifications', 'sidebarHidden'];
// Not shown in Settings (too technical): always the built-in values, updated with the app
const FIXED_SETTINGS = ['ptsUrl', 'ptsTransportUrl', 'oasisUrl', 'xtimeApiPath', 'xtimeRoUrl', 'xtimeDetailsPath', 'xtimeEmployeesPath'];
// Ford transport link codes: 704 = OASIS landing page (workshop manual), 702 = Over-the-Air update
// dashboard, 701 = Connected Vehicle Data
const FORD_LINKS = { pts: '704', ota: '702', cvd: '701' };
const NUMBER_SETTINGS = { pollSeconds: [15, 3600], sidebarWidth: [260, 700], xtimeRefreshMinutes: [20, 600] };

// Ford, Lincoln and Mercury get PTS; anything else gets ProDemand.
// Make from Xtime first; if there isn't one, the VIN's manufacturer code (first 3 characters).
const FORD_WMI = /^(1F[A-DMT]|2F[ABMT]|3F[A-EMT]|1L[N]|2L[MN]|3L[N]|5L[DMT]|[123]ME|4M2|1ZV|NM0|WF0|MAJ|6FP|MN[AB]|LVS|PE1)/;
function isFordVehicle(vin, ymm) {
  const text = String(ymm || '').toLowerCase();
  if (/\b(ford|lincoln|mercury)\b/.test(text)) return true;
  return FORD_WMI.test(String(vin || '').toUpperCase());
}

// Best effort: type the VIN into a box labelled "VIN" on the ProDemand page (any same-site frame).
const PRODEMAND_FILL = `((vin) => {
  const docs = [document, ...[...document.querySelectorAll('iframe,frame')].map((f) => { try { return f.contentDocument; } catch { return null; } }).filter(Boolean)];
  for (const d of docs) {
    const i = [...d.querySelectorAll('input')].find((x) => x.offsetParent !== null && /(^|[^a-z])vin(number|[^a-z]|$)/i.test([x.id, x.name, x.placeholder, x.getAttribute('aria-label')].join(' ')));
    if (!i) continue;
    if (i.value && i.value.toUpperCase() === vin) return 'filled';
    const w = d.defaultView;
    Object.getOwnPropertyDescriptor(w.HTMLInputElement.prototype, 'value').set.call(i, vin);
    i.dispatchEvent(new w.Event('input', { bubbles: true }));
    i.dispatchEvent(new w.Event('change', { bubbles: true }));
    return 'filled';
  }
  return 'none';
})`;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// ---------- files ----------
// The first profile ("main") keeps the original file names, so existing setups carry straight over.
const userFile = (...p) => path.join(app.getPath('userData'), ...p);
const readJsonAt = (file, fallback) => { try { return JSON.parse(fs.readFileSync(file, 'utf8')); } catch { return fallback; } };
const writeJsonAt = (file, value) => {
  try { fs.mkdirSync(path.dirname(file), { recursive: true }); fs.writeFileSync(file, JSON.stringify(value, null, 2)); }
  catch (e) { console.error(e); }
};
const profileFile = (id, name) => (id === 'main' ? userFile(name) : userFile('profiles', id, name));
const partitionFor = (id) => (id === 'main' ? 'persist:workbench' : `persist:profile-${id}`);

// ---------- shared state ----------
let profiles = [];            // [{ id, name }]
const windows = new Map();    // profile id -> workbench (one window per profile)
let quitting = false;

const saveProfiles = () => writeJsonAt(userFile('profiles.json'), { profiles, open: [...windows.keys()] });
const pushAll = () => { for (const wb of windows.values()) wb.pushState(); };

// =====================================================================================
// Updates: the installed app checks GitHub Releases, downloads a newer version in the
// background, and installs it when you press "Restart to update" (or next time you close it).
// =====================================================================================
let updater = null;
const update = { state: 'idle', version: '', percent: 0, message: '', checkedAt: null };

// ---------- beta versions ----------
// A beta build (version like 0.4.1-beta.12, built from any branch other than main) has the new Xtime
// features: the app's own dashboard, chat, and running without Xtime's page. Regular builds behave as before.
// "Beta updates" (⚙ Settings → Version & updates) lets one computer install beta builds; turning it off
// goes back to the newest regular version on the next check.
const IS_BETA = !app.isPackaged || /-/.test(app.getVersion());
// No choice saved yet: a beta version keeps getting betas (it was installed on purpose, e.g. by running a beta
// installer from GitHub); a regular version keeps getting regular versions
const updatePrefs = { beta: IS_BETA, ...(readJsonAt(userFile('updates.json'), {}) || {}) };
function applyUpdateChannel() {
  if (!updater) return;
  if (updatePrefs.beta) {
    updater.allowPrerelease = true;
    updater.channel = 'beta';          // beta.yml: every release (beta or regular) has one
  } else if (IS_BETA) {
    updater.allowPrerelease = false;
    updater.channel = 'latest';        // leaving beta: back to the newest regular version (may be "older")
  } else {
    updater.allowPrerelease = false;
    updater.allowDowngrade = false;
  }
}
function setBetaUpdates(on) {
  updatePrefs.beta = !!on;
  writeJsonAt(userFile('updates.json'), updatePrefs);
  applyUpdateChannel();
  pushAll();
  if (updater) updater.checkForUpdates().catch(() => {});
}
const setUpdate = (patch) => { Object.assign(update, patch); pushAll(); };

function initUpdater() {
  if (!app.isPackaged) { Object.assign(update, { state: 'off', message: 'Running from source, so updates are off' }); return; }
  if (process.env.PORTABLE_EXECUTABLE_DIR) {
    Object.assign(update, { state: 'off', message: "The portable version can't update itself. Install with PTS-Workbench-Setup to get updates." });
    return;
  }
  try { updater = require('electron-updater').autoUpdater; } catch {
    Object.assign(update, { state: 'error', message: 'The updater is missing from this build' });
    return;
  }
  // Keep a short log of what the updater sees (⚙ Settings → Version & updates → Open update log)
  const logFile = path.join(app.getPath('userData'), 'updater.log');
  const log = (level) => (...args) => {
    try {
      try { if (fs.statSync(logFile).size > 512 * 1024) fs.renameSync(logFile, logFile + '.old'); } catch { /* no log yet */ }
      fs.appendFileSync(logFile, `${new Date().toISOString()} ${level} ${args.map((a) => (typeof a === 'string' ? a : JSON.stringify(a))).join(' ')}\n`);
    } catch { /* logging is best effort */ }
  };
  updater.logger = { info: log('INFO'), warn: log('WARN'), error: log('ERROR'), debug: () => {} };
  update.logFile = logFile;
  updater.autoDownload = true;
  applyUpdateChannel();
  updater.autoInstallOnAppQuit = true; // a downloaded update also installs whenever you close the app
  updater.on('checking-for-update', () => setUpdate({ state: 'checking', message: '' }));
  // info.version is the newest version GitHub offered, so a stuck release is easy to spot
  updater.on('update-not-available', (info) => setUpdate({ state: 'current', latest: info?.version || '', checkedAt: Date.now(), message: '' }));
  updater.on('update-available', (info) => setUpdate({ state: 'downloading', version: info.version, percent: 0, checkedAt: Date.now() }));
  updater.on('download-progress', (p) => {
    const pct = Math.round(p.percent || 0);
    if (pct - update.percent >= 5 || pct === 100) setUpdate({ state: 'downloading', percent: pct });
  });
  updater.on('update-downloaded', (info) => setUpdate({ state: 'ready', version: info.version, percent: 100 }));
  updater.on('error', (err) => {
    const text = String(err?.message || err);
    const message = /404|not found/i.test(text) ? 'No release found on GitHub yet (or the repository is private)'
      : /ENOTFOUND|ETIMEDOUT|ECONNRESET|net::/i.test(text) ? "Couldn't reach GitHub"
      : text.split('\n')[0].slice(0, 160);
    setUpdate({ state: 'error', message, checkedAt: Date.now() });
  });
  const check = () => { updater.checkForUpdates().catch(() => { /* reported through 'error' */ }); };
  setTimeout(check, 15 * 1000);          // shortly after starting
  setInterval(check, 30 * 60 * 1000);    // and every 30 minutes
}

async function installUpdate() {
  if (!updater || update.state !== 'ready') return;
  quitting = true;
  saveProfiles();          // reopen the same windows after the update
  await closeOutAll();     // record closing time + send anything not yet synced (clocks keep running)
  updater.quitAndInstall(true, true); // silent install, then start the app again
}

// Before the app closes: every window records when it closed and sends unsynced changes (a few seconds at most)
let closingOut = false;
async function closeOutAll() {
  closingOut = true;
  await Promise.all([...windows.values()].map((wb) => wb.closeOut().catch(() => {})));
}

// =====================================================================================
// Printing. PTS (and some Xtime screens) show their content inside frames, so this prints the part
// of the page you're working in, not the outer page with the content cut off.
// =====================================================================================
const appUiContents = new WeakSet(); // the app's own screens (not web pages)
const isPrintKey = (input) => input.type === 'keyDown' && (input.control || input.meta) && !input.alt && input.key.toLowerCase() === 'p';
async function printPage(wc) {
  const fallback = () => wc.print({}, () => {});
  try {
    const top = wc.mainFrame;
    const probe = `(() => { const d = document; const fs = d.body && d.body.tagName === 'FRAMESET';
      return { focus: d.hasFocus(), area: innerWidth * innerHeight, frameset: fs,
               text: fs ? 0 : ((d.body && d.body.innerText) || '').trim().length }; })()`;
    const info = [];
    for (const f of top.framesInSubtree) {
      let depth = 0;
      for (let p = f.parent; p; p = p.parent) depth++;
      try { info.push({ f, depth, ...(await f.executeJavaScript(probe)) }); } catch { /* another site's frame */ }
    }
    const topInfo = info.find((i) => i.f === top);
    if (!topInfo) return fallback();
    const big = (i) => i.area >= topInfo.area * 0.3 && !i.frameset && i.text > 0;
    // The frame you last clicked in, if it's a big part of the page; otherwise the biggest frame with content
    let pick = info.filter((i) => i.f !== top && i.focus && big(i)).sort((a, b) => b.depth - a.depth)[0];
    if (!pick && (topInfo.frameset || topInfo.text < 200)) pick = info.filter((i) => i.f !== top && big(i)).sort((a, b) => b.area - a.area)[0];
    pick ||= topInfo;
    await pick.f.executeJavaScript('window.print()', true);
  } catch { fallback(); }
}

// Sync requests use Electron's net.fetch, which follows the computer's own network/proxy settings
async function syncRequest(method, url, body) {
  const res = await net.fetch(url, {
    method,
    headers: { 'content-type': 'application/json' },
    body: body ? JSON.stringify(body) : undefined,
  });
  let json = null;
  try { json = await res.json(); } catch { /* not JSON */ }
  return { status: res.status, body: json };
}

function prepareSession(id) {
  const ses = session.fromPartition(partitionFor(id));
  // Present as plain Chrome so the sites treat the app like your normal browser.
  ses.setUserAgent(ses.getUserAgent().replace(/\s(?:Electron|pts-workbench|PTS Workbench)\/\S+/gi, ''));
}

// =====================================================================================
// One window = one profile
// =====================================================================================
function createWorkbench(profile) {
  const PARTITION = partitionFor(profile.id);
  const readJson = (name, fallback) => readJsonAt(profileFile(profile.id, name), fallback);
  const writeJson = (name, value) => writeJsonAt(profileFile(profile.id, name), value);

  const savedSettings = readJson('settings.json', {});
  let settings = { ...DEFAULT_SETTINGS, ...savedSettings };
  if (!IS_BETA) settings.xtimeDashboard = false; // the app's own Xtime dashboard is in beta versions only
  // Addresses that aren't shown in Settings always come from the app itself, so an update can fix them
  for (const k of FIXED_SETTINGS) settings[k] = DEFAULT_SETTINGS[k];
  // Before employee numbers, the first window always used this technician ID by default
  if (profile.id === 'main' && !('techId' in savedSettings)) settings.techId = '594351';
  let employeeStatus = ''; // result of the last employee-number lookup, shown in Settings

  // ---------- this person's hours, flags, clock and sync ----------
  // Each window is one person with their own hours. The first window keeps the original files.
  const trackFile = profileFile(profile.id, 'tracking.json');
  const syncStatus = { state: 'off', message: '', lastSync: null };
  let syncChoice = null; // turning sync on: { flags, shifts, ros, updatedAt } while asking restore or overwrite
  let sync = null;
  const syncOn = () => !!SYNC_URL && !!settings.syncEnabled;
  const tracker = new Tracker({
    load: () => readJsonAt(trackFile, null),
    save: (data) => { writeJsonAt(trackFile, data); sync?.noteLocalChange(); },
    deferStartupCheck: syncOn(), // with sync, "forgot to clock out" is checked after hearing from the other computers
  });
  sync = new SyncEngine({
    tracker,
    identity: () => ({ enabled: !!settings.syncEnabled, storeId: settings.storeId, employeeNumber: settings.employeeNumber }),
    url: SYNC_URL,
    load: () => readJsonAt(profileFile(profile.id, 'sync.json'), null),
    save: (st) => writeJsonAt(profileFile(profile.id, 'sync.json'), st),
    request: syncRequest,
    onStatus: (st) => { Object.assign(syncStatus, st); pushState(); },
  });
  syncStatus.lastSync = sync.state.lastSync || null;
  let syncFlushed = false;
  async function flushSync() {
    if (syncFlushed) return;
    syncFlushed = true;
    try { await sync.flush(4000); } catch { /* best effort */ }
  }
  // Closing: record when (for "forgot to clock out"), then send anything not yet synced
  // ---------- turning sync on ----------
  // If the server already has a copy for this person, ask: restore this computer from it, or overwrite it?
  async function beginSync() {
    sync.paused = true;
    Object.assign(syncStatus, { state: 'syncing', message: 'Checking the sync server…' }); pushState();
    let info;
    try { info = await sync.peek(); } catch (e) {
      sync.paused = false;
      settings = { ...settings, syncEnabled: false }; writeJson('settings.json', settings);
      const msg = String(e?.message || e);
      Object.assign(syncStatus, { state: 'error', message: /employee/i.test(msg) ? msg
        : `Sync wasn't turned on: ${/fetch|network|ENOTFOUND|ECONN|ETIMEDOUT|net::/i.test(msg) ? "can't reach the sync server" : msg}. Try again.` });
      pushState();
      return;
    }
    if (!info.exists) { sync.paused = false; sync.rejoin(); sync.start(); return; } // nothing there yet: upload this computer's copy
    syncChoice = info;
    Object.assign(syncStatus, { state: 'ok', message: 'Waiting for your choice' });
    pushState();
  }
  async function chooseSync(choice) {
    if (!syncChoice) return;
    syncChoice = null;
    if (choice === 'cancel') {
      sync.paused = false;
      settings = { ...settings, syncEnabled: false }; writeJson('settings.json', settings);
      Object.assign(syncStatus, { state: 'off', message: '' }); pushState();
      return;
    }
    sync.paused = false;
    Object.assign(syncStatus, { state: 'syncing', message: '' }); pushState();
    try {
      if (choice === 'restore') await sync.restoreFromServer();
      else await sync.overwriteServer();
      sync.start();
    } catch (e) {
      Object.assign(syncStatus, { state: 'error', message: String(e?.message || e).slice(0, 120) });
    }
    pushAll();
  }

  async function closeOut() {
    tracker.appClosed();
    await flushSync();
  }
  let heartbeatTimer = null;
  if (settings.prodemandUrl === 'https://www1.prodemand.com/') settings.prodemandUrl = DEFAULT_SETTINGS.prodemandUrl; // old wrong default
  const sidebarWidth = () => Math.min(700, Math.max(260, Number(settings.sidebarWidth) || 380));

  let win, chrome, xtimeView;
  let mode = 'xtime';           // 'xtime' | 'manual'
  let drawerOpen = true;
  let chatOpen = false;          // the app's Dealership chat panel
  const loggedChatFields = new Set();
  let loggedXtimeFields = false;
  let loggedLineFields = false;
  let overlay = false;          // a dialog is open in the app UI; hide web views
  let myROs = [];
  let xtimeStatus = 'Log in to Xtime to load your ROs';
  let lastPoll = null;
  let pollTimer = null;
  let watchTimer = null;
  let nextTabId = 1;
  const workspaces = new Map(); // ro -> { ro, vin, ymm, tabs: [], activeTab }
  let activeRO = null;
  const hiddenROs = new Set();  // hidden from My ROs; unhidden only from that RO's page in Xtime
  let xtimeViewingRO = null;
  let xtimeName = '';           // technician name read from Xtime, for the title bar
  let searchQuery = '';
  let periodOffset = 0;
  let lookup = null;
  let addLookup = null;         // "Add vehicle" with no VIN: { ro, status: 'loading'|'error', message }

  // Xtime connection health (its session expires after about 2 hours)
  let xtimeFailures = 0;
  let xtimeLoadedAt = Date.now();
  let lastRecovery = 0;

  // ---------- saved logins (this window only; encrypted with the Windows account) ----------
  let pendingUser = null; // username typed on the first step of a two-step sign-in
  const autoTries = new Map(); // "host:step" -> last time "Sign in" was pressed for you
  const readLogins = () => readJson('logins.json', []);
  const canEncrypt = () => { try { return safeStorage.isEncryptionAvailable(); } catch { return false; } };

  function getLogin(host, identifierOnly) {
    if (!canEncrypt()) return null;
    const rec = readLogins().find((l) => l.host === host);
    if (!rec) return null;
    let password = '';
    if (!identifierOnly) {
      try { password = safeStorage.decryptString(Buffer.from(rec.pw, 'base64')); } catch { return null; }
    }
    // Press "Sign in" at most once per 10 minutes per page, so a wrong password can't loop
    const key = `${host}:${identifierOnly ? 'user' : 'pass'}`;
    const autoSubmit = !!settings.autoLogin && Date.now() - (autoTries.get(key) || 0) > 10 * 60 * 1000;
    if (autoSubmit) autoTries.set(key, Date.now());
    return { username: rec.username || '', password, autoSubmit };
  }

  function saveLogin(host, username, password) {
    if (!password || !canEncrypt()) return;
    if (!username && pendingUser?.host === host && Date.now() - pendingUser.at < 10 * 60 * 1000) username = pendingUser.username;
    const list = readLogins();
    const old = list.find((l) => l.host === host);
    if (old && old.username === username) {
      try { if (safeStorage.decryptString(Buffer.from(old.pw, 'base64')) === password) return; } catch { /* replace it */ }
    }
    const rec = { host, username, pw: safeStorage.encryptString(password).toString('base64'), savedAt: Date.now() };
    writeJson('logins.json', [...list.filter((l) => l.host !== host), rec]);
    notice(`Saved login for ${host}`, { removeHost: host });
    pushState();
  }

  function rememberUsername(host, username) {
    if (username) pendingUser = { host, username: String(username).slice(0, 200), at: Date.now() };
  }

  function notice(text, extra = {}) {
    if (chrome && !chrome.webContents.isDestroyed()) chrome.webContents.send('notice', { text, ...extra, at: Date.now() });
  }

  const displayName = () => xtimeName || profile.name || 'Main';
  const updateTitle = () => { if (win && !win.isDestroyed()) win.setTitle(`PTS Workbench - ${displayName()}`); };

  // ---------- persistence (only RO/VIN/vehicle + tab layout; no customer data) ----------
  function saveWorkspaces() {
    writeJson('workspaces.json', {
      activeRO,
      hidden: [...hiddenROs],
      xtimeName,
      list: [...workspaces.values()].map((ws) => ({
        ro: ws.ro, vin: ws.vin, ymm: ws.ymm,
        tabs: ws.tabs.map((t) => ({ kind: t.kind, title: t.title, ...(t.manualState ? { state: t.manualState } : {}) })),
        activeIndex: Math.max(0, ws.tabs.findIndex((t) => t.id === ws.activeTab)),
      })),
    });
  }

  function loadWorkspaces() {
    const saved = readJson('workspaces.json', null);
    for (const ro of saved?.hidden || []) hiddenROs.add(String(ro));
    if (saved?.xtimeName) xtimeName = String(saved.xtimeName);
    if (!saved?.list) return;
    for (const w of saved.list) {
      const tabs = (w.tabs?.length ? w.tabs : [{ kind: 'pts', title: 'Manual' }])
        .map((t) => ({ id: nextTabId++, kind: t.kind, title: t.title, view: null, status: '', manualState: t.state || null }));
      const ws = { ro: w.ro, vin: w.vin, ymm: w.ymm || '', tabs, activeTab: tabs[w.activeIndex || 0]?.id ?? tabs[0].id };
      if (!tabs.some((t) => t.kind === 'xtime')) addTab(ws, 'xtime', { activate: false, front: true }); // older saves
      workspaces.set(w.ro, ws);
    }
    if (saved.activeRO && workspaces.has(saved.activeRO)) activeRO = saved.activeRO;
  }

  // ---------- state to the UI ----------
  function pushState() {
    if (!chrome || chrome.webContents.isDestroyed()) return;
    const roInfo = {};
    const statusLists = tracker?.statusLists() ?? { review: [], completed: [] };
    for (const ro of new Set([...myROs.map((r) => r.ro), ...workspaces.keys(), ...(lookup ? [lookup.ro] : []),
      ...statusLists.review.map((r) => r.ro), ...statusLists.completed.map((r) => r.ro)])) {
      const s = tracker?.roSummary(ro);
      if (s) roInfo[ro] = s;
    }
    chrome.webContents.send('state', {
      mode, drawerOpen, chatOpen, settings, xtimeStatus, lastPoll, activeRO, xtimeSignin, msgCount,
      myROs, roInfo,
      tracking: tracker?.summary() ?? null,
      period: tracker?.period(periodOffset) ?? null,
      search: searchQuery && tracker ? { query: searchQuery, results: tracker.searchRO(searchQuery) } : null,
      currentPeriodTotal: tracker?.period(0).total ?? 0,
      lookup,
      employeeStatus,
      statusLists,
      addLookup,
      hidden: [...hiddenROs],
      xtimeViewingRO,
      logins: readLogins().map((l) => ({ host: l.host, username: l.username })), // never the passwords
      canSavePasswords: canEncrypt(),
      sync: { ...syncStatus, configured: !!SYNC_URL, enabled: !!settings.syncEnabled, isMain: true, choice: syncChoice },
      app: { version: app.getVersion(), update: { ...update }, beta: IS_BETA, betaUpdates: !!updatePrefs.beta },
      profile: { id: profile.id, name: profile.name, xtimeName, displayName: displayName() },
      profiles: profiles.map((p) => ({
        id: p.id, name: p.name, open: windows.has(p.id), current: p.id === profile.id,
        displayName: windows.get(p.id)?.displayName() || p.name,
      })),
      workspaces: [...workspaces.values()].map((ws) => ({
        ro: ws.ro, vin: ws.vin, ymm: ws.ymm, activeTab: ws.activeTab,
        tabs: ws.tabs.map((t) => ({ id: t.id, kind: t.kind, title: t.title, status: t.status })),
      })),
    });
  }

  // ---------- layout ----------
  function allContentViews() {
    const views = [xtimeView];
    for (const ws of workspaces.values()) for (const t of ws.tabs) if (t.view) views.push(t.view);
    return views;
  }

  function layout() {
    if (!win || win.isDestroyed()) return;
    const { width, height } = win.getContentBounds();
    chrome.setBounds({ x: 0, y: 0, width, height });
    let visible = null;
    if (!overlay) {
      if (mode === 'xtime' && settings.xtimeDashboard) {
        visible = null; // the app's own dashboard is drawn in the app's screen
      } else if (mode === 'xtime') {
        xtimeView.setBounds({ x: 0, y: TOP_BAR, width: Math.max(0, width - (chatOpen ? CHAT_W : drawerOpen ? DRAWER : 0)), height: Math.max(0, height - TOP_BAR) });
        visible = xtimeView;
      } else if (activeRO && workspaces.has(activeRO)) {
        const ws = workspaces.get(activeRO);
        const tab = ws.tabs.find((t) => t.id === ws.activeTab);
        if (tab) {
          ensureView(ws, tab);
          const side = settings.sidebarHidden ? 0 : sidebarWidth(); // the Workbench sidebar can be tucked away
          tab.view.setBounds({ x: side, y: TOP_BAR + TAB_STRIP, width: Math.max(0, width - side - (chatOpen ? CHAT_W : 0)), height: Math.max(0, height - TOP_BAR - TAB_STRIP) });
          visible = tab.view;
        }
      }
    }
    for (const v of allContentViews()) v.setVisible(v === visible);
    visibleView = visible;
  }
  let visibleView = null;
  // Print whatever page is showing (Xtime, or the open tab of a vehicle)
  function printVisible() {
    if (visibleView && !visibleView.webContents.isDestroyed()) printPage(visibleView.webContents);
    else notice('Open a page first, then print it');
  }

  // ---------- web views ----------
  function hardenWebContents(wc) {
    // Popups (e.g. wiring diagrams) open as normal windows that share this window's login.
    wc.setWindowOpenHandler(({ url }) => {
      if (!/^https?:/i.test(url)) return { action: 'deny' };
      return {
        action: 'allow',
        overrideBrowserWindowOptions: {
          autoHideMenuBar: true, title: `PTS Workbench - ${displayName()}`,
          webPreferences: {
            partition: PARTITION, sandbox: true, contextIsolation: true, plugins: true, // plugins = PDF viewer
            preload: path.join(__dirname, 'web-preload.js'), nodeIntegrationInSubFrames: true,
          },
        },
      };
    });
    wc.on('will-attach-webview', (e) => e.preventDefault());
  }

  function makeWebView() {
    const view = new WebContentsView({
      webPreferences: {
        partition: PARTITION, sandbox: true, contextIsolation: true, nodeIntegration: false, plugins: true, // plugins = PDF viewer
        preload: path.join(__dirname, 'web-preload.js'), nodeIntegrationInSubFrames: true,
      },
    });
    win.contentView.addChildView(view);
    view.setVisible(false);
    hardenWebContents(view.webContents);
    return view;
  }

  const fordLink = (kind, vin) => settings.ptsTransportUrl
    .replace('{LINK}', FORD_LINKS[kind] || FORD_LINKS.pts)
    .replace('{VIN}', encodeURIComponent(vin));
  const hostOf = (url) => { try { return new URL(url).hostname; } catch { return ''; } };

  function ensureView(ws, tab) {
    if (tab.view) return;
    if (tab.kind === 'manual') { tab.view = makeManualView(ws, tab); return; }
    if (tab.kind === 'oasisview') { tab.view = makeManualView(ws, tab, 'oasis.html'); return; }
    if (tab.kind === 'diag') { tab.view = makeManualView(ws, tab, 'diag.html'); return; }
    tab.view = makeWebView();
    const wc = tab.view.webContents;
    if (tab.kind === 'wiring') {
      openWiring(ws, tab);
    } else if (FORD_LINKS[tab.kind]) {
      setupFordTab(ws, tab);
    } else if (tab.kind === 'xtime') {
      tab.status = '';
      wc.loadURL(settings.xtimeRoUrl.replace('{RO}', encodeURIComponent(ws.ro)));
    } else if (tab.kind === 'prodemand') {
      wc.on('did-finish-load', () => fillProDemand(ws, tab));
      tab.status = 'Loading ProDemand…';
      wc.loadURL(settings.prodemandUrl);
    } else {
      tab.status = '';
      wc.loadURL(settings.oasisUrl.replace('{VIN}', encodeURIComponent(ws.vin)));
    }
  }

  // PTS / OTA / Connected Data: open Ford's transport link for this VIN. If PTS sends you to its
  // sign-in page first, open the link again once you're back (at most once every 2 minutes).
  function setupFordTab(ws, tab) {
    const wc = tab.view.webContents;
    const ptsHost = hostOf(settings.ptsUrl);
    let viaLogin = false;
    let lastReopen = 0;
    wc.on('did-navigate', (_e, url) => {
      if (hostOf(url) && hostOf(url) !== ptsHost) { viaLogin = true; tab.status = 'Log in to PTS'; pushState(); }
    });
    wc.on('did-finish-load', async () => {
      if (hostOf(wc.getURL()) !== ptsHost) return;
      if (viaLogin && Date.now() - lastReopen > 2 * 60 * 1000) {
        viaLogin = false;
        lastReopen = Date.now();
        tab.status = 'Opening vehicle…'; pushState();
        wc.loadURL(fordLink(tab.kind, ws.vin));
        return;
      }
      viaLogin = false;
      const token = (tab.vinToken = (tab.vinToken || 0) + 1);
      if (tab.status) { tab.status = ''; pushState(); }
      if (tab.cloneSnapshot) {
        const snap = tab.cloneSnapshot;
        delete tab.cloneSnapshot; // only once; a later reload starts fresh
        tab.status = 'Copying page…'; pushState();
        await applySnapshot(tab, snap, token);
        if (tab.vinToken === token && tab.status === 'Copying page…') { tab.status = ''; pushState(); }
      }
    });
    tab.status = 'Loading PTS…';
    wc.loadURL(fordLink(tab.kind, ws.vin));
  }

  function reopenFordTab(ws, tab) {
    if (!tab.view || tab.view.webContents.isDestroyed()) return;
    tab.vinToken = (tab.vinToken || 0) + 1;
    tab.status = 'Opening vehicle…'; pushState();
    tab.view.webContents.loadURL(fordLink(tab.kind, ws.vin));
  }

  // ProDemand: copy the VIN, and type it into ProDemand's VIN box if one can be found
  async function fillProDemand(ws, tab) {
    const wc = tab.view?.webContents;
    if (!wc || wc.isDestroyed()) return;
    const token = (tab.vinToken = (tab.vinToken || 0) + 1);
    try { clipboard.writeText(ws.vin); } catch {}
    for (let attempt = 0; attempt < 10; attempt++) {
      if (tab.vinToken !== token || wc.isDestroyed()) return;
      try {
        if ((await wc.executeJavaScript(`${PRODEMAND_FILL}(${JSON.stringify(ws.vin)})`, true)) === 'filled') {
          tab.status = 'VIN entered (also copied)'; pushState(); return;
        }
      } catch { /* still loading */ }
      await sleep(1000);
    }
    if (tab.vinToken === token) { tab.status = 'VIN copied — paste it into ProDemand'; pushState(); }
  }

  // ---------- the app's own workshop manual ----------
  const manualSes = session.fromPartition(PARTITION);
  const manualService = new ManualService(manualSes, profileFile(profile.id, 'manual-cache'), profileFile(profile.id, 'manual.log'));
  try {
    if (!manualSes.protocol.isProtocolHandled('ptsasset')) {
      manualSes.protocol.handle('ptsasset', (req) => {
        let target = '';
        try { target = new URL(req.url).searchParams.get('u') || ''; } catch {}
        return manualService.asset(target);
      });
    }
  } catch (e) { console.error('ptsasset', e); }
  const manualViews = new Map(); // webContents id -> { ws, tab }

  function makeManualView(ws, tab, page = 'viewer.html') {
    const view = new WebContentsView({
      webPreferences: {
        partition: PARTITION, sandbox: true, contextIsolation: true, nodeIntegration: false,
        preload: path.join(__dirname, 'manual', 'viewer-preload.js'),
      },
    });
    win.contentView.addChildView(view);
    view.setVisible(false);
    hardenWebContents(view.webContents); // links to Ford pages open in a window with your login
    const id = view.webContents.id;
    manualViews.set(id, { ws, tab });
    view.webContents.once('destroyed', () => manualViews.delete(id));
    // F12: the page inspector, for working out problems with the viewer
    view.webContents.on('before-input-event', (ev, input) => {
      if (input.type === 'keyDown' && input.key === 'F12') { ev.preventDefault(); view.webContents.openDevTools({ mode: 'detach' }); }
    });
    tab.status = '';
    view.webContents.loadFile(path.join(__dirname, 'manual', page));
    return view;
  }

  // Wiring opens straight to the vehicle (Ford's wiring page works on its own)
  async function openWiring(ws, tab) {
    const wc = tab.view.webContents;
    tab.status = 'Finding the vehicle…'; pushState();
    try {
      const book = await manualService.book(ws.vin);
      if (wc.isDestroyed()) return;
      tab.status = ''; pushState();
      wc.loadURL(`${PTS_ORIGIN}/wiring?vin=${encodeURIComponent(ws.vin)}&vehicleId=${encodeURIComponent(book.params.vehicleId)}`);
    } catch (e) {
      if (wc.isDestroyed()) return;
      tab.status = e.needsLogin ? 'Sign in to PTS, then press ↻' : (e.message || "Couldn't open wiring");
      pushState();
      wc.loadURL(settings.ptsUrl);
      if (e.needsLogin) {
        // once you're signed in, try again
        const retry = () => {
          if (hostOf(wc.getURL()) !== hostOf(PTS_ORIGIN)) return;
          wc.off('did-finish-load', retry);
          if (tab.view) openWiring(ws, tab);
        };
        wc.on('did-finish-load', retry);
      }
    }
  }

  // Sign in to PTS in a small window; resolves once PTS itself has loaded (or the window is closed)
  let signInWindow = null;
  function signInToPts() {
    if (signInWindow && !signInWindow.isDestroyed()) { signInWindow.focus(); return signInWindow.donePromise; }
    const w = new BrowserWindow({
      width: 980, height: 760, autoHideMenuBar: true, title: 'Sign in to PTS', parent: undefined,
      webPreferences: { partition: PARTITION, sandbox: true, contextIsolation: true,
        preload: path.join(__dirname, 'web-preload.js'), nodeIntegrationInSubFrames: true },
    });
    signInWindow = w;
    w.donePromise = new Promise((resolve) => {
      let seenLogin = false;
      const check = () => {
        if (w.isDestroyed()) return;
        const host = hostOf(w.webContents.getURL());
        if (host && host !== hostOf(PTS_ORIGIN)) { seenLogin = true; return; }
        if (host === hostOf(PTS_ORIGIN)) setTimeout(() => { if (!w.isDestroyed()) w.close(); }, seenLogin ? 800 : 300);
      };
      w.webContents.on('did-finish-load', check);
      w.on('closed', () => { signInWindow = null; resolve(true); });
    });
    w.loadURL(settings.ptsUrl);
    return w.donePromise;
  }

  async function manualCall(sender, name, payload) {
    const entry = manualViews.get(sender.id);
    if (!entry) return undefined;
    const { ws, tab } = entry;
    const fail = (e) => ({ error: true, needsLogin: !!e.needsLogin, message: e.message || String(e), detail: e.detail || '' });
    switch (name) {
      case 'init': {
        const open = tab.pendingOpen || null; // a procedure sent here from the Diagnostics tab before this tab existed
        tab.pendingOpen = null;
        return { vin: ws.vin, ro: ws.ro, ymm: ws.ymm, state: tab.manualState || null, open };
      }
      case 'diag': // Diagnostics tab: Ford's search for DTCs / words on this vehicle
        try {
          const q = payload || {};
          const dtcs = (Array.isArray(q.dtcs) ? q.dtcs : []).map(String).filter((x) => /^[PBUC][0-9A-F]{4,6}(?:[:|][0-9A-Z]{2,12}){0,2}$/i.test(x)).slice(0, 20);
          const sources = Array.isArray(q.sources) ? q.sources.map(String) : undefined;
          return await manualService.diagSearch(ws.vin, { text: String(q.text || '').slice(0, 200), dtcs, sources,
            rows: Math.min(200, Math.max(1, Number(q.rows) || 50)), start: Math.max(0, Number(q.start) || 0), exact: !!q.exact });
        } catch (e) { return fail(e); }
      case 'open-diag': { // from the manual's search: search this DTC in the vehicle's Diagnostics tab
        const query = String(payload || '').slice(0, 200);
        const dt = ws.tabs.find((t) => t.kind === 'diag') || addTab(ws, 'diag', { activate: false });
        ws.activeTab = dt.id;
        dt.manualState = { ...(dt.manualState || {}), query };
        if (dt.view && !dt.view.webContents.isDestroyed() && !dt.view.webContents.isLoading()) dt.view.webContents.send('manual-open', { query });
        saveWorkspaces(); layout(); pushState();
        return true;
      }
      case 'open-proc': { // Diagnostics tab: show a procedure / pinpoint test in this vehicle's Workshop Manual tab
        const q = payload || {};
        const link = { searchNumber: String(q.searchNumber || ''), procUid: String(q.procUid || ''), linkType: 'Procedure',
          title: String(q.title || '').slice(0, 200), ppt: String(q.ppt || ''), section: 'Diagnostics' };
        if (!link.searchNumber && !link.procUid) return false;
        const mt = ws.tabs.find((t) => t.kind === 'manual') || addTab(ws, 'manual', { activate: false });
        ws.activeTab = mt.id;
        if (mt.view && !mt.view.webContents.isDestroyed() && !mt.view.webContents.isLoading()) mt.view.webContents.send('manual-open', link);
        else mt.pendingOpen = link;
        saveWorkspaces(); layout(); pushState();
        return true;
      }
      case 'book':
        try {
          const b = await manualService.book(ws.vin);
          return { title: b.title, vehicle: b.vehicle };
        } catch (e) { return fail(e); }
      case 'tree':
        try { return { html: await manualService.tree(ws.vin) }; } catch (e) { return fail(e); }
      case 'proc':
        try {
          const link = payload || {};
          return await manualService.proc(ws.vin, {
            searchNumber: String(link.searchNumber || ''), procUid: String(link.procUid || ''),
            linkType: link.linkType === 'PublicationContent' ? 'PublicationContent' : 'Procedure',
          });
        } catch (e) { return fail(e); }
      case 'oasis':
        try { return await manualService.oasis(ws.vin); } catch (e) { return fail(e); }
      case 'cv':
        try { return await manualService.connectedVehicle(ws.vin); } catch (e) { return fail(e); }
      case 'copy': try { clipboard.writeText(String(payload || '').slice(0, 500)); } catch {} return true;
      case 'spa':
        try { return await manualService.spaSteps(ws.vin, { file: String(payload?.file || ''), procUid: String(payload?.procUid || '') }); } catch (e) { return fail(e); }
      case 'report':
        try { return { ok: await manualService.reportPinpoint(ws.vin, payload || {}) }; } catch (e) { return fail(e); }
      case 'signin': await signInToPts(); return true;
      case 'open-log': shell.openPath(profileFile(profile.id, 'manual.log')).catch(() => {}); return true;
      case 'log': manualService.log(`viewer: ${String(payload || '').slice(0, 500)}`); return true;
      case 'open-pts': {
        const t = addTab(ws, 'pts');
        saveWorkspaces(); layout(); pushState();
        return t.id;
      }
      case 'open-url': {
        let url = String(payload || '');
        try { url = new URL(url, PTS_ORIGIN).href; } catch { return false; }
        if (!/^https:/i.test(url)) return false;
        const w = new BrowserWindow({ width: 1100, height: 800, autoHideMenuBar: true,
          webPreferences: { partition: PARTITION, sandbox: true, contextIsolation: true, plugins: true,
            preload: path.join(__dirname, 'web-preload.js'), nodeIntegrationInSubFrames: true } });
        w.loadURL(url);
        return true;
      }
      default: return undefined;
    }
  }
  function manualSave(sender, state) {
    const entry = manualViews.get(sender.id);
    if (!entry || !state || typeof state !== 'object') return false;
    const json = JSON.stringify(state);
    if (json.length > 200000) return true; // something's wrong; don't bloat the save file
    entry.tab.manualState = JSON.parse(json);
    clearTimeout(manualSave.timer);
    manualSave.timer = setTimeout(saveWorkspaces, 1000);
    return true;
  }

  function destroyTab(tab) {
    if (!tab.view) return;
    tab.vinToken = -1;
    try { win.contentView.removeChildView(tab.view); } catch {}
    try { tab.view.webContents.close(); } catch {}
    tab.view = null;
  }

  // ---------- Xtime ----------
  // ---------- Windows notifications ----------
  // RO assigned to you, a line approved (with what Xtime says about parts), and new Xtime messages.
  // Clicking one opens that RO's Xtime page in the Workbench.
  function notifyUser(title, body, ro) {
    if (!settings.notifications || !Notification.isSupported()) return;
    try {
      const n = new Notification({ title, body, silent: false });
      n.on('click', () => {
        try { if (win.isMinimized()) win.restore(); win.show(); win.focus(); } catch {}
        if (ro) {
          const r = myROs.find((x) => x.ro === ro);
          openWorkspace({ ro, vin: r?.vin || '', ymm: r ? [r.year, r.make, r.model].filter(Boolean).join(' ') : '', tab: 'xtime' });
          layout(); pushState();
        } else { mode = 'xtime'; layout(); pushState(); }
      });
      n.show();
    } catch { /* notifications are optional */ }
  }
  let knownMine = null;            // ROs on your list at the last read (null until the first read)
  const approvedSeen = new Map();  // ro -> approved line keys at the last read
  function partsNote(r) {
    const d = r?.dash || {};
    if (d.notInStock) return '⚠ Parts not in stock';
    if (d.partsStatus) return `Parts: ${d.partsStatus}`;
    if (r?.waitingParts) return 'Waiting for parts';
    return 'No parts flagged as out of stock';
  }
  function noteAssigned(list) {
    const now = new Set(list.map((r) => r.ro));
    if (knownMine) {
      for (const r of list) {
        if (knownMine.has(r.ro)) continue;
        const ymm = [r.year, r.make, r.model].filter(Boolean).join(' ');
        notifyUser(`RO ${r.ro} assigned to you`, [ymm, r.tag && `Tag ${r.tag}`].filter(Boolean).join(' · ') || 'New RO on your list', r.ro);
      }
    }
    knownMine = now;
  }
  function noteApprovals(r, lines) {
    const key = (l) => l.key || l.dms;
    const approved = new Set((lines || []).filter((l) => l.approved && !l.primary && !l.declined).map(key));
    const before = approvedSeen.get(r.ro);
    approvedSeen.set(r.ro, approved);
    if (!before) return; // first look at this RO: nothing to compare with
    const fresh = (lines || []).filter((l) => approved.has(key(l)) && !before.has(key(l)));
    if (!fresh.length) return;
    const what = fresh.map((l) => `${l.dms || ''} ${l.complaint || l.service || ''}`.trim()).join('; ').slice(0, 140);
    notifyUser(`RO ${r.ro}: ${fresh.length === 1 ? 'line approved' : `${fresh.length} lines approved`}`, `${what}\n${partsNote(r)}`, r.ro);
  }
  // Xtime's unread-messages counter (top right of Xtime's page)
  let msgCount = null;
  async function readMsgCount() {
    const frame = findXtimeDataFrame();
    if (!frame) return;
    let n = null;
    // Unread Dealership messages, from Xtime's chat (works without Xtime's page)
    if (IS_BETA && (chatConfig || findInspectFrame())) {
      try {
        const r = await (chatConfig ? frame : findInspectFrame()).executeJavaScript(`${XTIME_CHAT}({"type":"count"}, ${chatCfgArg()})`, true);
        if (r?.config) keepChatConfig(r.config);
        if (r?.ok && Number.isFinite(r.count)) n = r.count;
      } catch {}
    }
    // Otherwise the counter on Xtime's page, when it's loaded
    const page = findInspectFrame();
    if (n == null && page) {
      try { n = await page.executeJavaScript(`(() => { const e = document.querySelector('.messages-count'); return e ? Number(e.getAttribute('data-badge')) || 0 : null; })()`, true); } catch {}
    }
    if (n == null) return;
    if (msgCount != null && n > msgCount) notifyUser('New Xtime message', n - msgCount === 1 ? `1 new message · ${n} unread` : `${n - msgCount} new messages · ${n} unread`, null);
    if (n !== msgCount) { msgCount = n; pushState(); }
  }
  const msgTimer = setInterval(readMsgCount, 20000);

  // ---------- signing in to Xtime from the app's own form ----------
  // When Xtime sends its page to Cox's sign-in, the app shows its own form and fills in Cox's real page.
  let xtimeSignin = null; // { step, error, factors, message, buttons, username, busy } while signing in
  function findSigninFrame() {
    const main = xtimeView?.webContents?.mainFrame;
    if (!main) return null;
    return (main.framesInSubtree || []).find((f) => { try { return COX_SIGNIN_HOST.test(new URL(f.url).hostname); } catch { return false; } }) || null;
  }
  async function signinRun(action) {
    const frame = findSigninFrame();
    if (!frame) return null;
    try { return await frame.executeJavaScript(`${COX_SIGNIN}(${JSON.stringify(action || { type: 'state' })})`, true); } catch { return null; }
  }
  let signinChecking = false;
  async function checkSignin() {
    if (signinChecking || !xtimeView || xtimeView.webContents.isDestroyed()) return;
    signinChecking = true;
    try {
      const frame = findSigninFrame();
      if (!frame) {
        // Landed on Scheduling soon after signing in: go to Inspect
        if (autoInspectUntil > Date.now() && onScheduling() && !xtimeView.webContents.isLoading()) switchToInspect();
        if (xtimeSignin && !['loading', 'starting'].includes(xtimeSignin.step)) { // signed in: make sure Xtime is on Inspect (it lands on Scheduling), then read it again
          // The dialog stays up ("Loading Inspect…") until the dashboard has data again
          xtimeSignin = { step: 'loading', since: Date.now(), busy: false, error: '' }; pushState();
          autoInspectUntil = Date.now() + 90 * 1000;
          setTimeout(() => ensureInspect(2), 4000);
        } else if (['loading', 'starting'].includes(xtimeSignin?.step) && !xtimeSignin.slow && Date.now() - xtimeSignin.since > 90 * 1000) {
          xtimeSignin = { ...xtimeSignin, slow: true }; pushState(); // taking long: offer a reload
        }
        return;
      }
      const st = await signinRun({ type: 'state' });
      if (!st?.ok) return;
      // Xtime's sign-in page often shows for a moment while Xtime signs in by itself (or a saved login is
      // filled in), then moves on. Typing into it then gets lost, so the form only shows once the page has
      // stayed put for a few seconds; until then the dialog says it's connecting.
      const key = `${String(frame.url).split(/[?#]/)[0]}|${st.step}`;
      if (key !== signinSettle.key || xtimeView.webContents.isLoading()) signinSettle = { key, since: Date.now() };
      const settled = Date.now() - signinSettle.since >= 4000 || Date.now() - signinActedAt < 60 * 1000;
      if (!settled) {
        if (xtimeSignin?.step !== 'starting') { xtimeSignin = { step: 'starting', since: Date.now(), busy: false, error: '' }; pushState(); }
        return;
      }
      let host = '';
      try { host = new URL(frame.url).hostname; } catch {}
      const saved = host ? getLogin(host, true) : null;
      const next = { step: st.step, error: st.error, factors: st.factors, message: st.message, buttons: st.buttons,
        username: st.username || saved?.username || '', savedPassword: !!(saved && hasSavedPassword(host)), canRemember: canEncrypt(), busy: !!xtimeSignin?.busy, host };
      if (JSON.stringify(next) !== JSON.stringify(xtimeSignin)) { xtimeSignin = next; pushState(); }
    } catch { /* checked again shortly */ } finally { signinChecking = false; }
  }
  let signinSettle = { key: '', since: 0 };
  let signinActedAt = 0; // you pressed something in the sign-in form: show its next steps right away
  const signinTimer = setInterval(checkSignin, 1500);
  const hasSavedPassword = (host) => { try { return !!getLogin(host)?.password; } catch { return false; } };

  // After signing in, Xtime opens Scheduling; the app's data comes from Inspect, so go there the same way
  // Scheduling's top-left menu does ("INSPECT": login.<xtime domain>/?pt=svp&view=svp&StoreID=<store>)
  let autoInspectUntil = Date.now() + 90 * 1000; // also right after the app starts (it may sign in on its own)
  let lastInspectSwitch = 0;
  const onScheduling = () => {
    try {
      const u = new URL(xtimeView.webContents.getURL());
      return /(^|\.)xtime\.app\.coxautoinc\.com$/i.test(u.hostname) && (/^\/panama\//i.test(u.pathname) || /^x\d*\./i.test(u.hostname));
    } catch { return false; }
  };
  function switchToInspect() {
    if (Date.now() - lastInspectSwitch < 20000) return false; // once at a time
    let domain = 'xtime.app.coxautoinc.com';
    try { domain = new URL(xtimeView.webContents.getURL()).hostname.replace(/^[^.]+\./, '') || domain; } catch {}
    lastInspectSwitch = Date.now();
    xtimeStatus = 'Opening Xtime Inspect…'; pushState();
    try { xtimeView.webContents.loadURL(`https://login.${domain}/?pt=svp&view=svp&StoreID=${encodeURIComponent(settings.storeId)}`); } catch {}
    return true;
  }
  function ensureInspect(triesLeft) {
    if (!xtimeView || xtimeView.webContents.isDestroyed()) return;
    if (findInspectFrame()) { pollXtime(); return; }
    if (findSigninFrame()) return; // still signing in (e.g. a verification step)
    if (xtimeView.webContents.isLoading()) { if (triesLeft > 0) setTimeout(() => ensureInspect(triesLeft), 2000); return; }
    if (!(onScheduling() && switchToInspect())) reloadXtime('Opening Xtime Inspect…');
    if (triesLeft > 1) setTimeout(() => ensureInspect(triesLeft - 1), 8000);
  }

  async function waitForStep(steps, ms = 8000) {
    const end = Date.now() + ms;
    while (Date.now() < end) {
      const st = await signinRun({ type: 'state' });
      if (st?.ok && (steps.includes(st.step) || st.error)) return st;
      if (!findSigninFrame()) return { ok: true, step: 'done' };
      await new Promise((r) => setTimeout(r, 300));
    }
    return await signinRun({ type: 'state' });
  }

  // Xtime's Inspect frame inside the full Xtime page (only there while that page is loaded)
  function findInspectFrame() {
    const main = xtimeView?.webContents?.mainFrame;
    if (!main) return null;
    return main.framesInSubtree.find((f) => {
      try { return new URL(f.url).hostname === settings.xtimeDataHost; } catch { return false; }
    }) || null;
  }
  // Where the app's Xtime requests run: the small background page on the Inspect site when it's ready
  // (no Xtime page needed), otherwise the full Xtime page's Inspect frame
  function findXtimeDataFrame() {
    if (IS_BETA && liteReady()) return xtimeLite.webContents.mainFrame;
    return findInspectFrame();
  }

  // ---------- the small background page ----------
  // A blank-ish page on Xtime's Inspect site (its robots.txt). It shares the window's Xtime login, so the
  // same requests Xtime's page makes work from it, without loading or running Xtime's page itself.
  // The full Xtime page is then only loaded to sign in, to show it, or when asked for.
  let xtimeLite = null;
  let liteLoading = false;
  const liteUrl = () => `https://${settings.xtimeDataHost}/robots.txt`;
  function liteReady() {
    try {
      return !!xtimeLite && !xtimeLite.webContents.isDestroyed() && !liteLoading
        && new URL(xtimeLite.webContents.getURL()).hostname === settings.xtimeDataHost;
    } catch { return false; }
  }
  function startLite() {
    if (!xtimeLite) {
      xtimeLite = new WebContentsView({ webPreferences: { partition: PARTITION, sandbox: true, contextIsolation: true, nodeIntegration: false, backgroundThrottling: false } });
      hardenWebContents(xtimeLite.webContents);
      xtimeLite.webContents.setWindowOpenHandler(() => ({ action: 'deny' }));
      xtimeLite.webContents.on('did-start-loading', () => { liteLoading = true; });
      xtimeLite.webContents.on('did-stop-loading', () => { liteLoading = false; });
    }
    try { xtimeLite.webContents.loadURL(liteUrl()); } catch {}
  }

  // Xtime's chat settings (dealer code, your Xtime username), saved from the full page so chat
  // works without it. Nothing about customers.
  let chatConfig = readJson('xtime-chat.json', null);
  function keepChatConfig(c) {
    if (!c || !c.dealerCode || !c.userName) return;
    const next = { dealerCode: c.dealerCode, userName: c.userName, env: c.env || '', chatgatewayUrl: c.chatgatewayUrl || '', storeId: c.storeId || settings.storeId };
    if (JSON.stringify(next) !== JSON.stringify(chatConfig)) { chatConfig = next; writeJson('xtime-chat.json', chatConfig); }
  }
  const chatCfgArg = () => JSON.stringify(chatConfig || { storeId: settings.storeId });

  // The full Xtime page: put away (blank) while the app's dashboard is in use and everything works without it
  let xtimeParked = false;
  let liteTriedAt = Date.now();
  function unparkXtime(status) {
    if (!xtimeParked) return;
    xtimeParked = false;
    if (status) { xtimeStatus = status; pushState(); }
    try { xtimeView.webContents.loadURL(settings.xtimeUrl); } catch {}
  }
  function parkCheck() {
    if (xtimeParked || !settings.xtimeDashboard || xtimeSignin || !liteReady() || !chatConfig) return;
    if (!findInspectFrame() || xtimeView.webContents.isLoading()) return;      // still signing in / getting to Inspect
    if (!lastPoll || Date.now() - lastPoll > 3 * 60 * 1000 || xtimeFailures) return; // the background page must be working
    if (Date.now() - xtimeLoadedAt < 30 * 1000) return;
    xtimeParked = true;
    try { xtimeView.webContents.loadURL('about:blank'); } catch {}
  }

  let polling = false;
  async function pollXtime() {
    if (polling) return;
    polling = true;
    try { await pollXtimeOnce(); } finally { polling = false; }
  }

  // Reload the Xtime page: it signs back in on its own while the overall login is still good,
  // otherwise it shows the sign-in page, which the saved password can fill in.
  function reloadXtime(status) {
    lastRecovery = Date.now();
    xtimeFailures = 0;
    if (status) { xtimeStatus = status; pushState(); }
    xtimeParked = false;
    try { xtimeView.webContents.loadURL(settings.xtimeUrl); } catch {}
  }

  // When Xtime is stuck on an error page (e.g. a sign-in step that can't be reloaded), start it fresh
  const onLoginPage = () => {
    try {
      const u = new URL(xtimeView.webContents.getURL());
      return u.protocol === 'https:' && u.hostname !== settings.xtimeDataHost;
    } catch { return false; }
  };

  function xtimeFailed(message, expired) {
    xtimeFailures++;
    if (expired && xtimeParked) { reloadXtime('Xtime session expired — reconnecting…'); return; }
    if ((expired || xtimeFailures >= 2) && Date.now() - lastRecovery > 10 * 60 * 1000) {
      reloadXtime('Xtime session expired — reconnecting…');
      return;
    }
    xtimeStatus = expired ? 'Xtime session expired — log in to Xtime' : message;
    pushState();
  }

  // Employee number → Xtime technician ID (and your name for the title bar), from the store's list.
  // Looked up once per employee number; the result is saved with the settings.
  async function resolveTechId(frame, force = false) {
    const num = String(settings.employeeNumber || '').trim();
    if (!num) return;
    if (!force && settings.techIdFor === num && settings.techId) return;
    let res = null;
    try {
      res = await frame.executeJavaScript(
        `${XTIME_EMPLOYEE}(${JSON.stringify(num)}, ${JSON.stringify(settings.storeId)}, ${JSON.stringify(settings.xtimeEmployeesPath)})`, true);
    } catch { res = null; }
    if (res?.ok) {
      settings.techId = res.id;
      settings.techIdFor = num;
      writeJson('settings.json', settings);
      employeeStatus = `Found: ${res.name || 'employee ' + num}${res.active ? '' : ' (marked inactive in Xtime)'}`;
      if (res.name && res.name !== xtimeName) { xtimeName = res.name; updateTitle(); saveWorkspaces(); }
    } else {
      employeeStatus = res?.error || "Couldn't look up the employee number yet";
    }
    pushAll();
  }

  async function pollXtimeOnce() {
    const frame = findXtimeDataFrame();
    if (!frame) {
      if (xtimeParked) { unparkXtime('Opening Xtime…'); if (!liteLoading) startLite(); return; }
      if (onLoginPage()) {
        // Xtime has sent the page to its sign-in screen; a saved login fills it in
        xtimeStatus = 'Log in to Xtime to load your ROs';
        pushState();
      } else {
        // An error page, or a page that never finished: start Xtime fresh (at most every 2 minutes)
        xtimeFailures++;
        if (Date.now() - lastRecovery > 2 * 60 * 1000) reloadXtime('Xtime page failed to load — reloading…');
        else { xtimeStatus = 'Xtime page failed to load — retrying shortly'; pushState(); }
      }
      return;
    }
    await resolveTechId(frame);
    if (!settings.techId) {
      if (xtimeSignin?.step === 'starting') xtimeSignin = null; // nothing to load until the employee number is set
      xtimeStatus = settings.employeeNumber ? (employeeStatus || 'Looking up your employee number…') : 'Enter your employee number in ⚙ Settings';
      pushState();
      return;
    }
    try {
      const res = await frame.executeJavaScript(
        `${XTIME_EXTRACT}(${JSON.stringify(settings.techId)}, ${JSON.stringify(settings.xtimeApiPath)}, ${JSON.stringify(String(settings.xtimeEmployeesPath || '').replace('{STORE}', encodeURIComponent(settings.storeId)))})`, true);
      if (!res?.ok) { xtimeFailed(res?.error || 'Could not read Xtime', !!res?.expired); return; }
      xtimeFailures = 0;
      myROs = res.mine;
      if (xtimeSignin?.step === 'loading' || xtimeSignin?.step === 'starting') xtimeSignin = null; // the data is here: close the dialog
      noteAssigned(myROs);
      // Xtime's field names (names only, no values), so dashboard columns that don't match can be fixed
      if (res.fieldNames && !loggedXtimeFields) {
        loggedXtimeFields = true;
        try { fs.writeFileSync(userFile('xtime-fields.txt'), `${new Date().toISOString()}\n${res.fieldNames.join('\n')}\n`); } catch {}
      }
      // Move ROs to Review when they go to Pending Review or close (skip an empty dashboard: likely a glitch)
      if (res.total > 0) tracker.syncStatus(settings.techId, myROs);
      lastPoll = Date.now();
      xtimeStatus = `${myROs.length} of ${res.total} ROs are yours`;
      // The name from the employee list wins; the RO data's name is only a fallback without an employee number
      if (res.techName && !settings.employeeNumber && res.techName !== xtimeName) { xtimeName = res.techName; updateTitle(); saveWorkspaces(); pushAll(); }
      for (const r of myROs) {
        const ws = workspaces.get(r.ro);
        if (ws) ws.ymm = [r.year, r.make, r.model].filter(Boolean).join(' ');
      }
      pushState(); // show the list right away, then fill in the lines

      let failed = 0;
      const mineSet = new Set(myROs.map((r) => r.ro));
      const others = [...workspaces.values()].filter((w) => !mineSet.has(w.ro)).map((w) => ({ ro: w.ro, other: true }));
      for (const r of [...myROs, ...others]) {
        try {
          const d = await frame.executeJavaScript(
            `${XTIME_DETAILS}(${JSON.stringify(r.ro)}, ${JSON.stringify(settings.xtimeDetailsPath)})`, true);
          if (d?.ok) {
            if (!r.other) noteApprovals(r, d.lines);
            tracker.updateLines(r.ro, d.lines, r.other ? '' : [r.year, r.make, r.model].filter(Boolean).join(' '));
          }
          else if (!r.other) failed++; // someone else's RO may have closed; don't count it as a problem
        } catch { failed++; }
      }
      if (failed) xtimeStatus += ` · lines unavailable for ${failed}`;
    } catch (e) {
      xtimeFailed('Could not read Xtime (still loading?)', false);
    }
    pushState();
  }

  // RO number from an address like /RepairOrder/Details/820367 (built from the RO details path setting)
  function viewingROFromXtime() {
    const frames = xtimeView?.webContents?.mainFrame?.framesInSubtree || [];
    const tpl = String(settings.xtimeDetailsPath || '/RepairOrder/Details/{RO}');
    const re = new RegExp(tpl.replace(/[.*+?^$()|[\]\\]/g, '\\$&').replace('\\{RO\\}', '([A-Za-z0-9-]+)').replace('{RO}', '([A-Za-z0-9-]+)'), 'i');
    for (const f of frames) {
      try {
        const m = new URL(f.url).pathname.match(re);
        if (m) return decodeURIComponent(m[1]);
      } catch { /* about:blank etc. */ }
    }
    return null;
  }

  async function lookupRO(ro) {
    if (!/^\w{1,20}$/.test(ro)) return;
    lookup = { ro, status: 'loading', message: 'Loading lines from Xtime…' };
    pushState();
    const frame = findXtimeDataFrame();
    if (!frame) {
      lookup = { ro, status: 'error', message: 'Xtime isn\'t logged in, so its lines can\'t be loaded. Add them below by hand.' };
      pushState();
      return;
    }
    try {
      const d = await frame.executeJavaScript(
        `${XTIME_DETAILS}(${JSON.stringify(ro)}, ${JSON.stringify(settings.xtimeDetailsPath)}, ${JSON.stringify(settings.xtimeApiPath)})`, true);
      if (d?.ok) {
        tracker.updateLines(ro, d.lines, d.vehicle?.ymm || '');
        const shown = tracker.roSummary(ro)?.lines.filter((l) => !l.manual).length ?? 0;
        const hidden = d.lines.length - shown;
        lookup = {
          ro, status: 'ok',
          message: !d.lines.length ? 'Xtime has no lines for this RO. Add them below by hand.'
            : hidden ? `Xtime has ${d.lines.length} lines on this RO; ${hidden} declined or not approved, so not shown.` : '',
        };
      } else {
        const why = d?.notOnDashboard ? 'it isn\'t on the Xtime dashboard any more (closed?)' : (d?.error || 'unknown error');
        lookup = { ro, status: 'error', message: `Couldn't load RO ${ro} from Xtime: ${why}. Add its lines below by hand.` };
      }
    } catch (e) {
      lookup = { ro, status: 'error', message: `Couldn't load RO ${ro} from Xtime. Add its lines below by hand.` };
    }
    pushAll();
  }

  function startPolling() {
    clearInterval(pollTimer);
    pollTimer = setInterval(pollXtime, Math.max(15, Number(settings.pollSeconds) || 60) * 1000);
  }

  // ---------- workspaces ----------
  const TAB_NAMES = { diag: 'Diagnostics', xtime: 'Xtime', oasisview: 'OASIS', manual: 'Workshop Manual', wiring: 'Wiring', pts: 'PTS', ota: 'OTA Updates', cvd: 'Connected Data', prodemand: 'ProDemand', oasis: 'OASIS' };
  function addTab(ws, kind, { activate = true, front = false } = {}) {
    const n = ws.tabs.filter((t) => t.kind === kind).length;
    const base = TAB_NAMES[kind] || 'Manual';
    const tab = { id: nextTabId++, kind, title: n ? `${base} ${n + 1}` : base, view: null, status: '' };
    if (front) ws.tabs.unshift(tab); else ws.tabs.push(tab);
    if (activate) ws.activeTab = tab.id;
    return tab;
  }

  function openWorkspace({ ro, vin, ymm, tab }) {
    ro = String(ro).trim();
    vin = String(vin).trim().toUpperCase();
    if (!ro || vin.length !== 17) return;
    let ws = workspaces.get(ro);
    if (!ws) {
      // First tab: the RO in Xtime; then PTS for a Ford/Lincoln/Mercury, ProDemand for anything else
      ws = { ro, vin, ymm: ymm || '', tabs: [], activeTab: null };
      workspaces.set(ro, ws);
      addTab(ws, 'xtime', { activate: false });
      if (isFordVehicle(vin, ymm)) { addTab(ws, 'pts'); addTab(ws, 'manual', { activate: false }); } // Xtime | PTS | Workshop Manual
      else addTab(ws, 'prodemand');
    } else if (ws.vin !== vin) {
      ws.vin = vin; // VIN corrected in Xtime: re-enter it in open manual tabs
      for (const t of ws.tabs) if (FORD_LINKS[t.kind] && t.view) reopenFordTab(ws, t);
      for (const t of ws.tabs) {
        if (t.kind === 'manual') t.manualState = null; // the open procedures were for the old VIN
        if (t.kind === 'diag') t.manualState = null;
        if ((t.kind === 'manual' || t.kind === 'wiring' || t.kind === 'oasisview' || t.kind === 'diag') && t.view) destroyTab(t); // reopen for the right vehicle
      }
    }
    if (tab) { // e.g. from the dashboard: show the RO's Xtime page
      const t = ws.tabs.find((x) => x.kind === tab) || (tab === 'xtime' ? addTab(ws, 'xtime', { activate: false }) : null);
      if (t) ws.activeTab = t.id;
    }
    activeRO = ro;
    mode = 'manual';
    saveWorkspaces();
    layout();
    pushState();
  }

  // Messages you've read in the app (Xtime message ids only, no text): shown as read right away,
  // even before Xtime's own lists catch up. Kept 60 days.
  const READ_KEEP = 60 * 24 * 60 * 60 * 1000;
  const readCache = new Map(Object.entries(readJson('chat-read.json', {}) || {}).filter(([, t]) => Date.now() - t < READ_KEEP));
  const markLocal = (list) => (list || []).map((m) => (m.unread && readCache.has(String(m.id)) ? { ...m, unread: false } : m));
  const readBusy = new Map(); // channelId -> when a read receipt was last sent
  // Opening a conversation marks it read, like Xtime does. Xtime is asked which of your messages there are still
  // unread (it knows best), so this runs on every open (at most every 15 s per conversation).
  // chat-read.log: what happened (counts and ids only, no message text), for checking it works.
  const readLog = (line) => {
    try {
      const f = userFile('chat-read.log');
      try { if (fs.statSync(f).size > 256 * 1024) fs.renameSync(f, f + '.old'); } catch {}
      fs.appendFileSync(f, `${new Date().toISOString()} ${line}\n`);
    } catch { /* best effort */ }
  };
  async function sendReadReceipts(channelId, messages) {
    if (!IS_BETA || !channelId) return;
    if (Date.now() - (readBusy.get(channelId) || 0) < 15 * 1000) return; // already done just now
    readBusy.set(channelId, Date.now());
    const frame = findXtimeDataFrame();
    if (!frame || !chatConfig) { readLog(`${channelId} skipped: ${!frame ? 'Xtime not loaded' : 'chat settings not known yet'}`); return; }
    let r = null;
    try { r = await frame.executeJavaScript(`${XTIME_CHAT}(${JSON.stringify({ type: 'read', channelId })}, ${chatCfgArg()})`, true); } catch (e) { r = { ok: false, error: e.message }; }
    const shownUnread = (messages || []).filter((m) => m.unread && !m.mine).length;
    readLog(`${channelId} unread here ${shownUnread} · Xtime said unread ${r?.tried ?? (r?.ids || []).length} ${r?.shape || ''} · ${r?.ok ? 'receipts sent' + (r.ids?.length ? ` for ${r.ids.join(',')}` : ' (none needed)') : `FAILED: ${r?.error || 'no answer'}`}`);
    if (!r?.ok) { readBusy.delete(channelId); return; }                 // try again next time it's opened
    const now = Date.now();
    // Everything others sent in this conversation counts as read here now
    for (const id of [...(r.ids || []), ...(messages || []).filter((m) => !m.mine).map((m) => m.id)]) if (id) readCache.set(String(id), now);
    for (const [id, t] of readCache) if (now - t > READ_KEEP) readCache.delete(id);
    writeJson('chat-read.json', Object.fromEntries(readCache));
    if (chrome && !chrome.webContents.isDestroyed()) chrome.webContents.send('chat', { kind: 'read', channelId, ids: [...readCache.keys()] });
    setTimeout(readMsgCount, 3000); // the unread counter catches up
  }

  async function chatRun(action, kind) {
    const reply = (r) => { if (chrome && !chrome.webContents.isDestroyed()) chrome.webContents.send('chat', { kind, channelId: action.channelId, ...r }); };
    // Chat settings come from Xtime's full page the first time; after that the background page is enough
    const frame = chatConfig ? findXtimeDataFrame() : (findInspectFrame() || findXtimeDataFrame());
    if (!frame) return reply({ ok: false, error: 'Xtime is still loading' });
    try {
      const r = await frame.executeJavaScript(`${XTIME_CHAT}(${JSON.stringify(action)}, ${chatCfgArg()})`, true);
      if (r?.config) keepChatConfig(r.config);
      if (r?.needPage) unparkXtime(); // e.g. the first time: load Xtime's page once to learn its chat settings
      if (r?.fieldNames?.length && !loggedChatFields.has(kind)) { // field names only, for mapping; no message text
        loggedChatFields.add(kind);
        try { fs[loggedChatFields.size === 1 ? 'writeFileSync' : 'appendFileSync'](userFile('xtime-chat-fields.txt'), `${new Date().toISOString()} ${kind}\n${r.fieldNames.join('\n')}\n`); } catch {}
      }
      if (!r?.ok) return reply({ ok: false, error: r?.error || 'Could not read Xtime chat' });
      if (kind === 'list') reply({ ok: true, conversations: markLocal(r.conversations) });
      else if (kind === 'history') {
        reply({ ok: true, messages: markLocal(r.messages) });
        sendReadReceipts(action.channelId, markLocal(r.messages)); // you've opened it: mark it read, like Xtime does
      }
      else if (kind === 'roster') reply({ ok: true, people: r.people });
      else reply({ ok: true });
    } catch (e) { reply({ ok: false, error: e.message || String(e) }); }
  }

  // Shared (tracker) changes refresh every window; window-only changes refresh just this one.
  const handlers = {
    'ui-ready': () => pushState(),
    'set-mode': (m) => { mode = m === 'manual' ? 'manual' : 'xtime'; layout(); pushState(); },
    // The messages counter: Xtime's chat is on Xtime's own page for now
    'open-messages': () => {
      mode = 'xtime';
      if (settings.xtimeDashboard) { settings = { ...settings, xtimeDashboard: false }; writeJson('settings.json', settings); }
      unparkXtime('Opening Xtime…');
      chatOpen = false;
      layout(); pushState();
    },
    // The app's own read-only Dealership chat (Xtime's chat gateway, read through the Inspect page).
    // Messages are shown and forgotten: never saved to a file, never synced, nothing is sent or marked read.
    'chat-toggle': ({ open } = {}) => { if (!IS_BETA) open = false; chatOpen = open == null ? !chatOpen : !!open; layout(); pushState(); },
    // your messages from the last 2 weeks, plus everything (about 4 months back) on ROs assigned to you
    'chat-list': async () => chatRun({ type: 'list', days: 14, roDays: 120, ros: [...new Set([...myROs.map((r) => r.ro), ...workspaces.keys()])] }, 'list'),
    'chat-history': async ({ channelId } = {}) => { if (channelId) chatRun({ type: 'history', channelId: String(channelId) }, 'history'); },
    'chat-roster': async () => chatRun({ type: 'roster' }, 'roster'),
    // Sends only what you typed, only when you press Send, to a conversation that already exists
    'chat-send': async ({ channelId, ro, text } = {}) => {
      if (!IS_BETA) return;
      text = String(text || '').slice(0, 2000);
      if (!channelId || !text.trim()) return;
      await chatRun({ type: 'send', channelId: String(channelId), ro: String(ro || ''), text }, 'send');
      setTimeout(() => chatRun({ type: 'history', channelId: String(channelId) }, 'history'), 1500);
    },
    // The dashboard's expanded RO: its lines, and the Done button
    'ro-lines': async ({ ro } = {}) => {
      ro = String(ro || '');
      const reply = (r) => { if (chrome && !chrome.webContents.isDestroyed()) chrome.webContents.send('ro-lines', { ro, ...r }); };
      const frame = findXtimeDataFrame();
      if (!frame) return reply({ ok: false, error: 'Xtime is still loading' });
      try {
        const r = await frame.executeJavaScript(`${XTIME_RO}(${JSON.stringify(settings.storeId)}, ${JSON.stringify(ro)}, {"type":"get"})`, true);
        if (r?.fieldNames && !loggedLineFields) {
          loggedLineFields = true;
          try { fs.writeFileSync(userFile('xtime-line-fields.txt'), `${new Date().toISOString()}\n${r.fieldNames.join('\n')}\n`); } catch {}
        }
        reply(r?.ok ? { ok: true, lines: r.lines } : { ok: false, error: r?.error || 'Could not read Xtime' });
      } catch (e) { reply({ ok: false, error: e.message || String(e) }); }
    },
    'ro-line-done': async ({ ro, id, value } = {}) => {
      const frame = findXtimeDataFrame();
      if (!frame || id == null) return;
      try {
        const r = await frame.executeJavaScript(
          `${XTIME_RO}(${JSON.stringify(settings.storeId)}, ${JSON.stringify(String(ro))}, ${JSON.stringify({ type: 'done', id, value: !!value })})`, true);
        if (!r?.ok) notice(`Couldn't update the line in Xtime: ${r?.error || 'error'}`);
      } catch (e) { notice(`Couldn't update the line in Xtime: ${e.message || e}`); }
      handlers['ro-lines']({ ro });
    },
    // The app's "Sign in to Xtime" form: username and password (both steps), a code, a choice, or a page button
    'xtime-signin': async ({ username, password, remember, code, choose, press } = {}) => {
      if (!xtimeSignin || xtimeSignin.busy || xtimeSignin.step === 'starting' || xtimeSignin.step === 'loading') return;
      signinActedAt = Date.now();
      xtimeSignin = { ...xtimeSignin, busy: true, error: '' }; pushState();
      let res = null;
      try {
        if (password != null) {
          // Left blank with a saved login: use the saved password (it never goes to the screen)
          if (!password && xtimeSignin?.host) {
            const saved = getLogin(xtimeSignin.host);
            if (saved?.password && (!username || !saved.username || saved.username.toLowerCase() === String(username).toLowerCase())) {
              password = saved.password; username = username || saved.username; remember = false;
            }
          }
          if (!password) { res = { ok: false, error: 'Enter your password' }; return; }
          let st = await signinRun({ type: 'state' });
          if (st?.step === 'username') {
            res = await signinRun({ type: 'username', value: String(username || '') });
            if (res?.ok !== false) st = await waitForStep(['password', 'code', 'choose', 'other']);
          }
          if (st?.step === 'password' && !st.error) {
            res = await signinRun({ type: 'password', value: String(password) });
            if (remember && res?.ok !== false && xtimeSignin?.host) saveLogin(xtimeSignin.host, String(username || st.username || ''), String(password));
            await waitForStep(['code', 'choose', 'eula', 'duplicate', 'other'], 6000);
          } else if (st?.error) res = st;
        } else if (code != null) {
          res = await signinRun({ type: 'code', value: String(code).replace(/\s+/g, '') });
          await waitForStep(['other', 'eula', 'duplicate'], 6000);
        } else if (choose != null) {
          res = await signinRun({ type: 'choose', index: Number(choose) });
          await waitForStep(['code'], 6000);
        } else if (press) {
          res = await signinRun({ type: 'press', text: String(press) });
        }
      } finally {
        if (xtimeSignin) xtimeSignin = { ...xtimeSignin, busy: false, error: res?.ok === false ? res.error : (res?.error || '') };
        await checkSignin();
        pushState();
      }
    },
    'toggle-dashboard': () => {
      if (!IS_BETA) return;
      settings = { ...settings, xtimeDashboard: !settings.xtimeDashboard };
      writeJson('settings.json', settings);
      if (!settings.xtimeDashboard) unparkXtime('Opening Xtime…');
      layout(); pushState();
    },
    'toggle-drawer': () => { drawerOpen = !drawerOpen; layout(); pushState(); },
    'set-overlay': (on) => { overlay = !!on; layout(); },
    // ↻: refresh the list; if Xtime has been failing, start the Xtime page fresh instead
    'refresh-ros': () => {
      if (xtimeFailures > 0 || (!findXtimeDataFrame() && !onLoginPage())) { reloadXtime('Reloading Xtime…'); startLite(); }
      else pollXtime();
    },
    // Review / Completed
    'mark-completed': (ro) => {
      ro = String(ro);
      tracker.markCompleted(ro);
      if (workspaces.has(ro)) handlers['close-ro'](ro); // frees its tabs; it can be opened again from Completed
      notice(`RO ${ro} moved to Completed`);
      pushAll();
    },
    'move-back': (ro) => { tracker.moveBack(String(ro)); notice(`RO ${ro} moved back to Active`); pushAll(); },
    'open-any': async (ro) => {
      ro = String(ro);
      if (workspaces.has(ro)) { handlers['select-ro'](ro); return; }
      const rec = tracker.data.ros[ro];
      await handlers['add-vehicle']({ ro, vin: rec?.vin || '', ymm: rec?.ymm || '' });
      if (addLookup?.ro === ro && addLookup.status === 'error') { notice(addLookup.message); addLookup = null; pushState(); }
    },
    'open-ro': (payload) => openWorkspace(payload),
    // "Add vehicle" with the VIN left blank: get the VIN (and vehicle) from Xtime, then open it
    'add-vehicle': async ({ ro, vin, ymm }) => {
      ro = String(ro || '').trim();
      vin = String(vin || '').trim().toUpperCase();
      if (!/^\w{1,20}$/.test(ro)) return;
      if (vin) { addLookup = null; openWorkspace({ ro, vin, ymm }); return; }
      const known = myROs.find((r) => r.ro === ro) || (workspaces.get(ro) && { vin: workspaces.get(ro).vin, ymm: workspaces.get(ro).ymm });
      if (known?.vin) { addLookup = null; openWorkspace({ ro, vin: known.vin, ymm: ymm || known.ymm || [known.year, known.make, known.model].filter(Boolean).join(' ') }); return; }
      const frame = findXtimeDataFrame();
      if (!frame) {
        addLookup = { ro, status: 'error', message: "Xtime isn't logged in, so the VIN can't be looked up. Log in to Xtime or type the VIN." };
        pushState(); return;
      }
      addLookup = { ro, status: 'loading', message: `Looking up RO ${ro} in Xtime…` };
      pushState();
      let res = null;
      try {
        res = await frame.executeJavaScript(
          `${XTIME_VEHICLE}(${JSON.stringify(ro)}, ${JSON.stringify(settings.xtimeApiPath)}, ${JSON.stringify(settings.xtimeDetailsPath)})`, true);
      } catch { res = null; }
      if (addLookup?.ro !== ro) return; // dialog closed or another RO asked for meanwhile
      if (!res?.ok) {
        addLookup = { ro, status: 'error', message: `${res?.error || "Couldn't reach Xtime"}. Type the VIN to open it anyway.` };
        pushState(); return;
      }
      addLookup = null;
      openWorkspace({ ro, vin: res.vin, ymm: ymm || res.ymm });
      // and load its lines straight away
      try {
        const d = await frame.executeJavaScript(
          `${XTIME_DETAILS}(${JSON.stringify(ro)}, ${JSON.stringify(settings.xtimeDetailsPath)}, ${JSON.stringify(settings.xtimeApiPath)})`, true);
        if (d?.ok) { tracker.updateLines(ro, d.lines, ymm || res.ymm); pushAll(); }
      } catch { /* the next refresh will try again */ }
    },
    'cancel-add': () => { addLookup = null; pushState(); },
    'hide-ro': (ro) => { hiddenROs.add(String(ro)); saveWorkspaces(); pushState(); },
    'unhide-ro': (ro) => { hiddenROs.delete(String(ro)); saveWorkspaces(); pushState(); },
    'unhide-all': () => { hiddenROs.clear(); saveWorkspaces(); pushState(); },
    'reorder-ws': (order) => {
      if (!Array.isArray(order)) return;
      const keep = order.map(String);
      const entries = [
        ...keep.filter((ro) => workspaces.has(ro)).map((ro) => [ro, workspaces.get(ro)]),
        ...[...workspaces].filter(([ro]) => !keep.includes(ro)),
      ];
      workspaces.clear();
      for (const [ro, ws] of entries) workspaces.set(ro, ws);
      saveWorkspaces(); pushState();
    },
    'select-ro': (ro) => {
      if (!workspaces.has(ro)) return;
      activeRO = ro; mode = 'manual';
      saveWorkspaces(); layout(); pushState();
    },
    // Shift clock and flags are shared by every window
    'clock-in': () => { tracker.clockIn(); pushAll(); },
    'clock-out': () => { tracker.clockOut(); pushAll(); },
    'set-line-hours': ({ ro, key, hours }) => { tracker.setOverride(String(ro), String(key), hours); pushAll(); },
    'flag-line': ({ ro, key }) => { tracker.flagLine(String(ro), String(key)); pushAll(); },
    'rename-line': ({ ro, key, name }) => { tracker.renameLine(String(ro), String(key), name); pushAll(); },
    'unflag-line': ({ ro, key }) => { tracker.unflagLine(String(ro), String(key)); pushAll(); },
    'update-flag': ({ id, changes }) => { tracker.updateFlag(Number(id), changes || {}); pushAll(); },
    'delete-flag': (id) => { tracker.deleteFlag(Number(id)); pushAll(); },
    'search-flags': (q) => { searchQuery = String(q || '').replace(/[^\w-]/g, '').slice(0, 20); pushState(); },
    'set-period': (offset) => { periodOffset = Math.min(0, Math.round(Number(offset) || 0)); pushState(); },
    'lookup-ro': (ro) => lookupRO(String(ro).trim()),
    'clear-lookup': () => { lookup = null; pushState(); },
    'add-manual-line': ({ ro, dms, complaint, hours, date }) => {
      ro = String(ro).trim();
      if (!ro) return;
      const key = tracker.addManualLine(ro, { dms, complaint, hours });
      // A date in the past is logged at noon that day, so it lands in the right pay period
      let when = null;
      if (date) {
        const [y, m, d] = String(date).split('-').map(Number);
        const picked = new Date(y, m - 1, d, 12).getTime();
        const today = new Date(); today.setHours(0, 0, 0, 0);
        if (Number.isFinite(picked) && picked < today.getTime()) when = picked;
      }
      tracker.flagLine(ro, key, when);
      if (!lookup || lookup.ro !== ro) lookup = { ro, status: 'ok', message: '' };
      pushAll();
    },
    'toggle-sidebar': (hide) => {
      settings = { ...settings, sidebarHidden: hide == null ? !settings.sidebarHidden : !!hide };
      writeJson('settings.json', settings);
      layout(); pushState();
    },
    'set-sidebar': (w) => {
      settings.sidebarWidth = Math.round(Number(w) || 380);
      layout();
      clearTimeout(handlers.saveSidebarSoon);
      handlers.saveSidebarSoon = setTimeout(() => writeJson('settings.json', settings), 500);
    },
    'close-ro': (ro) => {
      const ws = workspaces.get(ro);
      if (!ws) return;
      ws.tabs.forEach(destroyTab);
      workspaces.delete(ro);
      if (activeRO === ro) activeRO = [...workspaces.keys()][0] ?? null;
      saveWorkspaces(); layout(); pushState();
    },
    'add-tab': async ({ ro, kind }) => {
      const ws = workspaces.get(ro);
      if (!ws) return;
      // "+ Manual" while looking at a PTS tab copies that tab: same vehicle, same page, same scroll.
      const src = ws.tabs.find((t) => t.id === ws.activeTab);
      let snapshot = null;
      const KINDS = ['diag', 'oasisview', 'manual', 'wiring', 'pts', 'ota', 'cvd', 'prodemand', 'oasis', 'xtime'];
      kind = KINDS.includes(kind) ? kind : 'pts';
      if (kind === 'pts' && src?.kind === 'pts' && src.view && !src.view.webContents.isDestroyed()) {
        try { snapshot = await snapshotFrames(src.view.webContents); } catch { snapshot = null; }
      }
      const tab = addTab(ws, kind);
      if (snapshot) {
        tab.cloneSnapshot = snapshot;
        tab.title = `${src.title} copy`.slice(0, 40);
      }
      saveWorkspaces(); layout(); pushState();
    },
    'select-tab': ({ ro, id }) => {
      const ws = workspaces.get(ro);
      if (!ws?.tabs.some((t) => t.id === id)) return;
      ws.activeTab = id;
      saveWorkspaces(); layout(); pushState();
    },
    'close-tab': ({ ro, id }) => {
      const ws = workspaces.get(ro);
      if (!ws) return;
      const i = ws.tabs.findIndex((t) => t.id === id);
      if (i < 0) return;
      destroyTab(ws.tabs[i]);
      ws.tabs.splice(i, 1);
      if (!ws.tabs.length) { handlers['close-ro'](ro); return; }
      if (ws.activeTab === id) ws.activeTab = ws.tabs[Math.max(0, i - 1)].id;
      saveWorkspaces(); layout(); pushState();
    },
    'rename-tab': ({ ro, id, title }) => {
      const tab = workspaces.get(ro)?.tabs.find((t) => t.id === id);
      if (!tab) return;
      tab.title = String(title).trim().slice(0, 40) || tab.title;
      saveWorkspaces(); pushState();
    },
    'reenter-vin': ({ ro, id }) => {
      const ws = workspaces.get(ro);
      const tab = ws?.tabs.find((t) => t.id === id);
      if (FORD_LINKS[tab?.kind] && tab.view) reopenFordTab(ws, tab);
      if (tab?.kind === 'prodemand' && tab.view) { tab.status = 'Entering VIN…'; pushState(); fillProDemand(ws, tab); }
    },
    'copy-vin': (ro) => {
      const ws = workspaces.get(ro);
      if (!ws) return;
      try { clipboard.writeText(ws.vin); notice(`VIN ${ws.vin} copied`); } catch {}
    },
    'remove-login': (host) => {
      writeJson('logins.json', readLogins().filter((l) => l.host !== String(host)));
      notice(`Removed saved login for ${host}`);
      pushState();
    },
    'print-page': () => printVisible(),
    'reload-tab': ({ ro, id }) => {
      const tab = workspaces.get(ro)?.tabs.find((t) => t.id === id);
      tab?.view?.webContents.reload();
    },
    // Start from the Xtime start page rather than re-loading the current address, which may be a
    // one-time sign-in step that fails when loaded twice
    'reload-xtime': () => reloadXtime('Reloading Xtime…'),
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
      const xtimeChanged = clean.xtimeUrl && clean.xtimeUrl !== settings.xtimeUrl;
      const syncWas = !!settings.syncEnabled;
      const empChanged = clean.employeeNumber !== undefined && clean.employeeNumber !== settings.employeeNumber;
      const storeChanged = clean.storeId !== undefined && clean.storeId !== settings.storeId;
      // New employee number or store: look the technician ID up again (and don't keep showing the old one's ROs)
      if (empChanged || storeChanged) { clean.techId = ''; clean.techIdFor = ''; employeeStatus = ''; myROs = []; }
      const techChanged = (clean.techId && clean.techId !== settings.techId) || empChanged || storeChanged;
      settings = { ...settings, ...clean };
      writeJson('settings.json', settings);
      if (sync) {
        if (settings.syncEnabled && !syncWas) { syncFlushed = false; beginSync(); } // turned on: ask first if the server has a copy
        else if (!settings.syncEnabled && syncWas) { sync.stop(); Object.assign(syncStatus, { state: 'off', message: '' }); }
        else if (settings.syncEnabled && (empChanged || storeChanged)) sync.syncNow(); // new person: their own data
      }
      if (typeof next.profileName === 'string' && next.profileName.trim()) {
        profile.name = next.profileName.trim().slice(0, 40);
        saveProfiles();
      }
      if (techChanged) { xtimeName = ''; saveWorkspaces(); } // re-read the name for the new tech ID
      updateTitle();
      if (xtimeChanged) xtimeView.webContents.loadURL(settings.xtimeUrl);
      startPolling();
      pollXtime();
      pushAll();
    },
    // Restart the whole app (reopens the same windows; the shift clock keeps running)
    'restart-app': async () => {
      quitting = true;
      saveProfiles();
      await closeOutAll();
      app.relaunch();
      app.quit();
    },
    // Sync across computers
    'sync-now': () => { if (sync) sync.syncNow(); },
    'sync-choice': (choice) => chooseSync(['restore', 'overwrite', 'cancel'].includes(choice) ? choice : 'cancel'),
    // Updates
    'check-update': () => { if (updater) updater.checkForUpdates().catch(() => {}); },
    'set-beta-updates': (on) => setBetaUpdates(on),
    'open-update-log': () => { if (update.logFile) shell.openPath(update.logFile).catch(() => {}); },
    'install-update': () => installUpdate(),
    // Windows / profiles
    'open-profile': (id) => openProfileWindow(String(id)),
    'new-profile': (name) => {
      const clean = String(name || '').trim().slice(0, 40);
      if (!clean) return;
      const id = `p${Date.now().toString(36)}`;
      profiles.push({ id, name: clean });
      saveProfiles();
      openProfileWindow(id);
    },
  };

  // ---------- window ----------
  function open() {
    loadWorkspaces();
    tracker.prune(60, [...workspaces.keys()]);
    // Sync: "forgot to clock out" waits for the first sync (at most 15 s when offline)
    if (syncOn()) {
      let checked = false;
      const check = () => { if (checked) return; checked = true; tracker.startupCheck(); pushState(); };
      sync.start().finally(check);
      setTimeout(check, 15 * 1000);
    }
    // Heartbeat lets a crash/power loss end an open shift at the right time, and refreshes totals.
    heartbeatTimer = setInterval(() => { tracker.heartbeat(); pushState(); }, 60 * 1000);
    prepareSession(profile.id);
    win = new BaseWindow({ width: 1500, height: 950, minWidth: 900, minHeight: 600, title: 'PTS Workbench', autoHideMenuBar: true });
    updateTitle();

    chrome = new WebContentsView({
      webPreferences: { preload: path.join(__dirname, 'preload.js'), sandbox: true, contextIsolation: true, nodeIntegration: false },
    });
    win.contentView.addChildView(chrome);
    chrome.webContents.loadFile(path.join(__dirname, 'ui', 'index.html'));
    chrome.webContents.setWindowOpenHandler(() => ({ action: 'deny' }));
    appUiContents.add(chrome.webContents);
    chrome.webContents.on('before-input-event', (ev, input) => {
      if (isPrintKey(input)) { ev.preventDefault(); printVisible(); }
    });

    // The app's dashboard starts with "Connecting to Xtime…" until the first ROs arrive (or Xtime asks to sign in)
    if (IS_BETA && settings.xtimeDashboard) xtimeSignin = { step: 'starting', since: Date.now(), busy: false, error: '' };
    xtimeView = makeWebView();
    xtimeView.webContents.loadURL(settings.xtimeUrl); // signs in (and shows Xtime) when needed; put away once the app works without it
    if (IS_BETA) startLite();
    let soon = null;
    xtimeView.webContents.on('did-frame-finish-load', () => {
      clearTimeout(soon);
      soon = setTimeout(pollXtime, 2500); // e.g. right after you log in
    });
    xtimeView.webContents.on('did-finish-load', () => { xtimeLoadedAt = Date.now(); });
    // The Xtime page itself failed to load (network blip, expired sign-in step): try again shortly
    xtimeView.webContents.on('did-fail-load', (_e, code, _desc, _url, isMainFrame) => {
      if (!isMainFrame || code === -3) return; // -3 = cancelled by a redirect, which is normal
      xtimeStatus = 'Xtime page failed to load — retrying shortly';
      pushState();
      setTimeout(() => { if (Date.now() - lastRecovery > 2 * 60 * 1000) reloadXtime('Reloading Xtime…'); }, 20 * 1000);
    });
    // Watch which RO page is open in Xtime, so a hidden RO can be unhidden from there.
    watchTimer = setInterval(() => {
      if (IS_BETA) parkCheck();
      // The background page landed somewhere else (e.g. sent to sign in before Xtime was signed in): try again once Inspect works
      if (IS_BETA && !liteReady() && !liteLoading && findInspectFrame() && Date.now() - liteTriedAt > 30 * 1000) { liteTriedAt = Date.now(); startLite(); }
      const ro = viewingROFromXtime();
      if (ro !== xtimeViewingRO) { xtimeViewingRO = ro; pushState(); }
      // Refresh Xtime before its session runs out — only while you're not looking at the Xtime tab
      const due = Date.now() - xtimeLoadedAt > Number(settings.xtimeRefreshMinutes || 90) * 60 * 1000;
      if (due && !xtimeParked && (mode !== 'xtime' || overlay) && findInspectFrame() && Date.now() - lastRecovery > 5 * 60 * 1000) {
        reloadXtime('');
      }
    }, 1500);

    win.on('resize', layout);
    win.on('closed', () => {
      clearInterval(pollTimer);
      clearInterval(signinTimer);
      clearInterval(msgTimer);
      clearInterval(watchTimer);
      clearInterval(heartbeatTimer);
      if (!closingOut) closeOut().finally(() => sync.stop()); // closing just this window
      else sync.stop();
      // Web pages inside the window aren't closed automatically; close them so nothing keeps running
      for (const ws of workspaces.values()) for (const t of ws.tabs) { t.vinToken = -1; try { t.view?.webContents.close(); } catch {} }
      try { xtimeView.webContents.close(); } catch {}
      try { xtimeLite?.webContents.close(); } catch {}
      try { chrome.webContents.close(); } catch {}
      windows.delete(profile.id);
      if (!quitting) { saveProfiles(); pushAll(); } // other windows' profile lists update
      win = null;
    });
    layout();
    startPolling();
  }

  return {
    open,
    closeOut,
    partition: PARTITION,
    getLogin, saveLogin, rememberUsername,
    pushState,
    handlers,
    displayName,
    keepROs: () => [...workspaces.keys()],
    owns: (sender) => chrome && sender === chrome.webContents,
    manualCall, manualSave,
    // Ctrl+B pressed inside one of this window's pages: show or hide the Workbench sidebar
    sidebarKey: (wc) => {
      if (!allContentViews().some((v) => v.webContents === wc) || mode !== 'manual') return false;
      handlers['toggle-sidebar']();
      return true;
    },
    focus: () => { if (win && !win.isDestroyed()) { if (win.isMinimized()) win.restore(); win.focus(); } },
  };
}

// ---------- "+ Manual" copies the current PTS tab ----------
// PTS keeps its state inside its frames, which can't be cloned directly. Instead we record the
// page each frame is showing (and how far it's scrolled), then rebuild that in the new tab
// after its VIN has been entered.
async function snapshotFrames(wc) {
  const out = [];
  const walk = async (frame, p, depth) => {
    if (depth > 6) return;
    const kids = frame.frames || [];
    for (let i = 0; i < kids.length; i++) {
      const f = kids[i];
      const fp = [...p, i];
      let scroll = null;
      try { scroll = await f.executeJavaScript('[window.scrollX, window.scrollY]'); } catch { /* frame gone */ }
      out.push({ path: fp, name: f.name || '', url: f.url || '', scroll });
      await walk(f, fp, depth + 1);
    }
  };
  await walk(wc.mainFrame, [], 1);
  return out;
}

function frameAt(wc, entry) {
  let f = wc.mainFrame;
  for (const i of entry.path) f = f?.frames?.[i];
  if (f && (!entry.name || f.name === entry.name)) return f;
  // Layout differs slightly: fall back to a frame with the same name at the same depth
  if (entry.name) {
    const depth = (fr) => { let d = 0; while (fr.parent) { d++; fr = fr.parent; } return d; };
    return wc.mainFrame.framesInSubtree.find((x) => x.name === entry.name && depth(x) === entry.path.length) || null;
  }
  return null;
}

async function applySnapshot(tab, snap, token) {
  const wc = tab.view?.webContents;
  const alive = () => wc && !wc.isDestroyed() && tab.vinToken === token;
  await sleep(2500); // let PTS finish loading the vehicle first
  const maxDepth = Math.max(0, ...snap.map((e) => e.path.length));
  for (let d = 1; d <= maxDepth && alive(); d++) {
    let navigated = false;
    for (const e of snap.filter((x) => x.path.length === d)) {
      if (!/^https?:/i.test(e.url) || /VehicleMenu/i.test(e.url)) continue; // the vehicle comes from the VIN
      const f = frameAt(wc, e);
      if (!f || f.url === e.url) continue;
      try { await f.executeJavaScript(`location.replace(${JSON.stringify(e.url)})`); navigated = true; } catch { /* skip */ }
    }
    if (navigated) await sleep(2000); // child frames load after their parent
  }
  if (!alive()) return;
  for (const e of snap) {
    if (!e.scroll || (!e.scroll[0] && !e.scroll[1])) continue;
    const f = frameAt(wc, e);
    try { await f?.executeJavaScript(`window.scrollTo(${Number(e.scroll[0]) || 0}, ${Number(e.scroll[1]) || 0})`); } catch { /* skip */ }
  }
}

// =====================================================================================
// Windows
// =====================================================================================
function openProfileWindow(id) {
  if (windows.has(id)) { windows.get(id).focus(); return; }
  const profile = profiles.find((p) => p.id === id);
  if (!profile) return;
  const wb = createWorkbench(profile);
  windows.set(id, wb);
  wb.open();
  saveProfiles();
  pushAll();
}

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
    profiles = Array.isArray(saved?.profiles) && saved.profiles.length ? saved.profiles : [{ id: 'main', name: 'Main' }];
    if (!profiles.some((p) => p.id === 'main')) profiles.unshift({ id: 'main', name: 'Main' });

    initUpdater();

    // Reopen the windows that were open last time (at least the main one)
    const reopen = (saved?.open || []).filter((id) => profiles.some((p) => p.id === id));
    for (const id of reopen.length ? reopen : ['main']) openProfileWindow(id);
  });

  app.on('before-quit', (event) => {
    quitting = true;
    saveProfiles(); // remember which windows were open
    // Closing the app doesn't clock anyone out; each window records when it closed (for "forgot to
    // clock out") and sends unsynced changes first, a few seconds at most.
    if (closingOut) return;
    event.preventDefault();
    closeOutAll().finally(() => app.quit());
  });

  app.on('web-contents-created', (_e, wc) => {
    // Ctrl+P prints the page (the app's own screen handles it itself, to print the page that's showing)
    wc.on('before-input-event', (ev, input) => {
      if (appUiContents.has(wc)) return;
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