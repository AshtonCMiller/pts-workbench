// PTS Workbench — main process
// Each window runs one profile: its own Xtime/PTS login, settings, open vehicles and hidden ROs.
// The shift clock and the flag sheet are shared by every window, so all flags land in one set of hours.
// Every page is loaded from Ford/Cox as normal, so access is recorded under that window's own login.
// Started directly (package.json "main" pointing here rather than at boot.js): let boot.js choose the app files
// first. If it starts downloaded ones, they're the app and this copy stops here.
if (!global.ptsBoot) {
  require('./boot.js');
  if (global.ptsBoot?.quick) return; // eslint-disable-line no-unreachable
}
const { app, BaseWindow, BrowserWindow, WebContentsView, ipcMain, session, shell, safeStorage, clipboard, net, protocol, Notification, powerSaveBlocker, Menu } = require('electron');
const path = require('path');
const fs = require('fs');

// The app's pages (ui/, manual/) and the scripts it runs in Xtime (inject/) are read from PAGES. A "hot" update
// (one that only changes those) points PAGES at the new app files and reloads the pages — no restart.
let PAGES = __dirname;
let XTIME_EXTRACT, XTIME_RO, XTIME_INSPECT, XTIME_CHAT, COX_SIGNIN, XTIME_DETAILS, XTIME_VEHICLE, XTIME_EMPLOYEE;
function loadInjects() {
  const read = (name) => fs.readFileSync(path.join(PAGES, 'inject', name), 'utf8').trim();
  XTIME_EXTRACT = read('xtime-extract.js');
  XTIME_RO = read('xtime-ro.js');
  XTIME_INSPECT = read('xtime-inspect.js');
  XTIME_CHAT = read('xtime-chat.js');
  COX_SIGNIN = read('cox-signin.js');
  XTIME_DETAILS = read('xtime-details.js');
  XTIME_VEHICLE = read('xtime-vehicle.js');
  XTIME_EMPLOYEE = read('xtime-employee.js');
}
loadInjects();
const COX_SIGNIN_HOST = /(^|\.)signin\.coxautoinc\.com$/i;
const { Tracker } = require('./tracker');
const { ManualService, PTS: PTS_ORIGIN, ASSET_HOSTS } = require('./manual');
const { pathToFileURL } = require('url');

// Pictures in the app's own manual viewer come through ptsasset://, fetched with the window's PTS login
protocol.registerSchemesAsPrivileged([{ scheme: 'ptsasset', privileges: { standard: true, secure: true, supportFetchAPI: true, stream: true } }]);
const { SyncEngine } = require('./sync');
// The sync server's address is set in package.json ("ptsWorkbench": { "syncUrl": … })
const SYNC_URL = (() => { try { return require('../package.json').ptsWorkbench?.syncUrl || ''; } catch { return ''; } })();
// Feedback (bug reports / feature ideas) goes to the same server; this employee number can read it all
const FEEDBACK_ADMIN = (() => { try { return String(require('../package.json').ptsWorkbench?.feedbackAdmin || ''); } catch { return ''; } })();

const TOP_BAR = 48;
const TAB_STRIP = 38;
const SUB_STRIP = 34; // the Service Manual's own tab row
const DRAWER = 360;
const CHAT_W = 380;   // the read-only Dealership chat panel on the right

const DEFAULT_SETTINGS = {
  employeeNumber: '',  // your employee number; the Xtime technician ID is looked up from it
  storeId: '3590',
  dealerCode: '',      // Warranty Dealer Code (e.g. the dealership's PA code), asked at setup; used for FSA software validation codes
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
  keepAwake: true,         // keep the computer (and screen) from going to sleep while the app is open
  notifications: true,     // Windows pop-ups: RO assigned, line approved/declined, parts in, customer/warranty answered, new Xtime messages
};
const BOOLEAN_SETTINGS = ['autoLogin', 'setupDone', 'syncEnabled', 'xtimeDashboard', 'notifications', 'sidebarHidden', 'keepAwake'];

// Keep awake: while any open window has it on, Windows doesn't put the computer or screen to sleep
// (the same thing a "keep awake" browser extension or a playing video does). Released when the app closes.
const keepAwakeFor = new Set(); // windows (profile IDs) that want it
let keepAwakeId = null;
function setKeepAwake(owner, on) {
  if (on) keepAwakeFor.add(owner); else keepAwakeFor.delete(owner);
  if (keepAwakeFor.size && keepAwakeId === null) keepAwakeId = powerSaveBlocker.start('prevent-display-sleep');
  else if (!keepAwakeFor.size && keepAwakeId !== null) { powerSaveBlocker.stop(keepAwakeId); keepAwakeId = null; }
}
// Not shown in Settings (too technical): always the built-in values, updated with the app
const FIXED_SETTINGS = ['ptsUrl', 'ptsTransportUrl', 'oasisUrl', 'xtimeApiPath', 'xtimeRoUrl', 'xtimeDetailsPath', 'xtimeEmployeesPath'];
// Ford transport link codes: 704 = OASIS landing page (workshop manual), 702 = Over-the-Air update
// dashboard, 701 = Connected Vehicle Data
const FORD_LINKS = { pts: '704', ota: '702', cvd: '701' };
// Warranty Dealer Code: letters and numbers only, e.g. USA12345
const cleanDealerCode = (v) => String(v || '').toUpperCase().replace(/[^A-Z0-9]/g, '').slice(0, 12);
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
// A beta build: version like 0.4.1-beta.12, built from any branch other than main (new features ship there first).
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

