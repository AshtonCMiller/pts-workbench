// PTS Workbench — starts the app.
// Updates normally come as "app files" only (app-bundle.asar: all of the app's own code and pages), downloaded into
// %APPDATA%\PTS Workbench\app-updates\ — no installer, so Windows has no new program to check or block.
// This file picks which app files to start: the newest downloaded ones that fit this install, or the ones the app
// was installed with. A full installer is only needed when Electron itself changes ("shell" in package.json).
// It's never replaced by an app-files update, so keep it small and careful.
'use strict';
const { app } = require('electron');
const path = require('path');
const ofs = require('original-fs'); // plain files (Electron's own fs treats .asar files as folders)

const builtIn = require('../package.json');
const SHELL = Number(builtIn.ptsWorkbench?.shell) || 1;
const defaultEntry = path.join(__dirname, 'main.js');

// 0.4.1-beta.57 < 0.4.1 < 0.4.2
function compareVersions(a, b) {
  const parse = (v) => {
    const [core, pre] = String(v || '0').split('-');
    return { nums: core.split('.').map((n) => parseInt(n, 10) || 0), pre: pre ? pre.split('.') : [] };
  };
  const x = parse(a), y = parse(b);
  for (let i = 0; i < 3; i++) if ((x.nums[i] || 0) !== (y.nums[i] || 0)) return (x.nums[i] || 0) - (y.nums[i] || 0);
  if (!x.pre.length || !y.pre.length) return (x.pre.length ? -1 : 0) - (y.pre.length ? -1 : 0);
  for (let i = 0; i < Math.max(x.pre.length, y.pre.length); i++) {
    const p = x.pre[i], q = y.pre[i];
    if (p === undefined) return -1;
    if (q === undefined) return 1;
    const pn = /^\d+$/.test(p), qn = /^\d+$/.test(q);
    if (pn && qn && +p !== +q) return +p - +q;
    if (p !== q) return pn ? -1 : qn ? 1 : p < q ? -1 : 1;
  }
  return 0;
}

const dir = path.join(app.getPath('userData'), 'app-updates');
const curFile = path.join(dir, 'current.json');
const readCur = () => { try { return JSON.parse(ofs.readFileSync(curFile, 'utf8')); } catch { return null; } };
const writeCur = (c) => { try { ofs.mkdirSync(dir, { recursive: true }); ofs.writeFileSync(curFile, JSON.stringify(c, null, 2)); } catch { /* best effort */ } };

let entry = defaultEntry;
let version = builtIn.version;
let root = __dirname; // the folder with this run's pages (src/)
const cur = app.isPackaged && !process.env.PORTABLE_EXECUTABLE_DIR ? readCur() : null;
if (cur && cur.file) {
  const file = path.join(dir, path.basename(cur.file));
  // newer than what's installed — or chosen on purpose for exactly this install (leaving beta goes "back" a version)
  const wanted = compareVersions(cur.version, builtIn.version) > 0 || (cur.downgrade && cur.installed === builtIn.version);
  if (cur.shell === SHELL && wanted && !cur.bad && ofs.existsSync(file)) {
    // Counts starts that never got going: after 2, these app files are set aside and the installed ones used
    if ((cur.tries || 0) >= 2) { cur.bad = true; writeCur(cur); }
    else {
      cur.tries = (cur.tries || 0) + 1;
      writeCur(cur);
      entry = path.join(file, 'src', 'main.js');
      root = path.join(file, 'src');
      version = cur.version;
    }
  }
}

global.ptsBoot = {
  shell: SHELL, builtInVersion: builtIn.version, version, root, dir, compareVersions, readCur, writeCur,
  quick: entry !== defaultEntry,
  // the app calls this once its window is up: these app files work
  ok() {
    const c = readCur();
    if (c && c.version === version && c.tries) { c.tries = 0; writeCur(c); }
  },
};
// Everything that asks the app's version (updates, Settings, feedback) gets the version of the files running
if (version !== builtIn.version) app.getVersion = () => version;

// (When main.js started first and asked us — see its top — requiring it again just returns, and it carries on.)
try {
  require(entry);
} catch (e) {
  if (entry === defaultEntry) throw e;
  // the downloaded files don't start: set them aside and start the installed ones
  console.error('PTS Workbench: downloaded app files failed to start', e);
  const c = readCur();
  if (c) { c.bad = true; c.error = String(e?.message || e).slice(0, 300); writeCur(c); }
  delete app.getVersion; // back to Electron's own
  global.ptsBoot.version = builtIn.version; global.ptsBoot.root = __dirname; global.ptsBoot.quick = false;
  require(defaultEntry);
}
