// Updates: the app checks GitHub Releases and downloads newer versions in the background. Usually only the app's own
// files (app-bundle.asar, no installer): applied straight away when only pages changed ("hot"), otherwise on the next
// restart. A full installer only when Electron itself changes. Also "What's new" (src/whats-new.md).
'use strict';
const { app, net } = require('electron');
const fs = require('fs');
const path = require('path');
const S = require('./state'); // shared app state (see app/state.js)
const { readJsonAt, userFile, writeJsonAt } = require('./files');
const { loadInjects, pushAll, saveProfiles, windows } = S;

// =====================================================================================
// Updates: the installed app checks GitHub Releases, downloads a newer version in the
// background, and installs it when you press "Restart to update" (or next time you close it).
// =====================================================================================
S.updater = null;
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
  if (!S.updater) return;
  if (updatePrefs.beta) {
    S.updater.allowPrerelease = true;
    S.updater.channel = 'beta';          // beta.yml: every release (beta or regular) has one
  } else if (IS_BETA) {
    S.updater.allowPrerelease = false;
    S.updater.channel = 'latest';        // leaving beta: back to the newest regular version (may be "older")
  } else {
    S.updater.allowPrerelease = false;
    S.updater.allowDowngrade = false;
  }
}
function setBetaUpdates(on) {
  updatePrefs.beta = !!on;
  writeJsonAt(userFile('updates.json'), updatePrefs);
  applyUpdateChannel();
  pushAll();
  if (S.updater) S.updater.checkForUpdates().catch(() => {});
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
S.whatsNew = null;
let whatsNewReadyFor = '';
function initWhatsNew() {
  const groups = unseenWhatsNew(readWhatsNew(S.PAGES));
  if (groups.length) S.whatsNew = { id: `installed-${app.getVersion()}`, reason: 'installed', version: app.getVersion(), groups };
  else if (!whatsNewSeen()) markWhatsNewSeen(readWhatsNew(S.PAGES)); // nothing to show: start the list
}
// An update downloaded and waiting for a restart: what it brings (quick updates carry their notes; a full installer doesn't)
function whatsNewReady(version, srcDir) {
  const groups = srcDir ? unseenWhatsNew(readWhatsNew(srcDir)) : [];
  update.notes = groups.length ? groups : null;
  if (groups.length && whatsNewReadyFor !== version) {
    whatsNewReadyFor = version;
    S.whatsNew = { id: `ready-${version}`, reason: 'ready', version, groups };
  }
}

function initUpdater() {
  if (!app.isPackaged) { Object.assign(update, { state: 'off', message: 'Running from source, so updates are off' }); return; }
  if (process.env.PORTABLE_EXECUTABLE_DIR) {
    Object.assign(update, { state: 'off', message: "The portable version can't update itself. Install with PTS-Workbench-Setup to get updates." });
    return;
  }
  try { S.updater = require('electron-updater').autoUpdater; } catch {
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
  S.updater.logger = { info: log('INFO'), warn: log('WARN'), error: log('ERROR'), debug: () => {} };
  update.logFile = logFile;
  // Downloads are decided here: app files only when they fit this install (quick), the full installer otherwise
  S.updater.autoDownload = false;
  applyUpdateChannel();
  S.updater.autoInstallOnAppQuit = true; // a downloaded update also installs whenever you close the app
  S.updater.on('checking-for-update', () => setUpdate({ state: 'checking', message: '' }));
  // info.version is the newest version GitHub offered, so a stuck release is easy to spot
  S.updater.on('update-not-available', (info) => setUpdate({ state: 'current', latest: info?.version || '', checkedAt: Date.now(), message: '' }));
  S.updater.on('update-available', (info) => {
    setUpdate({ state: 'downloading', version: info.version, percent: 0, checkedAt: Date.now(), quick: false });
    quickUpdate(info).then((how) => {
      S.updater.logger.info(`v${info.version}: ${how}`);
      if (how === 'full') S.updater.downloadUpdate().catch(() => { /* reported through 'error' */ });
      else if (how === 'later') setUpdate({ state: 'current', message: `v${info.version} is still being published; trying again soon` });
    }, (e) => {
      S.updater.logger.warn(`quick update of v${info.version} failed: ${e.message || e}`);
      setUpdate({ state: 'error', message: `Couldn't download v${info.version}: ${String(e.message || e).slice(0, 120)}`, checkedAt: Date.now() });
    });
  });
  S.updater.on('download-progress', (p) => {
    const pct = Math.round(p.percent || 0);
    if (pct - update.percent >= 5 || pct === 100) setUpdate({ state: 'downloading', percent: pct });
  });
  S.updater.on('update-downloaded', (info) => setUpdate({ state: 'ready', version: info.version, percent: 100 }));
  S.updater.on('error', (err) => {
    const text = String(err?.message || err);
    const message = /404|not found/i.test(text) ? 'No release found on GitHub yet (or the repository is private)'
      : /ENOTFOUND|ETIMEDOUT|ECONNRESET|net::/i.test(text) ? "Couldn't reach GitHub"
      : text.split('\n')[0].slice(0, 160);
    setUpdate({ state: 'error', message, checkedAt: Date.now() });
  });
  const check = () => { S.updater.checkForUpdates().catch(() => { /* reported through 'error' */ }); };
  setTimeout(check, 15 * 1000);          // shortly after starting
  setInterval(check, 30 * 60 * 1000);    // and every 30 minutes
}

async function installUpdate() {
  if (!S.updater || update.state !== 'ready') return;
  S.quitting = true;
  saveProfiles();          // reopen the same windows after the update
  await S.closeOutAll();     // record closing time + send anything not yet synced (clocks keep running)
  if (update.quick) { app.relaunch(); app.exit(0); return; } // new app files: just start again (boot.js picks them)
  S.updater.quitAndInstall(true, true); // silent install, then start the app again
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
  try { return require('../../package.json').build?.publish || {}; } catch { return {}; }
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
  const why = (t) => { S.updater.logger.info(`v${info.version}: installer needed — ${t}`); return 'full'; };
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
    S.updater.logger.info(`v${v}: no app files (app-bundle.json) on its release yet — check ${n} of 6`);
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
  S.updater.logger.info(`v${app.getVersion()} → v${v}: ${diff.why}`);
  if (diff.hot) {
    S.PAGES = path.join(file, 'src');
    loadInjects();
    app.getVersion = () => v;
    BOOT.version = v;
    try { S.updater.currentVersion = new (require('semver').SemVer)(v); } catch { /* the next check may offer it again; harmless */ }
    for (const wb of windows.values()) wb.hotReload?.();
    // no restart: say so in each window's notifications (with what changed), instead of a popup
    const groups = unseenWhatsNew(readWhatsNew(S.PAGES));
    markWhatsNewSeen(groups);
    for (const wb of windows.values()) wb.noteUpdated?.(v, groups);
    setUpdate({ state: 'current', version: v, quick: false, hot: true, latest: v, checkedAt: Date.now(), message: `Updated to v${v} without restarting`, notes: null });
    return 'hot';
  }
  whatsNewReady(v, path.join(file, 'src'));
  setUpdate({ state: 'ready', version: v, percent: 100, quick: true });
  return 'ready';
}

module.exports = { update, IS_BETA, updatePrefs, setBetaUpdates, markWhatsNewSeen, initWhatsNew, initUpdater, installUpdate };
