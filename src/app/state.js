// Shared app state: what every part of the app reads, and the values that change while it runs. Other files use it
// as S (const S = require('./state')): S.PAGES, S.profiles, S.quitting…
//   PAGES       where the app's pages (ui/, manual/) and Xtime scripts (inject/) are read from; a hot update points
//               it at the new app files
//   XTIME_…     the scripts the app runs inside Xtime / Cox sign-in (src/inject/), read from PAGES
//   profiles    the people (profiles) set up on this computer; windows: the open window of each
//   quitting / closingOut, updater, whatsNew…  set by the files that own them (updates.js, main.js)
'use strict';
const fs = require('fs');
const path = require('path');
const { userFile, writeJsonAt } = require('./files');
const SRC = path.join(__dirname, '..'); // the app's src folder

const S = module.exports;

// The app's pages (ui/, manual/) and the scripts it runs in Xtime (inject/) are read from PAGES. A "hot" update
// (one that only changes those) points PAGES at the new app files and reloads the pages — no restart.
S.PAGES = SRC;
S.XTIME_EXTRACT = undefined;
S.XTIME_RO = undefined;
S.XTIME_INSPECT = undefined;
S.XTIME_CHAT = undefined;
S.COX_SIGNIN = undefined;
S.XTIME_DETAILS = undefined;
S.XTIME_VEHICLE = undefined;
S.XTIME_EMPLOYEE = undefined;
function loadInjects() {
  const read = (name) => fs.readFileSync(path.join(S.PAGES, 'inject', name), 'utf8').trim();
  S.XTIME_EXTRACT = read('xtime-extract.js');
  S.XTIME_RO = read('xtime-ro.js');
  S.XTIME_INSPECT = read('xtime-inspect.js');
  S.XTIME_CHAT = read('xtime-chat.js');
  S.COX_SIGNIN = read('cox-signin.js');
  S.XTIME_DETAILS = read('xtime-details.js');
  S.XTIME_VEHICLE = read('xtime-vehicle.js');
  S.XTIME_EMPLOYEE = read('xtime-employee.js');
}
loadInjects();

// ---------- shared state ----------
S.profiles = [];            // [{ id, name }]
const windows = new Map();    // profile id -> workbench (one window per profile)
S.quitting = false;

const saveProfiles = () => writeJsonAt(userFile('profiles.json'), { profiles: S.profiles, open: [...windows.keys()] });
const pushAll = () => { for (const wb of windows.values()) wb.pushState(); };

Object.assign(module.exports, { loadInjects, windows, saveProfiles, pushAll });
