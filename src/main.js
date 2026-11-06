// PTS Workbench — main process
// Each window runs one profile: its own Xtime/PTS login, settings, open vehicles and hidden ROs.
// The shift clock and the flag sheet are shared by every window, so all flags land in one set of hours.
// Every page is loaded from Ford/Cox as normal, so access is recorded under that window's own login.
const { app, BaseWindow, WebContentsView, ipcMain, session, shell, safeStorage, clipboard, net } = require('electron');
const path = require('path');
const fs = require('fs');

const XTIME_EXTRACT = fs.readFileSync(path.join(__dirname, 'inject', 'xtime-extract.js'), 'utf8').trim();
const XTIME_DETAILS = fs.readFileSync(path.join(__dirname, 'inject', 'xtime-details.js'), 'utf8').trim();
const XTIME_VEHICLE = fs.readFileSync(path.join(__dirname, 'inject', 'xtime-vehicle.js'), 'utf8').trim();
const XTIME_EMPLOYEE = fs.readFileSync(path.join(__dirname, 'inject', 'xtime-employee.js'), 'utf8').trim();
const { Tracker } = require('./tracker');
const { SyncEngine } = require('./sync');
// The sync server's address is set in package.json ("ptsWorkbench": { "syncUrl": … })
const SYNC_URL = (() => { try { return require('../package.json').ptsWorkbench?.syncUrl || ''; } catch { return ''; } })();

const TOP_BAR = 48;
const TAB_STRIP = 38;
const DRAWER = 360;

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
};
const BOOLEAN_SETTINGS = ['autoLogin', 'setupDone', 'syncEnabled'];
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
let tracker;
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
  tracker?.appClosed();    // the shift clock keeps running across the restart
  await flushSync();       // send anything not yet synced first
  updater.quitAndInstall(true, true); // silent install, then start the app again
}

// =====================================================================================
// Sync across computers (opt-in, set in the main window). Follows the main window's employee number.
// =====================================================================================
let sync = null;
let syncFlushed = false;
const syncStatus = { state: 'off', message: '', lastSync: null };

function syncIdentity() {
  const s = readJsonAt(profileFile('main', 'settings.json'), {}) || {};
  return { enabled: !!s.syncEnabled, storeId: s.storeId || DEFAULT_SETTINGS.storeId, employeeNumber: s.employeeNumber || '' };
}

async function syncRequest(method, url, body) {
  // Electron's net.fetch uses the computer's own network/proxy settings
  const res = await net.fetch(url, {
    method,
    headers: { 'content-type': 'application/json' },
    body: body ? JSON.stringify(body) : undefined,
  });
  let json = null;
  try { json = await res.json(); } catch { /* not JSON */ }
  return { status: res.status, body: json };
}

function initSync() {
  sync = new SyncEngine({
    tracker,
    identity: syncIdentity,
    url: SYNC_URL,
    load: () => readJsonAt(userFile('sync.json'), null),
    save: (state) => writeJsonAt(userFile('sync.json'), state),
    request: syncRequest,
    onStatus: (st) => { Object.assign(syncStatus, st); pushAll(); },
  });
  syncStatus.lastSync = sync.state.lastSync || null;
}

