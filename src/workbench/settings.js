// The window's settings and files: its profile's settings.json (with the built-in defaults), and its own files.
'use strict';
const { partitionFor, profileFile, readJsonAt, writeJsonAt } = require('../app/files');
const { DEFAULT_SETTINGS, FIXED_SETTINGS } = require('../app/config');
const { setKeepAwake } = require('../app/keep-awake');

module.exports = function install(my) {
  const PARTITION = partitionFor(my.profile.id);
  my.PARTITION = PARTITION;
  const readJson = (name, fallback) => readJsonAt(profileFile(my.profile.id, name), fallback);
  my.readJson = readJson;
  const writeJson = (name, value) => writeJsonAt(profileFile(my.profile.id, name), value);
  my.writeJson = writeJson;

  const savedSettings = readJson('settings.json', {});
  my.settings = { ...DEFAULT_SETTINGS, ...savedSettings };
  // Addresses that aren't shown in Settings always come from the app itself, so an update can fix them
  for (const k of FIXED_SETTINGS) my.settings[k] = DEFAULT_SETTINGS[k];
  // Before employee numbers, the first window always used this technician ID by default
  if (my.profile.id === 'main' && !('techId' in savedSettings)) my.settings.techId = '594351';
  my.employeeStatus = ''; // result of the last employee-number lookup, shown in Settings
  setKeepAwake(my.profile.id, my.settings.keepAwake !== false);
};