// ---------- What's new ----------
// src/whats-new.md lists the changes in plain words ("## heading", then "- item" lines; newest first). It travels with
// each update, so the app can show what changed:
//   • when an update is ready and waiting for a restart (read from the downloaded app files)
//   • once, after an update has installed (the items this computer hasn't been shown yet)
//   • as a notification, when an update switched over without a restart
// Which items were shown is kept in whats-new.json (this computer only).
function parseWhatsNew(md) {
  const groups = [];
  let g = null, item = null;
  for (const raw of String(md || '').split(/\r?\n/)) {
    const line = raw.replace(/\s+$/, '');
    let m;
    if ((m = line.match(/^##\s+(.+)/))) { g = { head: m[1].trim(), items: [] }; groups.push(g); item = null; }
    else if (/^#\s/.test(line) || /^<!--/.test(line.trim())) { item = null; }
    else if ((m = line.match(/^\s*[-*]\s+(.+)/))) {
      if (!g) { g = { head: '', items: [] }; groups.push(g); }
      item = { text: m[1].trim() }; g.items.push(item);
    } else if (item && /^\s+\S/.test(line)) item.text += ` ${line.trim()}`; // an item's text carried onto the next line
    else if (!line.trim()) item = null;
  }
  return groups.filter((x) => x.items.length).map((x) => ({ head: x.head, items: x.items.map((i) => i.text.slice(0, 600)) }));
}
const readWhatsNew = (srcDir) => { try { return parseWhatsNew(fs.readFileSync(path.join(srcDir, 'whats-new.md'), 'utf8')); } catch { return []; } };
const whatsNewKey = (text) => require('crypto').createHash('sha1').update(String(text).trim().toLowerCase()).digest('hex').slice(0, 12);
const whatsNewFile = () => userFile('whats-new.json');
function whatsNewSeen() { const d = readJsonAt(whatsNewFile(), null); return d && Array.isArray(d.seen) ? new Set(d.seen) : null; }
function markWhatsNewSeen(groups) {
  const seen = whatsNewSeen() || new Set();
  for (const g of groups || []) for (const t of g.items) seen.add(whatsNewKey(t));
  writeJsonAt(whatsNewFile(), { seen: [...seen].slice(-2000) });
}
// The items not shown on this computer yet (none saved yet: only the newest group, so a first install isn't flooded)
function unseenWhatsNew(groups) {
  const seen = whatsNewSeen();
  if (!seen) return groups.slice(0, 1);
  return groups.map((g) => ({ head: g.head, items: g.items.filter((t) => !seen.has(whatsNewKey(t))) })).filter((g) => g.items.length);
}
// shown in every window until one of them closes it: { id, reason: 'installed' | 'ready', version, groups }
let whatsNew = null;
let whatsNewReadyFor = '';
function initWhatsNew() {
  const groups = unseenWhatsNew(readWhatsNew(PAGES));
  if (groups.length) whatsNew = { id: `installed-${app.getVersion()}`, reason: 'installed', version: app.getVersion(), groups };
  else if (!whatsNewSeen()) markWhatsNewSeen(readWhatsNew(PAGES)); // nothing to show: start the list
}
// An update downloaded and waiting for a restart: what it brings (quick updates carry their notes; a full installer doesn't)
function whatsNewReady(version, srcDir) {
  const groups = srcDir ? unseenWhatsNew(readWhatsNew(srcDir)) : [];
  update.notes = groups.length ? groups : null;
  if (groups.length && whatsNewReadyFor !== version) {
    whatsNewReadyFor = version;
    whatsNew = { id: `ready-${version}`, reason: 'ready', version, groups };
  }
}

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
  // Downloads are decided here: app files only when they fit this install (quick), the full installer otherwise
  updater.autoDownload = false;
  applyUpdateChannel();
  updater.autoInstallOnAppQuit = true; // a downloaded update also installs whenever you close the app
  updater.on('checking-for-update', () => setUpdate({ state: 'checking', message: '' }));
  // info.version is the newest version GitHub offered, so a stuck release is easy to spot
  updater.on('update-not-available', (info) => setUpdate({ state: 'current', latest: info?.version || '', checkedAt: Date.now(), message: '' }));
  updater.on('update-available', (info) => {
    setUpdate({ state: 'downloading', version: info.version, percent: 0, checkedAt: Date.now(), quick: false });
    quickUpdate(info).then((how) => {
      updater.logger.info(`v${info.version}: ${how}`);
      if (how === 'full') updater.downloadUpdate().catch(() => { /* reported through 'error' */ });
      else if (how === 'later') setUpdate({ state: 'current', message: `v${info.version} is still being published; trying again soon` });
    }, (e) => {
      updater.logger.warn(`quick update of v${info.version} failed: ${e.message || e}`);
      setUpdate({ state: 'error', message: `Couldn't download v${info.version}: ${String(e.message || e).slice(0, 120)}`, checkedAt: Date.now() });
    });
  });
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
  if (update.quick) { app.relaunch(); app.exit(0); return; } // new app files: just start again (boot.js picks them)
  updater.quitAndInstall(true, true); // silent install, then start the app again
}

// ---------- quick updates: the app's own files only (no installer) ----------
// Each release has app-bundle.asar (all of the app's code and pages) and app-bundle.json (its version, the Electron
// "shell" it needs, its checksum, and a checksum per file). When the shell matches this install, only the app files
// are downloaded — into %APPDATA%\PTS Workbench\app-updates — and boot.js starts them next time. If the only changes
// are pages / Xtime scripts, they're switched to right away ("hot"), without restarting.
const BOOT = global.ptsBoot || null;
const ofs = require('original-fs');
// Where releases are: app-update.yml (written next to the app by the installer build), else package.json
const PUBLISH = (() => {
  try {
    const yml = fs.readFileSync(path.join(process.resourcesPath, 'app-update.yml'), 'utf8');
    const get = (k) => (yml.match(new RegExp(`^${k}:\\s*['"]?([^'"\\r\\n]+)`, 'm')) || [])[1]?.trim();
    if (get('owner') && get('repo')) return { owner: get('owner'), repo: get('repo') };
  } catch { /* not installed from the Setup build */ }
  try { return require('../package.json').build?.publish || {}; } catch { return {}; }
})();
const releaseFile = (version, name) => `https://github.com/${PUBLISH.owner}/${PUBLISH.repo}/releases/download/v${encodeURIComponent(version)}/${name}`;
async function fetchJson(url) {
  const res = await net.fetch(url, { cache: 'no-store' });
  if (res.status === 404) return null;
  if (!res.ok) throw new Error(`HTTP ${res.status} for ${url.split('/').slice(-2).join('/')}`);
  return res.json();
}
// The file list (path → checksum) of the app files running now: saved when they were downloaded, or from their release
async function runningManifest() {
  const v = app.getVersion();
  const saved = path.join(BOOT.dir, `app-${v}.json`);
  try { return JSON.parse(ofs.readFileSync(saved, 'utf8')); } catch { /* not saved */ }
  const m = await fetchJson(releaseFile(v, 'app-bundle.json')).catch(() => null);
  if (m) { try { ofs.mkdirSync(BOOT.dir, { recursive: true }); ofs.writeFileSync(saved, JSON.stringify(m)); } catch {} }
  return m;
}
// Only pages and Xtime scripts changed (not the app's main code, preloads, libraries or settings): no restart needed
const HOT_FILE = /^src\/(ui\/|inject\/|manual\/(?!viewer-preload\.js$)|whats-new\.md$)/;
function onlyPagesChanged(from, to) {
  if (!from?.files) return { hot: false, why: `no file list for the version running now (v${app.getVersion()})` };
  if (!to?.files) return { hot: false, why: 'the new version has no file list' };
  const paths = new Set([...Object.keys(from.files), ...Object.keys(to.files)]);
  const changed = [], cold = [];
  for (const p of paths) {
    if (from.files[p] === to.files[p]) continue;
    changed.push(p);
    if (!HOT_FILE.test(p)) cold.push(p);
  }
  return { hot: !cold.length, why: cold.length ? `needs a restart for ${cold.slice(0, 8).join(', ')}${cold.length > 8 ? ` (+${cold.length - 8} more)` : ''}`
    : `pages only: ${changed.slice(0, 8).join(', ') || 'nothing changed'}` };
}
const missingBundle = new Map(); // version → checks that found no app files yet
async function quickUpdate(info) {
  const why = (t) => { updater.logger.info(`v${info.version}: installer needed — ${t}`); return 'full'; };
  if (!BOOT) return why('this copy was started without boot.js (installed before quick updates)');
  if (!PUBLISH.owner || !PUBLISH.repo) return why("couldn't tell which GitHub repository the releases are in");
  const v = info.version;
  // already downloaded (waiting for a restart), or those files didn't start last time
  const cur = BOOT.readCur();
  if (cur?.version === v && cur.shell === BOOT.shell) {
    if (cur.bad) return why(`its app files didn't start before (${cur.error || 'no details'})`);
    if (cur.file && ofs.existsSync(path.join(BOOT.dir, cur.file))) {
      whatsNewReady(v, path.join(BOOT.dir, cur.file, 'src'));
      setUpdate({ state: 'ready', version: v, percent: 100, quick: true });
      return 'ready';
    }
  }
  const m = await fetchJson(releaseFile(v, 'app-bundle.json'));
  if (!m) {
    // still uploading, or a release made before quick updates: keep checking for a while (6 checks ≈ 3 hours)
    // before falling back to the installer
    const n = (missingBundle.get(v) || 0) + 1;
    missingBundle.set(v, n);
    updater.logger.info(`v${v}: no app files (app-bundle.json) on its release yet — check ${n} of 6`);
    return n < 6 ? 'later' : why('its release has no app files');
  }
  if (Number(m.shell) !== BOOT.shell) return why(`it needs shell ${m.shell} (this install is shell ${BOOT.shell}: a new Electron)`);
  // download, checking the size and checksum
  const res = await net.fetch(releaseFile(v, m.file || 'app-bundle.asar'), { cache: 'no-store' });
  if (!res.ok || !res.body) throw new Error(`HTTP ${res.status}`);
  const total = Number(m.size) || Number(res.headers.get('content-length')) || 0;
  const hash = require('crypto').createHash('sha512');
  const parts = [];
  let got = 0;
  const reader = res.body.getReader();
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    parts.push(Buffer.from(value)); hash.update(value); got += value.length;
    if (total) { const pct = Math.round((got / total) * 100); if (pct - update.percent >= 5) setUpdate({ percent: pct, quick: true }); }
  }
  if (m.size && got !== Number(m.size)) throw new Error('download incomplete');
  if (m.sha512 && hash.digest('base64') !== m.sha512) throw new Error("download doesn't match its checksum");
  ofs.mkdirSync(BOOT.dir, { recursive: true });
  const name = `app-${v.replace(/[^0-9A-Za-z.-]/g, '')}.asar`;
  const file = path.join(BOOT.dir, name);
  ofs.writeFileSync(file + '.part', Buffer.concat(parts));
  ofs.renameSync(file + '.part', file);
  try { ofs.writeFileSync(path.join(BOOT.dir, `app-${v}.json`), JSON.stringify(m)); } catch {}
  // What the next start uses (boot.js). "downgrade": leaving beta can go back a version, for exactly this install
  BOOT.writeCur({ version: v, file: name, shell: BOOT.shell, tries: 0, installed: BOOT.builtInVersion,
    downgrade: BOOT.compareVersions(v, BOOT.builtInVersion) <= 0, at: new Date().toISOString() });
  // tidy up: keep the new files and the ones running now
  const keep = new Set([name, `app-${app.getVersion()}.asar`, 'current.json', `app-${v}.json`, `app-${app.getVersion()}.json`]);
  try { for (const f of ofs.readdirSync(BOOT.dir)) if (!keep.has(f)) ofs.rmSync(path.join(BOOT.dir, f), { force: true }); } catch {}

  // Only pages / Xtime scripts changed: switch to them now
  const from = await runningManifest().catch(() => null);
  const diff = onlyPagesChanged(from, m);
  updater.logger.info(`v${app.getVersion()} → v${v}: ${diff.why}`);
  if (diff.hot) {
    PAGES = path.join(file, 'src');
    loadInjects();
    app.getVersion = () => v;
    BOOT.version = v;
    try { updater.currentVersion = new (require('semver').SemVer)(v); } catch { /* the next check may offer it again; harmless */ }
    for (const wb of windows.values()) wb.hotReload?.();
    // no restart: say so in each window's notifications (with what changed), instead of a popup
    const groups = unseenWhatsNew(readWhatsNew(PAGES));
    markWhatsNewSeen(groups);
    for (const wb of windows.values()) wb.noteUpdated?.(v, groups);
    setUpdate({ state: 'current', version: v, quick: false, hot: true, latest: v, checkedAt: Date.now(), message: `Updated to v${v} without restarting`, notes: null });
    return 'hot';
  }
  whatsNewReady(v, path.join(file, 'src'));
  setUpdate({ state: 'ready', version: v, percent: 100, quick: true });
  return 'ready';
}

// Before the app closes: every window records when it closed and sends unsynced changes (a few seconds at most)
let closingOut = false;
async function closeOutAll() {
  closingOut = true;
  await Promise.all([...windows.values()].map((wb) => wb.closeOut().catch(() => {})));
  // write sign-ins to disk now (Chromium otherwise saves them every half minute or so)
  await Promise.all([...windows.values()].map((wb) => session.fromPartition(wb.partition).cookies.flushStore().catch(() => {})));
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
  keepXtimeSignedIn(ses);
}

// ---------- staying signed in to Xtime between launches ----------
// Xtime (Cox) keeps its sign-in in "session" cookies, which are thrown away when the app closes, so it asked
// to sign in on every launch. While the app runs, each of those cookies is saved again with an end date, so
// the next launch picks up where the last one left off. Xtime itself still decides how long a sign-in lasts:
// when it ends one (signing out, or its own time limit), the cookie goes away like it would in a browser.
// Only Cox/Xtime cookies, only in this window's own profile (each person's window has its own sign-in).
const KEEP_HOSTS = /(^|\.)(coxautoinc\.com|xtime\.com)$/i;
const KEEP_DAYS = 14; // pushed forward every time Xtime updates the cookie
const keptSessions = new WeakSet();
function keepXtimeSignedIn(ses) {
  if (keptSessions.has(ses)) return;
  keptSessions.add(ses);
  const keep = async (c) => {
    if (!c || c.session === false || c.expirationDate) return;         // already has an end date
    const host = String(c.domain || '').replace(/^\./, '');
    if (!host || !KEEP_HOSTS.test(host)) return;
    const details = {
      url: `${c.secure ? 'https' : 'http'}://${host}${c.path || '/'}`,
      name: c.name, value: c.value, path: c.path || '/',
      secure: !!c.secure, httpOnly: !!c.httpOnly,
      expirationDate: Math.floor(Date.now() / 1000) + KEEP_DAYS * 24 * 60 * 60,
    };
    if (!c.hostOnly) details.domain = c.domain;                        // host-only cookies stay host-only
    if (c.sameSite && c.sameSite !== 'unspecified') details.sameSite = c.sameSite;
    try { await ses.cookies.set(details); } catch { /* left as a session cookie */ }
  };
  ses.cookies.on('changed', (_e, cookie, cause, removed) => {
    if (removed || cause === 'expired' || cause === 'evicted') return;
    keep(cookie);
  });
  // ones set before this ran (e.g. a second window of the same profile)
  ses.cookies.get({}).then((all) => all.forEach(keep)).catch(() => {});
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
  setKeepAwake(profile.id, settings.keepAwake !== false);

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
    onApplied: () => setTimeout(() => reconcileOpenROs(), 0),
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
  let mode = 'xtime';           // 'xtime' | 'manual' (the Workbench)
  let drawerOpen = true;
  let chatOpen = false;          // the app's Dealership chat panel
  const loggedChatFields = new Set();
  let loggedXtimeFields = false;
  let loggedLineFields = false;
  let overlay = false;          // a dialog is open in the app UI; hide web views (a still picture of the page stays behind it)
  let overlaySeq = 0;
  // ---------- hot updates: the app's own pages reloaded from new app files, without restarting ----------
  const staleViews = new Set(); // app pages still showing old files (the one you're looking at waits until you leave it)
  function reloadAppPage(v) {
    if (v && !v.webContents.isDestroyed() && v.ptsPage) v.webContents.loadFile(path.join(PAGES, 'manual', v.ptsPage));
  }
  function hotReload() {
    overlay = false; // a dialog open in the top bar closes with the reload
    if (chrome && !chrome.webContents.isDestroyed()) chrome.webContents.loadFile(path.join(PAGES, 'ui', 'index.html'));
    for (const { tab } of manualViews.values()) {
      const v = tab.view;
      if (!v || v.webContents.isDestroyed()) continue;
      if (v === visibleView) staleViews.add(v); else reloadAppPage(v);
    }
    layout();
  }
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
      list: [...workspaces.values()].map((ws) => {
        const kept = ws.tabs.filter((t) => t.kind !== 'doc'); // downloaded PDFs aren't kept
        return {
        ro: ws.ro, vin: ws.vin, ymm: ws.ymm,
        tabs: kept.map((t) => ({ kind: t.kind, title: t.title, ...(t.url ? { url: t.url } : {}), ...(t.manualState ? { state: t.manualState } : {}) })),
        activeIndex: Math.max(0, kept.findIndex((t) => t.id === ws.activeTab)),
        svcIndex: kept.findIndex((t) => t.id === ws.svcTab),
        layout: 6,
        };
      }),
    });
    syncOpenList();
  }

  // ---------- Feedback: bug reports & feature ideas ----------
  // Sent to the sync server's /v1/reports. A report carries the person's name, what they wrote, the app
  // version and which screen they were on: never VINs, RO numbers or customer details. Each computer has a
  // random reporter ID so people can see their own reports' status. The admin (FEEDBACK_ADMIN) reads them
  // all with a password (the server's ADMIN_KEY), kept on this computer encrypted.
  let feedbackNew = 0;
  const isFeedbackAdmin = () => !!FEEDBACK_ADMIN && String(settings.employeeNumber || '').trim() === FEEDBACK_ADMIN;
  function reporterId() {
    const f = profileFile(profile.id, 'reporter.json');
    let r = readJsonAt(f, null);
    if (!/^[a-f0-9]{32}$/.test(r?.id || '')) { r = { id: require('crypto').randomBytes(16).toString('hex') }; writeJsonAt(f, r); }
    return r.id;
  }
  function adminKey() {
    const rec = readJson('feedback-key.json', null);
    if (!rec?.k) return '';
    try { return canEncrypt() ? safeStorage.decryptString(Buffer.from(rec.k, 'base64')) : ''; } catch { return ''; }
  }
  function setAdminKey(key) {
    if (!key) { writeJson('feedback-key.json', {}); return; }
    if (!canEncrypt()) return;
    writeJson('feedback-key.json', { k: safeStorage.encryptString(key).toString('base64') });
  }
  function screenName() {
    if (mode !== 'manual') return 'Xtime';
    const ws = workspaces.get(activeRO);
    const t = ws && currentTab(ws);
    if (!t) return 'Workbench';
    return ['Workbench', inService(t.kind) ? 'Service Manual' : '', TAB_NAMES[t.kind] || t.kind].filter(Boolean).join(' › ');
  }
  const sendFeedback = (data) => { if (chrome && !chrome.webContents.isDestroyed()) chrome.webContents.send('feedback', data); };
  async function feedbackCall(method, pathPart, body, admin = false) {
    const headers = { 'content-type': 'application/json' };
    if (admin) headers.authorization = `Bearer ${adminKey()}`;
    const res = await net.fetch(`${SYNC_URL.replace(/\/+$/, '')}${pathPart}`, { method, headers, body: body ? JSON.stringify(body) : undefined });
    let json = null;
    try { json = await res.json(); } catch { /* not JSON */ }
    return { status: res.status, body: json || {} };
  }
  async function loadAllFeedback() {
    if (!isFeedbackAdmin()) return;
    if (!adminKey()) { sendFeedback({ kind: 'all', needsKey: true }); return; }
    try {
      const r = await feedbackCall('GET', '/v1/reports', null, true);
      if (r.status === 401) { sendFeedback({ kind: 'all', needsKey: true, message: 'That password was wrong.' }); return; }
      if (r.status === 503) { sendFeedback({ kind: 'all', error: 'The server has no admin password set up yet (REPORTS_ADMIN_KEY).' }); return; }
      if (r.status !== 200) throw new Error(r.body.error || `server said ${r.status}`);
      const list = r.body.reports || [];
      feedbackNew = list.filter((x) => x.status === 'new').length;
      sendFeedback({ kind: 'all', list });
      pushState();
    } catch (e) { sendFeedback({ kind: 'all', error: `Couldn't load feedback: ${e.message || e}` }); }
  }
  // The admin's badge: how many new reports (checked at start and every 15 minutes)
  async function countNewFeedback() {
    if (!isFeedbackAdmin() || !adminKey() || !SYNC_URL) return;
    try {
      const r = await feedbackCall('GET', '/v1/reports', null, true);
      if (r.status === 200) { const n = (r.body.reports || []).filter((x) => x.status === 'new').length; if (n !== feedbackNew) { feedbackNew = n; pushState(); } }
    } catch { /* try again later */ }
  }
  const feedbackHandlers = {
    'feedback-send': async (p) => {
      const kind = p?.kind === 'feature' ? 'feature' : 'bug';
      const title = String(p?.title || '').trim().slice(0, 140);
      const body = String(p?.body || '').trim().slice(0, 6000);
      if (!title) { sendFeedback({ kind: 'sent', ok: false, message: 'Give it a short title first.' }); return; }
      if (!SYNC_URL) { sendFeedback({ kind: 'sent', ok: false, message: 'This version has no server to send feedback to.' }); return; }
      try {
        const r = await feedbackCall('POST', '/v1/reports', {
          kind, title, body, reporter: reporterId(), name: displayName(), version: app.getVersion(),
          platform: `${process.platform} ${require('os').release()}`, screen: String(p?.screen || screenName()).slice(0, 120),
        });
        if (r.status === 429) throw new Error("you've sent a lot in the last hour; try again later");
        if (r.status !== 200) throw new Error(r.body.error || `server said ${r.status}`);
        sendFeedback({ kind: 'sent', ok: true });
        feedbackHandlers['feedback-mine']();
        if (isFeedbackAdmin()) countNewFeedback();
      } catch (e) { sendFeedback({ kind: 'sent', ok: false, message: `Couldn't send it: ${e.message || e}` }); }
    },
    'feedback-mine': async () => {
      if (!SYNC_URL) return;
      try {
        const r = await feedbackCall('GET', `/v1/reports/mine?reporter=${reporterId()}`);
        if (r.status !== 200) throw new Error(r.body.error || `server said ${r.status}`);
        sendFeedback({ kind: 'mine', list: r.body.reports || [] });
      } catch (e) { sendFeedback({ kind: 'mine', error: `Couldn't load your reports: ${e.message || e}` }); }
    },
    'feedback-all': () => loadAllFeedback(),
    'feedback-key': async (key) => {
      if (!isFeedbackAdmin()) return;
      key = String(key || '').trim();
      if (!canEncrypt()) { sendFeedback({ kind: 'all', error: "This computer can't store the password safely." }); return; }
      setAdminKey(key);
      loadAllFeedback();
    },
    'feedback-update': async (p) => {
      if (!isFeedbackAdmin()) return;
      const id = Number(p?.id);
      if (!Number.isInteger(id)) return;
      const body = {};
      if (p.status !== undefined) body.status = String(p.status);
      if (p.reply !== undefined) body.reply = String(p.reply).slice(0, 3000);
      try {
        const r = await feedbackCall('PATCH', `/v1/reports/${id}`, body, true);
        if (r.status !== 200) throw new Error(r.body.error || `server said ${r.status}`);
        notice('Saved');
      } catch (e) { notice(`Couldn't save: ${e.message || e}`); }
      loadAllFeedback();
    },
    'feedback-delete': async (id) => {
      if (!isFeedbackAdmin() || !Number.isInteger(Number(id))) return;
      try {
        const r = await feedbackCall('DELETE', `/v1/reports/${Number(id)}`, null, true);
        if (r.status !== 200) throw new Error(r.body.error || `server said ${r.status}`);
      } catch (e) { notice(`Couldn't delete: ${e.message || e}`); }
      loadAllFeedback();
    },
  };
  setTimeout(countNewFeedback, 20 * 1000);
  setInterval(countNewFeedback, 15 * 60 * 1000);

  // ---------- open ROs, synced between computers (RO number + year/make/model only) ----------
  // The tracker keeps the list of open ROs so sync carries it; the VIN never syncs, so a computer
  // that gets an RO from elsewhere finds the VIN itself (this computer's records, your Xtime list,
  // or an Xtime lookup). ROs it can't open yet wait here and are retried after each Xtime refresh.
  const syncWaiting = new Map(); // ro → ymm
  const syncOpening = new Set();
  function syncOpenList() {
    let t;
    try { t = tracker; } catch { return; } // not created yet
    if (!t) return;
    const next = {};
    for (const ws of workspaces.values()) next[ws.ro] = { ymm: ws.ymm || '' };
    for (const [ro, ymm] of syncWaiting) if (!next[ro]) next[ro] = { ymm };
    if (JSON.stringify(next) !== JSON.stringify(t.data.openROs || {})) { t.data.openROs = next; t.save(); }
  }
  function reconcileOpenROs() {
    const want = tracker.data.openROs || {};
    let changed = false;
    // closed on another computer
    let keep = false;
    for (const ro of [...workspaces.keys()]) {
      if (ro in want) continue;
      if (tracker.data.openClosedBy?.[ro] === 'app') { handlers['close-ro'](ro); changed = true; }
      else keep = true; // dropped by an older app version (or never known there): keep it open, and say so again
    }
    if (keep) syncOpenList();
    for (const ro of [...syncWaiting.keys()]) if (!(ro in want)) syncWaiting.delete(ro);
    // opened on another computer
    for (const [ro, w] of Object.entries(want)) {
      if (workspaces.has(ro) || syncOpening.has(ro)) continue;
      syncWaiting.set(ro, w?.ymm || '');
      openSyncedRO(ro, w?.ymm || '');
      changed = true;
    }
    if (changed) pushState();
  }
  async function openSyncedRO(ro, ymm) {
    const mine = myROs.find((r) => r.ro === ro);
    let vin = tracker.data.ros?.[ro]?.vin || mine?.vin || '';
    if (!ymm && mine) ymm = [mine.year, mine.make, mine.model].filter(Boolean).join(' ');
    if (!vin) {
      const frame = findXtimeDataFrame();
      if (!frame) return; // waits for Xtime
      syncOpening.add(ro);
      try {
        const res = await frame.executeJavaScript(
          `${XTIME_VEHICLE}(${JSON.stringify(ro)}, ${JSON.stringify(settings.xtimeApiPath)}, ${JSON.stringify(settings.xtimeDetailsPath)})`, true);
        if (res?.ok && res.vin) { vin = res.vin; ymm = ymm || res.ymm || ''; }
      } catch { /* tried again after the next Xtime refresh */ } finally { syncOpening.delete(ro); }
    }
    if (!vin || !(ro in (tracker.data.openROs || {})) || workspaces.has(ro)) return;
    syncWaiting.delete(ro);
    openWorkspace({ ro, vin, ymm, quiet: true });
  }
  function retrySyncedROs() { for (const [ro, ymm] of syncWaiting) if (!workspaces.has(ro) && !syncOpening.has(ro)) openSyncedRO(ro, ymm); }

  function loadWorkspaces() {
    const saved = readJson('workspaces.json', null);
    for (const ro of saved?.hidden || []) hiddenROs.add(String(ro));
    if (saved?.xtimeName) xtimeName = String(saved.xtimeName);
    if (!saved?.list) return;
    for (const w of saved.list) {
      const tabs = (w.tabs?.length ? w.tabs : [{ kind: 'pts', title: 'PTS' }])
        .map((t) => ({ id: nextTabId++, kind: t.kind, title: t.title, view: null, status: '', manualState: t.state || null, ...(t.url ? { url: String(t.url) } : {}) }))
        .filter((t) => t.kind !== 'doc' && (t.kind !== 'article' || /^https:/i.test(t.url || ''))); // a Ford article (FSA…) opened from OASIS
      const ws = { ro: w.ro, vin: w.vin, ymm: w.ymm || '', tabs, activeTab: tabs[w.activeIndex || 0]?.id ?? tabs[0].id,
        svcTab: w.svcIndex >= 0 ? tabs[w.svcIndex]?.id ?? null : null };
      if (!(w.layout >= 3)) upgradeTabs(ws, ws.activeTab); // saved before Xtime | PTS | Service Manual
      else if (w.layout < 4 && serviceTab(ws) && !ws.tabs.some((t) => t.kind === 'tsb')) { // saved before the TSBs tab
        const t = addTab(ws, 'tsb', { activate: false });
        ws.tabs.splice(ws.tabs.indexOf(t), 1);
        const diag = ws.tabs.findIndex((x) => x.kind === 'diag');
        ws.tabs.splice(diag >= 0 ? diag + 1 : ws.tabs.length, 0, t);
      }
      if (w.layout >= 3 && w.layout < 5 && serviceTab(ws)) { // saved before Wiring Diagrams was in the Service Manual
        const t = ws.tabs.find((x) => x.kind === 'wiring') || addTab(ws, 'wiring', { activate: false });
        if (t.title === 'Wiring') t.title = TAB_NAMES.wiring;
        ws.tabs.splice(ws.tabs.indexOf(t), 1);
        const man = ws.tabs.findIndex((x) => x.kind === 'manual');
        const lab = ws.tabs.findIndex((x) => x.kind === 'labor');
        ws.tabs.splice(man >= 0 ? man + 1 : lab >= 0 ? lab : ws.tabs.length, 0, t);
      }
      if (!(w.layout >= 6) && !ws.tabs.some((x) => x.kind === 'xview')) { // saved before the app's own Xtime RO page
        const t = addTab(ws, 'xview', { activate: false });
        ws.tabs.splice(ws.tabs.indexOf(t), 1);
        const xt = ws.tabs.findIndex((x) => x.kind === 'xtime');
        ws.tabs.splice(xt >= 0 ? xt + 1 : 0, 0, t);
      }
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
      feedback: { on: !!SYNC_URL, admin: isFeedbackAdmin(), newCount: feedbackNew },
      mode, drawerOpen, chatOpen, settings, xtimeStatus, lastPoll, activeRO, xtimeSignin, msgCount, notes,
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
      app: { version: app.getVersion(), update: { ...update }, beta: IS_BETA, betaUpdates: !!updatePrefs.beta, whatsNew },
      profile: { id: profile.id, name: profile.name, xtimeName, displayName: displayName() },
      profiles: profiles.map((p) => ({
        id: p.id, name: p.name, open: windows.has(p.id), current: p.id === profile.id,
        displayName: windows.get(p.id)?.displayName() || p.name,
      })),
      // Each vehicle's top row of tabs, and the Service Manual's own tabs
      workspaces: [...workspaces.values()].map((ws) => {
        const pick = (t) => ({ id: t.id, kind: t.kind, title: t.title, status: t.status, fixed: isFixedTab(ws, t) });
        return {
          ro: ws.ro, vin: ws.vin, ymm: ws.ymm, activeTab: topTab(ws)?.id ?? null, viewTab: currentTab(ws)?.id ?? null, asleep: !!ws.asleep || !ws.tabs.some((t) => t.view), /* not opened since the app started: asleep too */ fetching: ws.fetching || '',
          tabs: ws.tabs.filter((t) => !inService(t.kind)).map(pick),
          sub: serviceTab(ws) ? { active: subTab(ws)?.id ?? null, tabs: ws.tabs.filter((t) => inService(t.kind)).map(pick) } : null,
        };
      }),
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
    touchActive();
    // The vehicle on screen: fetch its tabs' data ahead if that hasn't happened in the last 15 minutes
    // (covers switching to Workbench, the app starting on a vehicle, and waking one up)
    const shown = mode === 'manual' ? workspaces.get(activeRO) : null;
    if (shown && (!shown.aheadAt || Date.now() - shown.aheadAt > 15 * 60 * 1000)) prefetchVehicle(shown);
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
        const tab = currentTab(ws);
        if (tab && tab.kind !== 'service') {
          ensureView(ws, tab);
          const side = settings.sidebarHidden ? 0 : sidebarWidth(); // the Workbench sidebar can be tucked away
          const top = TOP_BAR + TAB_STRIP + (inService(tab.kind) ? SUB_STRIP : 0);
          tab.view.setBounds({ x: side, y: top, width: Math.max(0, width - side - (chatOpen ? CHAT_W : 0)), height: Math.max(0, height - top) });
          visible = tab.view;
        }
      }
    }
    for (const v of allContentViews()) v.setVisible(v === visible);
    visibleView = visible;
    if (findWc && visible?.webContents !== findWc) closeFind(); // another page now: the find bar closes
    // app pages left on the old files by a hot update: reloaded once you've moved off them
    for (const v of staleViews) if (v !== visible) { staleViews.delete(v); reloadAppPage(v); }
  }
  let visibleView = null;

  // ---------- find in page (Ctrl+F): the app's find bar, for whatever page is showing ----------
  // (Wiring Diagrams has its own, which finds labels in the drawing)
  let findWc = null;
  const sendFind = (data) => { if (chrome && !chrome.webContents.isDestroyed()) chrome.webContents.send('find', data); };
  function visibleKind() {
    if (mode !== 'manual' || !activeRO || !workspaces.has(activeRO)) return '';
    return currentTab(workspaces.get(activeRO))?.kind || '';
  }
  function openFind() {
    const wc = visibleView && !visibleView.webContents.isDestroyed() ? visibleView.webContents : null;
    if (!wc) return false;
    if (findWc && findWc !== wc && !findWc.isDestroyed()) findWc.stopFindInPage('clearSelection');
    findWc = wc;
    if (!wc.findHooked) {
      wc.findHooked = true;
      wc.on('found-in-page', (_e, r) => { if (findWc === wc) sendFind({ result: true, active: r.activeMatchOrdinal || 0, matches: r.matches || 0 }); });
    }
    chrome.webContents.focus();
    sendFind({ open: true });
    return true;
  }
  function closeFind({ quiet = false } = {}) {
    const wc = findWc;
    findWc = null;
    if (wc && !wc.isDestroyed()) wc.stopFindInPage('keepSelection');
    if (!quiet) sendFind({ close: true });
    return wc && !wc.isDestroyed() ? wc : null;
  }
  // Ctrl+F in a page: true when the find bar opened (Wiring Diagrams keeps the key for its own find)
  function findKey(wc) {
    if (!visibleView || visibleView.webContents !== wc) return false;
    if (visibleKind() === 'wiring') return false;
    return openFind();
  }
  // Print whatever page is showing (Xtime, or the open tab of a vehicle)
  function printVisible() {
    if (visibleView && !visibleView.webContents.isDestroyed()) printPage(visibleView.webContents);
    else notice('Open a page first, then print it');
  }

  // ---------- web views ----------
  function hardenWebContents(wc) {
    // Popups (e.g. wiring diagrams) open as normal windows that share this window's login.
    wc.setWindowOpenHandler(windowOpenRule);
    wc.on('will-attach-webview', (e) => e.preventDefault());
  }
  function windowOpenRule({ url }) {
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

  // ---------- Ford's PDFs (an FSA's technical instructions, bulletin, owner letter…) ----------
  // Ford's document server refuses ("Access Denied") PDFs opened in the app's own browser tabs, but
  // hands them to the app's PTS connection (the one that loads Workshop Manual pictures). So a PDF link
  // in an FSA page is downloaded that way and shown from this computer, as its own Service Manual tab.
  const isFordPdf = (url) => { try { const u = new URL(url); return u.protocol === 'https:' && ASSET_HOSTS.test(u.hostname) && /\.pdf$/i.test(u.pathname); } catch { return false; } };
  const PDF_NAMES = [[/_TECHX?$/i, 'Technical instructions'], [/_BULL$/i, 'Dealer bulletin'], [/_MSRA$/i, 'Mobile repair assessment'],
    [/_DELIV_REC$/i, 'Pick-up & delivery record'], [/_Reimbursement/i, 'Reimbursement'], [/_Owner_Letter_SPN$/i, 'Owner letter (Spanish)'],
    [/_Owner_Letter/i, 'Owner letter']];
  function pdfTitle(url) {
    const file = decodeURIComponent(new URL(url).pathname.split('/').pop() || 'Document').replace(/\.pdf$/i, '');
    const id = (file.match(/^(\d{2}[A-Z]\d{2})_/i) || [])[1];
    const known = PDF_NAMES.find(([re]) => re.test(file));
    return (known ? `${id ? `${id} ` : ''}${known[1]}` : file.replace(/_/g, ' ')).slice(0, 40);
  }
  function catchFordPdfs(ws, wc) {
    // a link that loads the PDF into the page (Ford's FSA page shows them in its right-hand frame)
    wc.on('will-frame-navigate', (e) => {
      if (!isFordPdf(e.url)) return;
      e.preventDefault();
      // The frame asking can already be gone (Ford's page swaps its PDF frame out after the first document):
      // reading it then throws "Render frame was disposed", so the page's own address is used instead
      let referrer = '';
      try { referrer = e.frame?.parent?.url || e.frame?.url || ''; } catch { /* frame gone */ }
      try { if (!referrer && !wc.isDestroyed()) referrer = wc.getURL(); } catch { /* page gone */ }
      openFordPdf(ws, e.url, referrer).catch((err) => manualService.log(`PDF ${e.url.slice(0, 160)}: ${err.message || err}`));
    });
    // a link that opens the PDF in a new window
    wc.setWindowOpenHandler((details) => {
      if (isFordPdf(details.url)) {
        let referrer = '';
        try { referrer = details.referrer?.url || (!wc.isDestroyed() ? wc.getURL() : ''); } catch { /* page gone */ }
        openFordPdf(ws, details.url, referrer).catch((err) => manualService.log(`PDF ${details.url.slice(0, 160)}: ${err.message || err}`));
        return { action: 'deny' };
      }
      return windowOpenRule(details);
    });
  }
  // Ford's document server lets you in with CONTENT_AUTH / CONTENT_PERMISSIONS cookies on fordservicecontent.com.
  // The app gets them the way PTS's home page does when they're missing or out of date. Only names (never
  // values) are logged.
  async function contentCookies() {
    const all = await manualSes.cookies.get({}).catch(() => []);
    return all.filter((c) => /^CONTENT_(AUTH|PERMISSIONS)$/i.test(c.name) && (!c.expirationDate || c.expirationDate * 1000 > Date.now() + 60 * 1000));
  }
  // PTS's home page carries a one-time signed address on sso.fordservicecontent.com (a script tag that
  // answers akamaiCallback(...)); loading it is what hands out the cookies. CheckCookie only checks them.
  async function ensureContentCookies(force = false) {
    if (!force && (await contentCookies()).some((c) => /AUTH/i.test(c.name))) return true;
    const where = (u) => { try { const x = new URL(u); return `${x.host}${x.pathname}`; } catch { return '?'; } };
    try {
      const home = await manualService.text(`${PTS_ORIGIN}/`, { headers: { Accept: 'text/html,*/*' } });
      const srcs = [...home.matchAll(/<script\b[^>]*\bsrc\s*=\s*["']([^"']+)["']/gi)].map((m) => m[1].replace(/&amp;/g, '&'));
      const sso = srcs.filter((u) => /fordservicecontent\.com/i.test(u) && !/CheckCookie/i.test(u) && /^https:/i.test(u))
        .concat(srcs.filter((u) => /akamaiCallback/i.test(u) && !/CheckCookie/i.test(u)));
      if (!sso.length) {
        manualService.log(`content sign-on: no sign-on address on PTS's home page; its scripts: ${[...new Set(srcs.map(where))].join(', ').slice(0, 900)}`);
        return false;
      }
      for (const u of [...new Set(sso)].slice(0, 3)) {
        const abs = new URL(u, `${PTS_ORIGIN}/`).href;
        const res = await manualSes.fetch(abs, { credentials: 'include', referrer: `${PTS_ORIGIN}/`, headers: { Accept: 'text/javascript, application/javascript, */*' } });
        const body = await res.text();
        manualService.log(`content sign-on ${where(abs)} → ${res.status}: ${body.replace(/\s+/g, ' ').slice(0, 80)}`);
      }
      const have = await contentCookies();
      manualService.log(`   now have: ${have.map((c) => c.name).join(', ') || 'none'}`);
      return have.some((c) => /AUTH/i.test(c.name));
    } catch (e) {
      manualService.log(`content sign-on failed: ${e.message || e}`);
      return false;
    }
  }
  async function openFordPdf(ws, url, referrer) {
    const title = pdfTitle(url);
    const existing = ws.tabs.find((t) => t.kind === 'doc' && t.src === url);
    if (existing) { setActive(ws, existing); layout(); pushState(); return; }
    notice(`Opening ${title}…`);
    try {
      await ensureContentCookies();
      const get = () => manualService.fetch(url, { headers: { Accept: 'application/pdf,*/*' }, ...(referrer && /^https:/i.test(referrer) ? { referrer } : {}) });
      let res;
      try { res = await get(); } catch (e) {
        if (!e.needsLogin) throw e;
        // refused: get fresh cookies from Ford's sign-on and try once more
        if (!(await ensureContentCookies(true))) throw e;
        res = await get();
      }
      const buf = Buffer.from(await res.arrayBuffer());
      if (buf.subarray(0, 5).toString() !== '%PDF-') {
        manualService.log(`   not a PDF (${buf.length} bytes): ${buf.subarray(0, 200).toString().replace(/\s+/g, ' ')}`);
        throw new Error("Ford didn't send the document");
      }
      const dir = profileFile(profile.id, 'docs');
      fs.mkdirSync(dir, { recursive: true });
      // keep the folder small: documents older than a week go
      for (const f of fs.readdirSync(dir)) { try { const p = path.join(dir, f); if (Date.now() - fs.statSync(p).mtimeMs > 7 * 24 * 3600 * 1000) fs.unlinkSync(p); } catch {} }
      const file = path.join(dir, decodeURIComponent(new URL(url).pathname.split('/').pop()).replace(/[^\w.-]+/g, '_') || 'document.pdf');
      fs.writeFileSync(file, buf);
      const t = addTab(ws, 'doc', { activate: false });
      t.src = url;
      t.url = pathToFileURL(file).href;
      t.title = title;
      ws.tabs.splice(ws.tabs.indexOf(t), 1);
      ws.tabs.splice(ws.tabs.map((x) => inService(x.kind)).lastIndexOf(true) + 1, 0, t);
      setActive(ws, t);
      saveWorkspaces(); layout(); pushState();
    } catch (e) {
      manualService.log(`PDF ${url.slice(0, 160)}: ${e.message || e}`);
      // Ford's document server wants its CONTENT_AUTH / CONTENT_PERMISSIONS cookies: does the app have them?
      // (names, domains and expiry only; never the values)
      try {
        const all = await manualSes.cookies.get({});
        const mine = all.filter((c) => /^CONTENT_/i.test(c.name) || /fordservicecontent/i.test(c.domain || ''));
        manualService.log(`   cookies for Ford's document server: ${mine.map((c) => `${c.name} (${c.domain}${c.path || ''}, ${c.expirationDate ? `expires ${new Date(c.expirationDate * 1000).toISOString()}` : 'session'})`).join('; ') || 'none'}`);
      } catch { /* best effort */ }
      notice(e.needsLogin ? `Ford refused ${title} (signed out of PTS, or access denied). Sign in to PTS and try again.` : `Couldn't open ${title}: ${e.message || e}`);
    }
  }

  function ensureView(ws, tab) {
    if (tab.view || tab.kind === 'service') return; // Service Manual is only a row of tabs
    if (ws.asleep) { ws.asleep = false; setTimeout(pushState, 0); } // woken up by being looked at
    if (tab.kind === 'manual' || tab.kind === 'pced') { tab.view = makeManualView(ws, tab); return; } // PC/ED: the same viewer, Ford's PC/ED book
    if (tab.kind === 'oasisview') { tab.view = makeManualView(ws, tab, 'oasis/oasis.html'); return; }
    if (tab.kind === 'diag') { tab.view = makeManualView(ws, tab, 'diagnostics/diag.html'); return; }
    if (tab.kind === 'tsb') { tab.view = makeManualView(ws, tab, 'tsb/tsb.html'); return; }
    if (tab.kind === 'labor') { tab.view = makeManualView(ws, tab, 'labor-times/labor.html'); return; }
    if (tab.kind === 'wiring') { tab.view = makeManualView(ws, tab, 'wiring/wiring.html'); return; }
    if (tab.kind === 'xview') { tab.view = makeManualView(ws, tab, 'xtime-view/xview.html'); return; }
    tab.view = makeWebView();
    const wc = tab.view.webContents;
    if (tab.kind === 'article') { // Ford's own page for an FSA (or other article) opened from OASIS, inside Service Manual
      tab.status = '';
      catchFordPdfs(ws, wc);
      wc.loadURL(tab.url);
    } else if (tab.kind === 'doc') { // a Ford PDF the app downloaded (see openFordPdf)
      tab.status = '';
      wc.loadURL(tab.url);
    } else if (FORD_LINKS[tab.kind]) {
      setupFordTab(ws, tab);
    } else if (tab.kind === 'xtime') {
      tab.status = '';
      // back from sleep: the Xtime page it was on (same Xtime site only)
      const back = tab.resumeUrl && hostOf(tab.resumeUrl) === hostOf(settings.xtimeRoUrl) ? tab.resumeUrl : '';
      tab.resumeUrl = null;
      wc.loadURL(back || settings.xtimeRoUrl.replace('{RO}', encodeURIComponent(ws.ro)));
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
  // Pictures on Ford's document server need its sign-on cookies: one sign-on at a time, at most every 30 s
  let deniedRun = null, deniedAt = 0;
  manualService.onDenied = () => {
    if (deniedRun) return deniedRun;
    if (Date.now() - deniedAt < 30 * 1000) return Promise.resolve();
    deniedAt = Date.now();
    deniedRun = ensureContentCookies(true).finally(() => { deniedRun = null; });
    return deniedRun;
  };
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
    view.ptsPage = page; // to load it again from new app files (hot update)
    view.webContents.loadFile(path.join(PAGES, 'manual', page));
    return view;
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
    const bookKind = tab.kind === 'pced' ? 'pced' : 'workshop'; // which of the vehicle's books this viewer shows
    switch (name) {
      case 'init': {
        const open = tab.pendingOpen || null; // a procedure sent here from the Diagnostics tab before this tab existed
        tab.pendingOpen = null;
        return { vin: ws.vin, ro: ws.ro, ymm: ws.ymm, state: tab.manualState || null, open, book: bookKind };
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
        setActive(ws, dt); mode = 'manual';
        dt.manualState = { ...(dt.manualState || {}), query };
        if (dt.view && !dt.view.webContents.isDestroyed() && !dt.view.webContents.isLoading()) dt.view.webContents.send('manual-open', { query });
        saveWorkspaces(); layout(); pushState();
        return true;
      }
      case 'tsb-list': { // TSBs tab: the vehicle's TSBs / GSBs / SSMs
        const pacode = String(settings.dealerCode || '').replace(/^[A-Z]+/i, '');
        if (!pacode) return { needsDealer: true };
        try { return await manualService.tsbList(ws.vin, pacode); } catch (e) { return fail(e); }
      }
      case 'wiring-toc': // Wiring Diagrams tab: the vehicle's wiring book
        try { return await manualService.wiringToc(ws.vin); } catch (e) { return fail(e); }
      case 'wiring-pages':
        try { return await manualService.wiringPages(ws.vin, String(payload?.cell || '')); } catch (e) { return fail(e); }
      case 'wiring-svg':
        try { return await manualService.wiringSvg(ws.vin, String(payload?.cell || ''), Number(payload?.page) || 1); } catch (e) { return fail(e); }
      // ---- the app's own Xtime RO page (Xtime View tab): read through the signed-in Xtime page, like the dashboard ----
      case 'xview': {
        const frame = findXtimeDataFrame();
        if (!frame) return { error: true, message: 'Xtime is still loading. Try again in a moment.' };
        try {
          const r = await frame.executeJavaScript(`${XTIME_RO}(${JSON.stringify(settings.storeId)}, ${JSON.stringify(ws.ro)}, {"type":"view"})`, true);
          if (!r?.ok) return { error: true, expired: !!r?.expired, message: r?.error || "Couldn't read this RO from Xtime" };
          if (!r.view.advisor || !r.view.technician) manualService.log(`xtime view RO ${ws.ro}: advisor "${r.view.advisor}", technician "${r.view.technician}" (${JSON.stringify(r.view.people || {})})`);
          return { view: r.view, at: Date.now() };
        } catch (e) { return fail(e); }
      }
      case 'xview-done': { // a line's Done box: the same change as Xtime's Done button
        const frame = findXtimeDataFrame();
        const id = payload?.id;
        if (!frame || id == null) return { error: true, message: 'Xtime is still loading' };
        try {
          const r = await frame.executeJavaScript(`${XTIME_RO}(${JSON.stringify(settings.storeId)}, ${JSON.stringify(ws.ro)}, ${JSON.stringify({ type: 'done', id, value: !!payload.value })})`, true);
          return r?.ok ? { ok: true } : { error: true, message: r?.error || "Couldn't update the line in Xtime" };
        } catch (e) { return fail(e); }
      }
      case 'xinspect': { // the RO's inspection form: read it, change an item, or switch forms (as Xtime's page does)
        const frame = findXtimeDataFrame();
        if (!frame) return { error: true, message: 'Xtime is still loading. Try again in a moment.' };
        const q = payload || {};
        const action = q.type === 'save' ? { type: 'save', iiid: q.iiid, color: String(q.color || ''), value: String(q.value ?? '').slice(0, 255) }
          : q.type === 'switch' ? { type: 'switch', formId: q.formId }
          : q.type === 'catalog' ? { type: 'catalog' }
          : q.type === 'addLine' ? { type: 'addLine', line: q.line || {} }
          : q.type === 'deleteLine' ? { type: 'deleteLine', id: q.id }
          : { type: 'get' };
        try {
          const r = await frame.executeJavaScript(`${XTIME_INSPECT}(${JSON.stringify(settings.storeId)}, ${JSON.stringify(ws.ro)}, ` +
            `${JSON.stringify(settings.xtimeDetailsPath || '/RepairOrder/Details/{RO}')}, ${JSON.stringify(action)})`, true);
          if (!r?.ok) { manualService.log(`xtime inspection RO ${ws.ro} ${action.type}: ${r?.error || 'failed'}`); return { error: true, expired: !!r?.expired, message: r?.error || "Couldn't reach Xtime's inspection form" }; }
          if (action.type === 'save' && r.red) manualService.log(`xtime inspection RO ${ws.ro}: red item ${action.iiid}: ${r.red}`);
          return r;
        } catch (e) { return fail(e); }
      }
      case 'open-xtime': { // Xtime's own page for this RO, in its tab
        const t = ws.tabs.find((x) => x.kind === 'xtime') || addTab(ws, 'xtime', { activate: false, front: true });
        setActive(ws, t); mode = 'manual';
        saveWorkspaces(); layout(); pushState();
        return true;
      }
      case 'open-chat': // the chat panel, on this RO's conversation
        if (chrome && !chrome.webContents.isDestroyed()) chrome.webContents.send('chat', { kind: 'open', ro: ws.ro });
        return true;
      case 'wiring-search':
        try { return await manualService.wiringSearch(ws.vin, String(payload?.text || '')); } catch (e) { return fail(e); }
      case 'wiring-locations':
        try { return await manualService.wiringLocations(ws.vin, String(payload?.item || '').replace(/[^A-Za-z0-9_-]/g, '').slice(0, 20)); } catch (e) { return fail(e); }
      case 'wiring-ford': { // Ford's own wiring page (connector faces, the connector list…) as a tab in Service Manual
        try {
          const info = await manualService.wiringInfo(ws.vin);
          const q = payload || {};
          const kind = ['face', 'Connectors', 'LocIndex', 'page', 'TableOfContents'].includes(q.kind) ? q.kind : 'page';
          const url = kind === 'TableOfContents' ? info.tocUrl : manualService.wiringFordUrl(info, ws.vin, kind,
            { cell: String(q.cell || '').replace(/\D/g, ''), page: q.page === '' ? '' : Number(q.page) || 1, item: String(q.item || '').slice(0, 40), title: String(q.title || '').slice(0, 120) });
          return await manualCall(sender, 'open-article', { url, title: String(q.title || 'Wiring').slice(0, 40) });
        } catch (e) { return fail(e); }
      }
      case 'open-wiring': { // Diagnostics: show this wiring page in the vehicle's Wiring Diagrams tab
        const q = payload || {};
        const wiring = { cell: String(q.cell || '').replace(/\D/g, '').slice(0, 4), page: Number(q.page) || 1, item: String(q.item || '').slice(0, 40), title: String(q.title || '').slice(0, 120) };
        if (!wiring.cell) return false;
        const wt = ws.tabs.find((t) => t.kind === 'wiring') || addTab(ws, 'wiring', { activate: false });
        setActive(ws, wt); mode = 'manual';
        if (wt.view && !wt.view.webContents.isDestroyed() && !wt.view.webContents.isLoading()) wt.view.webContents.send('manual-open', { wiring });
        else wt.pendingOpen = { wiring };
        saveWorkspaces(); layout(); pushState();
        return true;
      }
      case 'tsb-article':
        try { return await manualService.tsbArticle(String(payload?.url || '')); } catch (e) { return fail(e); }
      case 'open-tsb': { // Diagnostics: show this TSB / GSB / SSM in the vehicle's TSBs tab
        const q = payload || {};
        const tsb = { id: String(q.id || '').slice(0, 40), type: String(q.type || '').toUpperCase().slice(0, 4), title: String(q.title || '').slice(0, 200), url: String(q.url || '').slice(0, 600) };
        if (!tsb.id) return false;
        const tt = ws.tabs.find((t) => t.kind === 'tsb') || addTab(ws, 'tsb', { activate: false });
        setActive(ws, tt); mode = 'manual';
        if (tt.view && !tt.view.webContents.isDestroyed() && !tt.view.webContents.isLoading()) tt.view.webContents.send('manual-open', { tsb });
        else tt.pendingOpen = { tsb };
        saveWorkspaces(); layout(); pushState();
        return true;
      }
      case 'open-proc': { // Diagnostics tab: show a procedure / pinpoint test in this vehicle's Workshop Manual (or PC/ED) tab
        const q = payload || {};
        const link = { searchNumber: String(q.searchNumber || ''), procUid: String(q.procUid || ''), linkType: 'Procedure',
          title: String(q.title || '').slice(0, 200), ppt: String(q.ppt || ''), section: 'Diagnostics' };
        if (!link.searchNumber && !link.procUid) return false;
        const kind = q.book === 'pced' ? 'pced' : 'manual';
        const mt = ws.tabs.find((t) => t.kind === kind) || (kind === 'pced' ? addPcedTab(ws) : addTab(ws, 'manual', { activate: false }));
        setActive(ws, mt); mode = 'manual';
        if (mt.view && !mt.view.webContents.isDestroyed() && !mt.view.webContents.isLoading()) mt.view.webContents.send('manual-open', link);
        else mt.pendingOpen = link;
        saveWorkspaces(); layout(); pushState();
        return true;
      }
      case 'book':
        try {
          const b = await manualService.book(ws.vin, bookKind);
          return { title: b.title, vehicle: b.vehicle, kind: b.kind };
        } catch (e) { return fail(e); }
      case 'tree':
        try {
          const html = await manualService.tree(ws.vin, bookKind);
          return { html, features: await manualService.vehicleFeatures(ws.vin) }; // to show only what applies to this vehicle
        } catch (e) { return fail(e); }
      case 'proc':
        try {
          const link = payload || {};
          return await manualService.proc(ws.vin, {
            searchNumber: String(link.searchNumber || ''), procUid: String(link.procUid || ''),
            linkType: link.linkType === 'PublicationContent' ? 'PublicationContent' : 'Procedure',
          }, bookKind);
        } catch (e) { return fail(e); }
      case 'oasis':
        try { return await manualService.oasis(ws.vin); } catch (e) { return fail(e); }
      case 'cv':
        try { return await manualService.connectedVehicle(ws.vin); } catch (e) { return fail(e); }
      case 'slts': // Labor Times tab: Ford's SLTS pages for this vehicle
        try {
          const q = payload || {};
          if (q.op === 'start') return await manualService.sltsStart(ws.vin);
          return await manualService.sltsRequest({ url: String(q.url || ''), method: q.method === 'POST' ? 'POST' : 'GET',
            fields: q.fields && typeof q.fields === 'object' ? q.fields : {} });
        } catch (e) { return fail(e); }
      case 'svc-list': // which FSAs need a software validation code, and codes already retrieved
        try {
          const form = await manualService.svcForm(ws.vin);
          return { fsas: form.fsas, jv: form.jv, saved: manualService.svcSaved(form.vin), dealerCode: settings.dealerCode || '' };
        } catch (e) { return fail(e); }
      case 'svc': {
        const fsa = String(payload?.fsa || '').toUpperCase().replace(/[^A-Z0-9]/g, '').slice(0, 12);
        if (!fsa) return { status: 'error', message: 'No FSA number' };
        if (!settings.dealerCode) return { status: 'needs-dealer' };
        try { return await manualService.svc(ws.vin, fsa, settings.dealerCode); } catch (e) { return fail(e); }
      }
      case 'set-dealer-code': {
        const code = cleanDealerCode(payload);
        if (!code) return false;
        settings = { ...settings, dealerCode: code };
        writeJson('settings.json', settings);
        pushState();
        return true;
      }
      case 'copy': try { clipboard.writeText(String(payload || '').slice(0, 500)); } catch {} return true;
      case 'spa':
        try { return await manualService.spaSteps(ws.vin, { file: String(payload?.file || ''), procUid: String(payload?.procUid || '') }, bookKind); } catch (e) { return fail(e); }
      case 'report':
        try { return { ok: await manualService.reportPinpoint(ws.vin, payload || {}, bookKind) }; } catch (e) { return fail(e); }
      case 'signin': await signInToPts(); return true;
      case 'open-log': shell.openPath(profileFile(profile.id, 'manual.log')).catch(() => {}); return true;
      case 'log': manualService.log(`viewer: ${String(payload || '').slice(0, 500)}`); return true;
      case 'open-pts': {
        const t = addTab(ws, 'pts');
        saveWorkspaces(); layout(); pushState();
        return t.id;
      }
      // A Ford article (FSA, notice) from OASIS: its own tab in Service Manual instead of a window
      case 'open-article': {
        let url = String(payload?.url || '');
        try { url = new URL(url, PTS_ORIGIN).href; } catch { return false; }
        if (!/^https:/i.test(url)) return false;
        let t = ws.tabs.find((x) => x.kind === 'article' && x.url === url);
        if (!t) {
          t = addTab(ws, 'article', { activate: false });
          t.url = url;
          t.title = String(payload?.title || '').replace(/\s+/g, ' ').trim().slice(0, 40) || 'Article';
          // next to the other Service Manual tabs (after the last one), not at the end of the top row
          ws.tabs.splice(ws.tabs.indexOf(t), 1);
          const lastSvc = ws.tabs.map((x) => inService(x.kind)).lastIndexOf(true);
          ws.tabs.splice(lastSvc + 1, 0, t);
        }
        setActive(ws, t);
        saveWorkspaces(); layout(); pushState();
        return true;
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
  // Every notification is also kept in the app's notifications menu (the bell), on this computer only:
  // the newest 100 from the last 7 days. New Xtime messages aren't kept there (Chat has its own counter).
  let notes = readJson('notifications.json', []).filter((n) => n && n.id && Date.now() - n.at < 7 * 24 * 3600 * 1000);
  let notesTimer = null;
  const saveNotes = () => { clearTimeout(notesTimer); notesTimer = setTimeout(() => writeJson('notifications.json', notes), 500); };
  function addNote(note) {
    notes = [{ id: `${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`, at: Date.now(), read: false, ...note }, ...notes].slice(0, 100);
    saveNotes(); pushState();
    return notes[0];
  }
  function openNoteRo(ro) {
    try { if (win.isMinimized()) win.restore(); win.show(); win.focus(); } catch {}
    if (ro) {
      const r = myROs.find((x) => x.ro === ro);
      openWorkspace({ ro, vin: r?.vin || workspaces.get(ro)?.vin || '', ymm: r ? [r.year, r.make, r.model].filter(Boolean).join(' ') : '', tab: 'xtime' });
      layout(); pushState();
    } else { mode = 'xtime'; layout(); pushState(); }
  }
  // kind: assigned | approved | declined | parts | customer | warranty | message
  function notifyUser(title, body, ro, kind = '') {
    const note = kind === 'message' ? null : addNote({ title, body, ro: ro || '', kind });
    if (!settings.notifications || !Notification.isSupported()) return;
    try {
      const n = new Notification({ title, body, silent: false });
      n.on('click', () => {
        if (note) { note.read = true; saveNotes(); }
        openNoteRo(ro);
      });
      n.show();
    } catch { /* notifications are optional */ }
  }
  // An update that switched over without a restart: a notification (bell, and Windows' if they're on) saying it's done
  // and that nothing was interrupted; clicking it shows what changed
  function noteUpdated(version, groups) {
    const items = (groups || []).flatMap((g) => g.items);
    const body = items.length
      ? `Nothing you were doing was interrupted. New: ${items.slice(0, 2).map((t) => t.replace(/\*\*|`/g, '').replace(/[.:].*$/, '')).join('; ')}${items.length > 2 ? ` and ${items.length - 2} more` : ''}.`
      : 'Nothing you were doing was interrupted.';
    const note = addNote({ title: `Updated to v${version}, no restart needed`, body, kind: 'update', version, whatsNew: groups?.length ? groups : null });
    notice(`Updated to v${version} without restarting${items.length ? ' · see 🔔 for what\'s new' : ''}`);
    if (!settings.notifications || !Notification.isSupported()) return;
    try {
      const n = new Notification({ title: note.title, body, silent: true });
      n.on('click', () => {
        note.read = true; saveNotes();
        try { if (win.isMinimized()) win.restore(); win.show(); win.focus(); } catch {}
        if (chrome && !chrome.webContents.isDestroyed()) chrome.webContents.send('show-whats-new', { reason: 'hot', version, groups: note.whatsNew || [] });
        pushState();
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
        notifyUser(`RO ${r.ro} assigned to you`, [ymm, r.tag && `Tag ${r.tag}`].filter(Boolean).join(' · ') || 'New RO on your list', r.ro, 'assigned');
      }
    }
    knownMine = now;
  }
  // notify.log: which notifications fired and why (RO numbers and line letters only), to check they work
  const notifyLog = (line) => {
    try {
      const f = userFile('notify.log');
      try { if (fs.statSync(f).size > 256 * 1024) fs.renameSync(f, f + '.old'); } catch {}
      fs.appendFileSync(f, `${new Date().toISOString()} ${line}\n`);
    } catch { /* best effort */ }
  };
  // Lines approved or declined since the last look. Every line counts, including the primary (diag)
  // line, since a tech waits on those too.
  const declinedSeen = new Map();
  function noteApprovals(r, lines) {
    const key = (l) => l.key || l.dms;
    const label = (l) => `${l.dms || ''} ${l.complaint || l.service || ''}`.trim();
    const approved = new Set((lines || []).filter((l) => l.approved && !l.declined).map(key));
    const declined = new Set((lines || []).filter((l) => l.declined).map(key));
    const before = approvedSeen.get(r.ro);
    const beforeDeclined = declinedSeen.get(r.ro);
    approvedSeen.set(r.ro, approved);
    declinedSeen.set(r.ro, declined);
    if (!before) { notifyLog(`RO ${r.ro}: watching ${lines?.length || 0} lines (approved: ${[...approved].join(',') || 'none'})`); return; }
    const fresh = (lines || []).filter((l) => approved.has(key(l)) && !before.has(key(l)));
    const lost = (lines || []).filter((l) => declined.has(key(l)) && !beforeDeclined?.has(key(l)));
    if (fresh.length) {
      notifyLog(`RO ${r.ro}: approved ${fresh.map(key).join(',')}`);
      notifyUser(`RO ${r.ro}: ${fresh.length === 1 ? 'line approved' : `${fresh.length} lines approved`}`,
        `${fresh.map(label).join('; ').slice(0, 140)}\n${partsNote(r)}`, r.ro, 'approved');
    }
    if (lost.length) {
      notifyLog(`RO ${r.ro}: declined ${lost.map(key).join(',')}`);
      notifyUser(`RO ${r.ro}: ${lost.length === 1 ? 'line declined' : `${lost.length} lines declined`}`, lost.map(label).join('; ').slice(0, 160), r.ro, 'declined');
    }
  }
  // Your ROs' waiting flags clearing: parts in, the customer answered, warranty answered
  const flagsSeen = new Map();
  function noteRoChanges(list) {
    for (const r of list) {
      const now = { parts: !!(r.waitingParts || r.dash?.notInStock), customer: !!r.waitingCustomer, warranty: !!r.waitingWarranty };
      const was = flagsSeen.get(r.ro);
      flagsSeen.set(r.ro, now);
      if (!was) continue;
      const ymm = [r.year, r.make, r.model].filter(Boolean).join(' ');
      if (was.parts && !now.parts) { notifyLog(`RO ${r.ro}: parts no longer waiting`); notifyUser(`RO ${r.ro}: parts are in`, [ymm, r.dash?.partsStatus].filter(Boolean).join(' · ') || 'No longer waiting for parts', r.ro, 'parts'); }
      if (was.customer && !now.customer) { notifyLog(`RO ${r.ro}: customer no longer waiting`); notifyUser(`RO ${r.ro}: customer answered`, `${ymm ? `${ymm} · ` : ''}No longer waiting on the customer`, r.ro, 'customer'); }
      if (was.warranty && !now.warranty) { notifyLog(`RO ${r.ro}: warranty no longer waiting`); notifyUser(`RO ${r.ro}: warranty answered`, `${ymm ? `${ymm} · ` : ''}No longer waiting on warranty`, r.ro, 'warranty'); }
    }
  }
  // Xtime's unread-messages counter (top right of Xtime's page)
  let msgCount = null;
  async function readMsgCount() {
    const frame = findXtimeDataFrame();
    if (!frame) return;
    let n = null;
    // Unread Dealership messages, from Xtime's chat (works without Xtime's page)
    if (chatConfig || findInspectFrame()) {
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
    if (msgCount != null && n > msgCount) notifyUser('New Xtime message', n - msgCount === 1 ? `1 new message · ${n} unread` : `${n - msgCount} new messages · ${n} unread`, null, 'message');
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
    if (liteReady()) return xtimeLite.webContents.mainFrame;
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
      if (syncWaiting.size) setTimeout(retrySyncedROs, 0); // ROs opened on another computer, waiting for Xtime
      if (xtimeSignin?.step === 'loading' || xtimeSignin?.step === 'starting') xtimeSignin = null; // the data is here: close the dialog
      noteAssigned(myROs);
      noteRoChanges(myROs);
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
  // A vehicle's tabs: Xtime | PTS | Service Manual (+ any added). "Service Manual" holds its own tabs:
  // OASIS | Diagnostics | Workshop Manual | Labor Times (the app's own pages).
  const SVC_KINDS = new Set(['oasisview', 'diag', 'tsb', 'manual', 'pced', 'wiring', 'labor', 'article', 'doc']); // article: Ford's page for an FSA etc. · doc: one of its PDFs
  const SVC_DEFAULTS = ['oasisview', 'diag', 'tsb', 'manual', 'wiring', 'labor'];
  const inService = (kind) => SVC_KINDS.has(kind);
  // The Service Manual's main tabs (OASIS, Diagnostics, TSBs, Workshop Manual, Wiring Diagrams, Labor Times) can't be
  // closed (nor Service Manual itself, which would close them); extra ones (a second Workshop Manual, an FSA, a TSB…) can
  // PC/ED (only for vehicles that have one) counts as a main tab too once it's there
  const isFixedTab = (ws, t) => t.kind === 'service' || ([...SVC_DEFAULTS, 'pced'].includes(t.kind) && ws.tabs.find((x) => x.kind === t.kind) === t);
  const serviceTab = (ws) => ws.tabs.find((t) => t.kind === 'service') || null;
  // The vehicle's chosen tab in the top row
  function topTab(ws) {
    return ws.tabs.find((t) => t.id === ws.activeTab && !inService(t.kind)) || ws.tabs.find((t) => !inService(t.kind)) || null;
  }
  // The Service Manual's chosen tab
  function subTab(ws) {
    return ws.tabs.find((t) => t.id === ws.svcTab && inService(t.kind)) || ws.tabs.find((t) => inService(t.kind)) || null;
  }
  // The page showing for this vehicle
  function currentTab(ws) {
    const top = topTab(ws);
    return top?.kind === 'service' ? subTab(ws) : top;
  }
  function ensureService(ws) {
    let svc = serviceTab(ws);
    if (svc) return svc;
    svc = { id: nextTabId++, kind: 'service', title: 'Service Manual', view: null, status: '' };
    // after Xtime and PTS
    const after = Math.max(...['xtime', 'pts'].map((k) => ws.tabs.map((t) => t.kind).lastIndexOf(k)));
    ws.tabs.splice(after + 1, 0, svc);
    return svc;
  }
  function setActive(ws, tab) {
    if (inService(tab.kind)) { ws.svcTab = tab.id; ws.activeTab = ensureService(ws).id; }
    else ws.activeTab = tab.id;
  }
  // A new vehicle: Xtime | PTS | Service Manual (OASIS | Diagnostics | Workshop Manual | Labor Times);
  // anything else: Xtime | ProDemand
  function addDefaultTabs(ws) {
    addTab(ws, 'xtime', { activate: false });
    addTab(ws, 'xview', { activate: false }); // the app's own view of the Xtime RO (trial: next to Xtime's page for now)
    if (isFordVehicle(ws.vin, ws.ymm)) {
      addTab(ws, 'pts', { activate: false });
      SVC_DEFAULTS.forEach((k, i) => addTab(ws, k, { activate: i === 0 })); // opens on Service Manual › OASIS
    } else {
      addTab(ws, 'prodemand');
    }
  }
  // Saved before this layout: Xtime and PTS first, the app's pages gathered into Service Manual, the rest after
  function upgradeTabs(ws, wasActive) {
    ws.tabs = ws.tabs.filter((t) => t.kind !== 'service');
    if (!ws.tabs.some((t) => t.kind === 'xtime')) addTab(ws, 'xtime', { activate: false, front: true });
    if (isFordVehicle(ws.vin, ws.ymm)) {
      if (!ws.tabs.some((t) => t.kind === 'pts')) addTab(ws, 'pts', { activate: false });
      for (const k of SVC_DEFAULTS) if (!ws.tabs.some((t) => t.kind === k)) addTab(ws, k, { activate: false });
    }
    const first = ['xtime', 'pts'].map((k) => ws.tabs.find((t) => t.kind === k)).filter(Boolean);
    const svc = [...SVC_DEFAULTS.map((k) => ws.tabs.find((t) => t.kind === k)).filter(Boolean), ...ws.tabs.filter((t) => inService(t.kind) && !SVC_DEFAULTS.includes(t.kind))];
    const rest = ws.tabs.filter((t) => !first.includes(t) && !svc.includes(t));
    const unique = (list) => list.filter((t, i) => list.indexOf(t) === i);
    ws.tabs = unique([...first, ...svc, ...rest]);
    if (svc.length) ensureService(ws);
    const was = ws.tabs.find((t) => t.id === wasActive);
    if (was) setActive(ws, was); else ws.activeTab = topTab(ws)?.id ?? null;
  }

  const TAB_NAMES = { xview: 'Xtime View', tsb: 'TSBs', article: 'Article', doc: 'Document', service: 'Service Manual', labor: 'Labor Times', diag: 'Diagnostics', xtime: 'Xtime', oasisview: 'OASIS', manual: 'Workshop Manual', pced: 'PC/ED', wiring: 'Wiring Diagrams', pts: 'PTS', ota: 'OTA Updates', cvd: 'Connected Data', prodemand: 'ProDemand', oasis: 'OASIS' };
  function addTab(ws, kind, { activate = true, front = false } = {}) {
    const n = ws.tabs.filter((t) => t.kind === kind).length;
    const base = TAB_NAMES[kind] || 'Manual';
    const tab = { id: nextTabId++, kind, title: n ? `${base} ${n + 1}` : base, view: null, status: '' };
    if (front) ws.tabs.unshift(tab); else ws.tabs.push(tab);
    if (inService(kind)) ensureService(ws);
    if (activate) setActive(ws, tab);
    return tab;
  }

  // PC/ED tab: only for vehicles that have a PC/ED manual (mostly older ones), right after Workshop Manual
  function addPcedTab(ws) {
    const t = addTab(ws, 'pced', { activate: false });
    ws.tabs.splice(ws.tabs.indexOf(t), 1);
    const man = ws.tabs.findIndex((x) => x.kind === 'manual');
    const after = man >= 0 ? man : ws.tabs.map((x) => inService(x.kind)).lastIndexOf(true);
    ws.tabs.splice(after + 1, 0, t);
    return t;
  }
  async function checkPced(ws) {
    if (ws.tabs.some((t) => t.kind === 'pced') || !serviceTab(ws) || ws.pcedChecked === ws.vin) return;
    try {
      const has = await manualService.hasPced(ws.vin);
      ws.pcedChecked = ws.vin;
      if (!has || workspaces.get(ws.ro) !== ws || ws.tabs.some((t) => t.kind === 'pced')) return;
      addPcedTab(ws);
      manualService.log(`RO ${ws.ro}: has a PC/ED manual, tab added`);
      saveWorkspaces(); layout(); pushState();
    } catch { /* checked again next time it's opened */ }
  }

  // quiet: opened by sync from another computer, so don't switch what you're looking at
  // ---------- vehicles asleep, and loading ahead ----------
  // A vehicle you haven't looked at for 10 minutes goes to sleep: its pages are closed to free memory
  // (PTS pages keep a snapshot of where they were, Xtime its address, the app's pages their saved
  // state) and come back when you open it again. Its Workshop Manual stays open, so it never loses
  // its place. Sleeping vehicles show greyed out in the list.
  const SLEEP_AFTER = 10 * 60 * 1000;
  function touchActive() {
    const ws = mode === 'manual' ? workspaces.get(activeRO) : null;
    if (ws) ws.lastUsed = Date.now();
  }
  async function sleepWorkspace(ws) {
    const tabs = ws.tabs.filter((t) => t.view && t.kind !== 'manual');
    if (!tabs.length) { ws.asleep = true; return; }
    for (const t of tabs) {
      const wc = t.view.webContents;
      if (wc.isDestroyed()) { t.view = null; continue; }
      try {
        if (FORD_LINKS[t.kind]) t.cloneSnapshot = await snapshotFrames(wc); // the PTS page, scroll and all
        else if (t.kind === 'xtime' && /^https:/i.test(wc.getURL())) t.resumeUrl = wc.getURL();
      } catch { /* it starts fresh instead */ }
      if (workspaces.get(ws.ro) !== ws || (ws.ro === activeRO && mode === 'manual')) return; // looked at meanwhile
      destroyTab(t);
    }
    ws.asleep = true;
    ws.aheadAt = 0; // fetch ahead again when it wakes
    pushState();
  }
  setInterval(() => {
    touchActive();
    const now = Date.now();
    for (const ws of workspaces.values()) {
      if (!ws.lastUsed) ws.lastUsed = now; // just loaded: give it the full 10 minutes
      if (ws.asleep || (ws.ro === activeRO && mode === 'manual') || now - ws.lastUsed < SLEEP_AFTER) continue;
      sleepWorkspace(ws).catch(() => {});
    }
  }, 60 * 1000);

  // When a vehicle is opened or clicked, fetch what its Service Manual tabs show (manual contents, OASIS,
  // connected vehicle data, TSBs, the SVC list, SLTS) in the background, so the tabs open straight away.
  // Tabs that are already loaded are skipped, and so is anything fetched ahead that hasn't been used yet.
  // The vehicle just clicked goes first; one vehicle is fetched at a time.
  let prefetchQueue = [];
  let prefetchBusy = false;
  function prefetchVehicle(ws) {
    if (!ws) return;
    ws.aheadAt = Date.now();
    if (!isFordVehicle(ws.vin, ws.ymm)) { manualService.log(`ahead RO ${ws.ro}: not a Ford/Lincoln, nothing to fetch`); return; }
    prefetchQueue = [ws, ...prefetchQueue.filter((w) => w !== ws)];
    if (ws.fetching !== 'loading') { ws.fetching = 'queued'; pushState(); } // shown next to the RO
    if (!prefetchBusy) runPrefetch();
  }
  async function runPrefetch() {
    prefetchBusy = true;
    while (prefetchQueue.length) {
      const ws = prefetchQueue.shift();
      if (workspaces.get(ws.ro) !== ws) { ws.fetching = ''; continue; } // closed meanwhile
      const loaded = new Set(ws.tabs.filter((t) => t.view).map((t) => t.kind));
      const pacode = String(settings.dealerCode || '').replace(/^[A-Z]+/i, '');
      manualService.log(`ahead RO ${ws.ro}: starting${loaded.size ? ` (already open: ${[...loaded].join(', ')})` : ''}`);
      ws.fetching = 'loading'; pushState();
      try { await manualService.prefetch(ws.vin, { pacode, skip: loaded }); } catch (e) { manualService.log(`ahead RO ${ws.ro} failed: ${e.message || e}`); }
      await checkPced(ws);
      ws.fetching = prefetchQueue.includes(ws) ? 'queued' : ''; pushState();
    }
    prefetchBusy = false;
  }

  function openWorkspace({ ro, vin, ymm, tab, quiet = false }) {
    ro = String(ro).trim();
    vin = String(vin).trim().toUpperCase();
    if (!ro || vin.length !== 17) return;
    let ws = workspaces.get(ro);
    if (!ws) {
      ws = { ro, vin, ymm: ymm || '', tabs: [], activeTab: null, svcTab: null, lastUsed: Date.now() };
      workspaces.set(ro, ws);
      addDefaultTabs(ws);
      setTimeout(() => prefetchVehicle(ws), 300);
    } else if (ws.vin !== vin) {
      ws.vin = vin; // VIN corrected in Xtime: re-enter it in open manual tabs
      for (const t of ws.tabs) if (FORD_LINKS[t.kind] && t.view) reopenFordTab(ws, t);
      for (const t of ws.tabs) {
        if (t.kind === 'manual' || t.kind === 'pced') t.manualState = null; // the open procedures were for the old VIN
        if (t.kind === 'diag') t.manualState = null;
        if (t.kind === 'tsb') t.manualState = null;
        if ((t.kind === 'manual' || t.kind === 'pced' || t.kind === 'wiring' || t.kind === 'oasisview' || t.kind === 'diag' || t.kind === 'tsb') && t.view) destroyTab(t); // reopen for the right vehicle
      }
    }
    else if (!quiet) prefetchVehicle(ws); // already open: fetch what its tabs haven't loaded yet
    if (tab) { // e.g. from the dashboard: show the RO's Xtime page
      const t = ws.tabs.find((x) => x.kind === tab) || (tab === 'xtime' ? addTab(ws, 'xtime', { activate: false }) : null);
      if (t) setActive(ws, t);
    }
    if (!quiet || !activeRO) activeRO = ro;
    if (!quiet) mode = 'manual';
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
    if (!channelId) return;
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
    find: ({ text, next, back } = {}) => { // from the find bar: typing starts a new search, Enter / arrows go to the next match
      if (!findWc || findWc.isDestroyed()) return;
      text = String(text || '').slice(0, 200);
      if (!text) { findWc.stopFindInPage('clearSelection'); sendFind({ result: true, active: 0, matches: 0, empty: true }); return; }
      findWc.findInPage(text, { forward: !back, findNext: !next, matchCase: false });
    },
    'find-close': () => { const wc = closeFind({ quiet: true }); if (wc) wc.focus(); },
    'ui-ready': () => { pushState(); global.ptsBoot?.ok?.(); }, // the app is up: these app files work
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
    'chat-toggle': ({ open } = {}) => { chatOpen = open == null ? !chatOpen : !!open; layout(); pushState(); },
    // your messages from the last 2 weeks, plus everything (about 4 months back) on ROs assigned to you
    'chat-list': async () => chatRun({ type: 'list', days: 14, roDays: 120, ros: [...new Set([...myROs.map((r) => r.ro), ...workspaces.keys()])] }, 'list'),
    'chat-history': async ({ channelId } = {}) => { if (channelId) chatRun({ type: 'history', channelId: String(channelId) }, 'history'); },
    'chat-roster': async () => chatRun({ type: 'roster' }, 'roster'),
    // Sends only what you typed, only when you press Send, to a conversation that already exists
    'chat-send': async ({ channelId, ro, text } = {}) => {
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
      settings = { ...settings, xtimeDashboard: !settings.xtimeDashboard };
      writeJson('settings.json', settings);
      if (!settings.xtimeDashboard) unparkXtime('Opening Xtime…');
      layout(); pushState();
    },
    'toggle-drawer': () => { drawerOpen = !drawerOpen; layout(); pushState(); },
    // A dialog opens / closes in the app's screen. The live page can't sit under it (it would cover the dialog), so a
    // picture of it is put in its place first — the dialog then dims it instead of the page going black.
    'set-overlay': async (on) => {
      const seq = ++overlaySeq;
      if (on && !overlay && visibleView && !visibleView.webContents.isDestroyed()) {
        try {
          const b = visibleView.getBounds();
          const img = await visibleView.webContents.capturePage();
          if (seq !== overlaySeq) return; // closed again meanwhile
          if (!img.isEmpty() && chrome && !chrome.webContents.isDestroyed()) {
            chrome.webContents.send('overlay-shot', { url: `data:image/jpeg;base64,${img.toJPEG(85).toString('base64')}`, x: b.x, y: b.y, width: b.width, height: b.height });
          }
        } catch { /* no picture: the area is just empty */ }
      }
      if (seq !== overlaySeq) return;
      overlay = !!on;
      if (!overlay && chrome && !chrome.webContents.isDestroyed()) chrome.webContents.send('overlay-shot', null);
      layout();
    },
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
      prefetchVehicle(workspaces.get(ro)); // whatever its tabs haven't loaded yet
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
    ...feedbackHandlers,
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
      const src = currentTab(ws);
      let snapshot = null;
      const KINDS = ['service', 'labor', 'diag', 'tsb', 'oasisview', 'manual', 'wiring', 'pts', 'ota', 'cvd', 'prodemand', 'oasis', 'xtime', 'xview'];
      kind = KINDS.includes(kind) ? kind : 'pts';
      if (kind === 'pts' && src?.kind === 'pts' && src.view && !src.view.webContents.isDestroyed()) {
        try { snapshot = await snapshotFrames(src.view.webContents); } catch { snapshot = null; }
      }
      if (kind === 'service') { // Service Manual with its standard tabs (only one per vehicle)
        for (const k of SVC_DEFAULTS) if (!ws.tabs.some((t) => t.kind === k)) addTab(ws, k, { activate: false });
        setActive(ws, subTab(ws) || ws.tabs.find((t) => inService(t.kind)));
        mode = 'manual';
        saveWorkspaces(); layout(); pushState();
        return;
      }
      const tab = addTab(ws, kind);
      mode = 'manual';
      if (snapshot) {
        tab.cloneSnapshot = snapshot;
        tab.title = `${src.title} copy`.slice(0, 40);
      }
      saveWorkspaces(); layout(); pushState();
    },
    'select-tab': ({ ro, id }) => {
      const ws = workspaces.get(ro);
      const t = ws?.tabs.find((x) => x.id === id);
      if (!t) return;
      setActive(ws, t);
      saveWorkspaces(); layout(); pushState();
    },
    'close-tab': ({ ro, id }) => {
      const ws = workspaces.get(ro);
      if (!ws) return;
      const i = ws.tabs.findIndex((t) => t.id === id);
      if (i < 0) return;
      const closing = ws.tabs[i];
      if (isFixedTab(ws, closing)) return; // the Service Manual's main tabs stay
      // Closing Service Manual closes its tabs too; closing its last tab closes Service Manual
      const gone = closing.kind === 'service' ? ws.tabs.filter((t) => t === closing || inService(t.kind)) : [closing];
      if (inService(closing.kind) && ws.tabs.filter((t) => inService(t.kind)).length === 1) gone.push(serviceTab(ws));
      const topBefore = ws.tabs.slice(0, ws.tabs.indexOf(gone.find((t) => t && !inService(t.kind)) || closing)).filter((t) => !inService(t.kind) && !gone.includes(t)).pop();
      for (const t of gone) if (t) destroyTab(t);
      ws.tabs = ws.tabs.filter((t) => !gone.includes(t));
      if (!ws.tabs.length) { handlers['close-ro'](ro); return; }
      if (inService(closing.kind) && ws.svcTab === id) {
        const before = ws.tabs.slice(0, i).filter((t) => inService(t.kind)).pop();
        ws.svcTab = (before || ws.tabs.find((t) => inService(t.kind)))?.id ?? null;
      }
      if (!ws.tabs.some((t) => t.id === ws.activeTab)) ws.activeTab = (topBefore || ws.tabs.find((t) => !inService(t.kind)))?.id ?? null;
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
    // the notifications menu (the bell)
    'notes-read': () => { let changed = false; for (const n of notes) if (!n.read) { n.read = true; changed = true; } if (changed) { saveNotes(); pushState(); } },
    'notes-clear': () => { notes = []; saveNotes(); pushState(); },
    'note-open': ({ id } = {}) => {
      const n = notes.find((x) => x.id === id);
      if (!n) return;
      n.read = true; saveNotes();
      if (n.kind === 'update') { pushState(); return; } // the window shows its What's new itself
      openNoteRo(n.ro);
    },
    // What's new closed: 'installed' ones count as shown; a ready update's ('Later') shows again after the restart
    'whats-new-done': ({ id } = {}) => {
      if (!whatsNew || whatsNew.id !== id) return;
      if (whatsNew.reason === 'installed') markWhatsNewSeen(whatsNew.groups);
      whatsNew = null;
      pushAll();
    },
    'note-remove': ({ id } = {}) => { notes = notes.filter((x) => x.id !== id); saveNotes(); pushState(); },
    // Tabs dragged into a new order, one row at a time (the top row, or the Service Manual's row):
    // ids = that row's tab ids in the new order. They take the places that row's tabs already had.
    'reorder-tabs': ({ ro, ids }) => {
      const ws = workspaces.get(ro);
      if (!ws || !Array.isArray(ids)) return;
      const moving = ids.map((id) => ws.tabs.find((t) => t.id === id));
      if (!moving.length || moving.some((t) => !t) || new Set(moving).size !== moving.length) return;
      const svc = inService(moving[0].kind);
      const row = ws.tabs.filter((t) => inService(t.kind) === svc);
      if (row.length !== moving.length || moving.some((t) => inService(t.kind) !== svc)) return; // must be the whole row
      const spots = ws.tabs.map((t, i) => (row.includes(t) ? i : -1)).filter((i) => i >= 0);
      spots.forEach((spot, i) => { ws.tabs[spot] = moving[i]; });
      saveWorkspaces(); pushState();
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
      if (clean.dealerCode !== undefined) clean.dealerCode = cleanDealerCode(clean.dealerCode);
      const xtimeChanged = clean.xtimeUrl && clean.xtimeUrl !== settings.xtimeUrl;
      const syncWas = !!settings.syncEnabled;
      const empChanged = clean.employeeNumber !== undefined && clean.employeeNumber !== settings.employeeNumber;
      const storeChanged = clean.storeId !== undefined && clean.storeId !== settings.storeId;
      // New employee number or store: look the technician ID up again (and don't keep showing the old one's ROs)
      if (empChanged || storeChanged) { clean.techId = ''; clean.techIdFor = ''; employeeStatus = ''; myROs = []; }
      const techChanged = (clean.techId && clean.techId !== settings.techId) || empChanged || storeChanged;
      settings = { ...settings, ...clean };
      writeJson('settings.json', settings);
      setKeepAwake(profile.id, settings.keepAwake !== false);
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
    syncOpenList(); // what's open here goes into the first sync (and isn't mistaken for closed)
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
    chrome.webContents.loadFile(path.join(PAGES, 'ui', 'index.html'));
    chrome.webContents.setWindowOpenHandler(() => ({ action: 'deny' }));
    appUiContents.add(chrome.webContents);
    chrome.webContents.on('before-input-event', (ev, input) => {
      if (isPrintKey(input)) { ev.preventDefault(); printVisible(); }
      // Ctrl+F with the app's own bar focused: find in the page that's showing
      if (input.type === 'keyDown' && input.control && !input.alt && !input.shift && input.key.toLowerCase() === 'f' && visibleView) {
        ev.preventDefault();
        if (visibleKind() === 'wiring') { // Wiring Diagrams' own find
          const wc = visibleView.webContents;
          wc.focus();
          wc.sendInputEvent({ type: 'keyDown', keyCode: 'F', modifiers: ['control'] });
          wc.sendInputEvent({ type: 'keyUp', keyCode: 'F', modifiers: ['control'] });
        } else openFind();
      }
    });

    // The app's dashboard starts with "Connecting to Xtime…" until the first ROs arrive (or Xtime asks to sign in)
    if (settings.xtimeDashboard) xtimeSignin = { step: 'starting', since: Date.now(), busy: false, error: '' };
    xtimeView = makeWebView();
    xtimeView.webContents.loadURL(settings.xtimeUrl); // signs in (and shows Xtime) when needed; put away once the app works without it
    startLite();
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
      parkCheck();
      // The background page landed somewhere else (e.g. sent to sign in before Xtime was signed in): try again once Inspect works
      if (!liteReady() && !liteLoading && findInspectFrame() && Date.now() - liteTriedAt > 30 * 1000) { liteTriedAt = Date.now(); startLite(); }
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
      setKeepAwake(profile.id, false);
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
    findKey,
    hotReload, noteUpdated,
    findClose: (wc) => { if (!findWc || findWc !== wc) return false; closeFind(); return true; },
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

    initWhatsNew();
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
