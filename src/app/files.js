// The app's files on this computer (%APPDATA%\PTS Workbench): reading and writing JSON, each profile's own files
// (the first profile, "main", keeps the original names), and each profile's own browser session (its logins).
'use strict';
const path = require('path');
const { app } = require('electron');
const fs = require('fs');

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

module.exports = { userFile, readJsonAt, writeJsonAt, profileFile, partitionFor };