async function flushSync() {
  if (syncFlushed || !sync) return;
  syncFlushed = true;
  try { await sync.flush(4000); } catch { /* best effort */ }
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
  // Addresses that aren't shown in Settings always come from the app itself, so an update can fix them
  for (const k of FIXED_SETTINGS) settings[k] = DEFAULT_SETTINGS[k];
  // Before employee numbers, the first window always used this technician ID by default
  if (profile.id === 'main' && !('techId' in savedSettings)) settings.techId = '594351';
  let employeeStatus = ''; // result of the last employee-number lookup, shown in Settings
  if (settings.prodemandUrl === 'https://www1.prodemand.com/') settings.prodemandUrl = DEFAULT_SETTINGS.prodemandUrl; // old wrong default
  const sidebarWidth = () => Math.min(700, Math.max(260, Number(settings.sidebarWidth) || 380));

  let win, chrome, xtimeView;
  let mode = 'xtime';           // 'xtime' | 'manual'
  let drawerOpen = true;
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
        tabs: ws.tabs.map((t) => ({ kind: t.kind, title: t.title })),
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
        .map((t) => ({ id: nextTabId++, kind: t.kind, title: t.title, view: null, status: '' }));
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
      mode, drawerOpen, settings, xtimeStatus, lastPoll, activeRO,
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
      sync: {
        ...syncStatus,
        configured: !!SYNC_URL,
        enabled: profile.id === 'main' ? !!settings.syncEnabled : syncIdentity().enabled,
        isMain: profile.id === 'main',
      },
      app: { version: app.getVersion(), update: { ...update } },
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
      if (mode === 'xtime') {
        xtimeView.setBounds({ x: 0, y: TOP_BAR, width: Math.max(0, width - (drawerOpen ? DRAWER : 0)), height: Math.max(0, height - TOP_BAR) });
        visible = xtimeView;
      } else if (activeRO && workspaces.has(activeRO)) {
        const ws = workspaces.get(activeRO);
        const tab = ws.tabs.find((t) => t.id === ws.activeTab);
        if (tab) {
          ensureView(ws, tab);
          const side = sidebarWidth();
          tab.view.setBounds({ x: side, y: TOP_BAR + TAB_STRIP, width: Math.max(0, width - side), height: Math.max(0, height - TOP_BAR - TAB_STRIP) });
          visible = tab.view;
        }
      }
    }
    for (const v of allContentViews()) v.setVisible(v === visible);
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
            partition: PARTITION, sandbox: true, contextIsolation: true,
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
        partition: PARTITION, sandbox: true, contextIsolation: true, nodeIntegration: false,
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
    tab.view = makeWebView();
    const wc = tab.view.webContents;
    if (FORD_LINKS[tab.kind]) {
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

  function destroyTab(tab) {
    if (!tab.view) return;
    tab.vinToken = -1;
    try { win.contentView.removeChildView(tab.view); } catch {}
    try { tab.view.webContents.close(); } catch {}
    tab.view = null;
  }

  // ---------- Xtime ----------
  function findXtimeDataFrame() {
    const main = xtimeView?.webContents?.mainFrame;
    if (!main) return null;
    return main.framesInSubtree.find((f) => {
      try { return new URL(f.url).hostname === settings.xtimeDataHost; } catch { return false; }
    }) || null;
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
      xtimeStatus = settings.employeeNumber ? (employeeStatus || 'Looking up your employee number…') : 'Enter your employee number in ⚙ Settings';
      pushState();
      return;
    }
    try {
      const res = await frame.executeJavaScript(
        `${XTIME_EXTRACT}(${JSON.stringify(settings.techId)}, ${JSON.stringify(settings.xtimeApiPath)})`, true);
      if (!res?.ok) { xtimeFailed(res?.error || 'Could not read Xtime', !!res?.expired); return; }
      xtimeFailures = 0;
      myROs = res.mine;
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
          if (d?.ok) tracker.updateLines(r.ro, d.lines, r.other ? '' : [r.year, r.make, r.model].filter(Boolean).join(' '));
          else if (!r.other) failed++; // someone else's RO may have closed; don't count it as a problem
        } catch { failed++; }
      }
      if (failed) xtimeStatus += ` · lines unavailable for ${failed}`;
    } catch (e) {
      xtimeFailed('Could not read Xtime (still loading?)', false);
    }
    pushAll(); // lines are shared, so the other window sees them too
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
  const TAB_NAMES = { xtime: 'Xtime', pts: 'PTS', ota: 'OTA Updates', cvd: 'Connected Data', prodemand: 'ProDemand', oasis: 'OASIS' };
  function addTab(ws, kind, { activate = true, front = false } = {}) {
    const n = ws.tabs.filter((t) => t.kind === kind).length;
    const base = TAB_NAMES[kind] || 'Manual';
    const tab = { id: nextTabId++, kind, title: n ? `${base} ${n + 1}` : base, view: null, status: '' };
    if (front) ws.tabs.unshift(tab); else ws.tabs.push(tab);
    if (activate) ws.activeTab = tab.id;
    return tab;
  }

  function openWorkspace({ ro, vin, ymm }) {
    ro = String(ro).trim();
    vin = String(vin).trim().toUpperCase();
    if (!ro || vin.length !== 17) return;
    let ws = workspaces.get(ro);
    if (!ws) {
      // First tab: the RO in Xtime; then PTS for a Ford/Lincoln/Mercury, ProDemand for anything else
      ws = { ro, vin, ymm: ymm || '', tabs: [], activeTab: null };
      workspaces.set(ro, ws);
      addTab(ws, 'xtime', { activate: false });
      addTab(ws, isFordVehicle(vin, ymm) ? 'pts' : 'prodemand');
    } else if (ws.vin !== vin) {
      ws.vin = vin; // VIN corrected in Xtime: re-enter it in open manual tabs
      for (const t of ws.tabs) if (FORD_LINKS[t.kind] && t.view) reopenFordTab(ws, t);
    }
    activeRO = ro;
    mode = 'manual';
    saveWorkspaces();
    layout();
    pushState();
  }

  // Shared (tracker) changes refresh every window; window-only changes refresh just this one.
  const handlers = {
    'ui-ready': () => pushState(),
    'set-mode': (m) => { mode = m === 'manual' ? 'manual' : 'xtime'; layout(); pushState(); },
    'toggle-drawer': () => { drawerOpen = !drawerOpen; layout(); pushState(); },
    'set-overlay': (on) => { overlay = !!on; layout(); },
    // ↻: refresh the list; if Xtime has been failing, start the Xtime page fresh instead
    'refresh-ros': () => {
      if (xtimeFailures > 0 || (!findXtimeDataFrame() && !onLoginPage())) reloadXtime('Reloading Xtime…');
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
      const KINDS = ['pts', 'ota', 'cvd', 'prodemand', 'oasis', 'xtime'];
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
      if (profile.id !== 'main') delete clean.syncEnabled; // sync follows the main window
      const empChanged = clean.employeeNumber !== undefined && clean.employeeNumber !== settings.employeeNumber;
      const storeChanged = clean.storeId !== undefined && clean.storeId !== settings.storeId;
      // New employee number or store: look the technician ID up again (and don't keep showing the old one's ROs)
      if (empChanged || storeChanged) { clean.techId = ''; clean.techIdFor = ''; employeeStatus = ''; myROs = []; }
      const techChanged = (clean.techId && clean.techId !== settings.techId) || empChanged || storeChanged;
      settings = { ...settings, ...clean };
      writeJson('settings.json', settings);
      if (profile.id === 'main' && sync) {
        if (settings.syncEnabled && !syncWas) { sync.rejoin(); sync.start(); }       // turned on: join fresh
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
      tracker?.appClosed();
      await flushSync();
      app.relaunch();
      app.quit();
    },
    // Sync across computers
    'sync-now': () => { if (sync) sync.syncNow(); },
    // Updates
    'check-update': () => { if (updater) updater.checkForUpdates().catch(() => {}); },
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
    prepareSession(profile.id);
    win = new BaseWindow({ width: 1500, height: 950, minWidth: 900, minHeight: 600, title: 'PTS Workbench', autoHideMenuBar: true });
    updateTitle();

    chrome = new WebContentsView({
      webPreferences: { preload: path.join(__dirname, 'preload.js'), sandbox: true, contextIsolation: true, nodeIntegration: false },
    });
    win.contentView.addChildView(chrome);
    chrome.webContents.loadFile(path.join(__dirname, 'ui', 'index.html'));
    chrome.webContents.setWindowOpenHandler(() => ({ action: 'deny' }));

    xtimeView = makeWebView();
    xtimeView.webContents.loadURL(settings.xtimeUrl);
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
      const ro = viewingROFromXtime();
      if (ro !== xtimeViewingRO) { xtimeViewingRO = ro; pushState(); }
      // Refresh Xtime before its session runs out — only while you're not looking at the Xtime tab
      const due = Date.now() - xtimeLoadedAt > Number(settings.xtimeRefreshMinutes || 90) * 60 * 1000;
      if (due && (mode !== 'xtime' || overlay) && findXtimeDataFrame() && Date.now() - lastRecovery > 5 * 60 * 1000) {
        reloadXtime('');
      }
    }, 1500);

    win.on('resize', layout);
    win.on('closed', () => {
      clearInterval(pollTimer);
      clearInterval(watchTimer);
      // Web pages inside the window aren't closed automatically; close them so nothing keeps running
      for (const ws of workspaces.values()) for (const t of ws.tabs) { t.vinToken = -1; try { t.view?.webContents.close(); } catch {} }
      try { xtimeView.webContents.close(); } catch {}
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
    partition: PARTITION,
    getLogin, saveLogin, rememberUsername,
    pushState,
    handlers,
    displayName,
    keepROs: () => [...workspaces.keys()],
    owns: (sender) => chrome && sender === chrome.webContents,
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
    const saved = readJsonAt(userFile('profiles.json'), null);
    profiles = Array.isArray(saved?.profiles) && saved.profiles.length ? saved.profiles : [{ id: 'main', name: 'Main' }];
    if (!profiles.some((p) => p.id === 'main')) profiles.unshift({ id: 'main', name: 'Main' });

    const syncOn = !!SYNC_URL && syncIdentity().enabled;
    tracker = new Tracker({
      load: () => readJsonAt(userFile('tracking.json'), null),
      save: (data) => { writeJsonAt(userFile('tracking.json'), data); sync?.noteLocalChange(); },
      deferStartupCheck: syncOn, // with sync, "forgot to clock out" is checked after hearing from the other computers
    });
    initSync();
    if (syncOn) {
      let checked = false;
      const check = () => { if (checked) return; checked = true; tracker.startupCheck(); pushAll(); };
      sync.start().finally(check);
      setTimeout(check, 15 * 1000); // offline: don't wait forever
    }
    // Keep lines for any RO open in any profile
    const openEverywhere = profiles.flatMap((p) => (readJsonAt(profileFile(p.id, 'workspaces.json'), {})?.list || []).map((w) => w.ro));
    tracker.prune(60, openEverywhere);

    // Heartbeat lets a crash/power loss end an open shift at the right time, and refreshes totals.
    setInterval(() => { tracker.heartbeat(); pushAll(); }, 60 * 1000);

    initUpdater();

    // Reopen the windows that were open last time (at least the main one)
    const reopen = (saved?.open || []).filter((id) => profiles.some((p) => p.id === id));
    for (const id of reopen.length ? reopen : ['main']) openProfileWindow(id);
  });

  app.on('before-quit', (event) => {
    quitting = true;
    saveProfiles(); // remember which windows were open
    // Closing the app doesn't clock you out; it records when, in case you forget to clock out.
    tracker?.appClosed();
    // Send anything not yet synced before closing (a few seconds at most)
    if (!syncFlushed && sync?.keys() && sync.configured()) {
      event.preventDefault();
      flushSync().finally(() => app.quit());
    }
  });

  app.on('web-contents-created', (_e, wc) => {
    wc.on('will-navigate', (ev, url) => { if (!/^https?:|^file:/i.test(url)) { ev.preventDefault(); shell.openExternal(url).catch(() => {}); } });
  });

  app.on('window-all-closed', () => app.quit());
}