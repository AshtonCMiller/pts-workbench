// Hours, flags and the shift clock (src/tracker.js, shared by every window so all flags land in one set of hours), and
// sync between computers (src/sync.js): turning it on (restore or overwrite), and sending changes before closing.
'use strict';
const { Tracker } = require('../tracker');
const { SyncEngine } = require('../sync');
const { profileFile, readJsonAt, writeJsonAt } = require('../app/files');
const { SYNC_URL } = require('../app/config');
const { syncRequest } = require('../app/web');
const { pushAll } = require('../app/state');

module.exports = function install(my) {
  Object.assign(my, { beginSync, chooseSync, closeOut }); // used by the window's other parts

  // ---------- this person's hours, flags, clock and sync ----------
  // Each window is one person with their own hours. The first window keeps the original files.
  const trackFile = profileFile(my.profile.id, 'tracking.json');
  const syncStatus = { state: 'off', message: '', lastSync: null };
  my.syncStatus = syncStatus;
  my.syncChoice = null; // turning sync on: { flags, shifts, ros, updatedAt } while asking restore or overwrite
  my.sync = null;
  const syncOn = () => !!SYNC_URL && !!my.settings.syncEnabled;
  my.syncOn = syncOn;
  const tracker = new Tracker({
    load: () => readJsonAt(trackFile, null),
    save: (data) => { writeJsonAt(trackFile, data); my.sync?.noteLocalChange(); },
    deferStartupCheck: syncOn(), // with sync, "forgot to clock out" is checked after hearing from the other computers
  });
  my.tracker = tracker;
  my.sync = new SyncEngine({
    tracker,
    identity: () => ({ enabled: !!my.settings.syncEnabled, storeId: my.settings.storeId, employeeNumber: my.settings.employeeNumber }),
    url: SYNC_URL,
    load: () => readJsonAt(profileFile(my.profile.id, 'sync.json'), null),
    save: (st) => writeJsonAt(profileFile(my.profile.id, 'sync.json'), st),
    request: syncRequest,
    onStatus: (st) => { Object.assign(syncStatus, st); my.pushState(); },
    onApplied: () => setTimeout(() => my.reconcileOpenROs(), 0),
  });
  syncStatus.lastSync = my.sync.state.lastSync || null;
  my.syncFlushed = false;
  async function flushSync() {
    if (my.syncFlushed) return;
    my.syncFlushed = true;
    try { await my.sync.flush(4000); } catch { /* best effort */ }
  }
  // Closing: record when (for "forgot to clock out"), then send anything not yet synced
  // ---------- turning sync on ----------
  // If the server already has a copy for this person, ask: restore this computer from it, or overwrite it?
  async function beginSync() {
    my.sync.paused = true;
    Object.assign(syncStatus, { state: 'syncing', message: 'Checking the sync server…' }); my.pushState();
    let info;
    try { info = await my.sync.peek(); } catch (e) {
      my.sync.paused = false;
      my.settings = { ...my.settings, syncEnabled: false }; my.writeJson('settings.json', my.settings);
      const msg = String(e?.message || e);
      Object.assign(syncStatus, { state: 'error', message: /employee/i.test(msg) ? msg
        : `Sync wasn't turned on: ${/fetch|network|ENOTFOUND|ECONN|ETIMEDOUT|net::/i.test(msg) ? "can't reach the sync server" : msg}. Try again.` });
      my.pushState();
      return;
    }
    if (!info.exists) { my.sync.paused = false; my.sync.rejoin(); my.sync.start(); return; } // nothing there yet: upload this computer's copy
    my.syncChoice = info;
    Object.assign(syncStatus, { state: 'ok', message: 'Waiting for your choice' });
    my.pushState();
  }
  async function chooseSync(choice) {
    if (!my.syncChoice) return;
    my.syncChoice = null;
    if (choice === 'cancel') {
      my.sync.paused = false;
      my.settings = { ...my.settings, syncEnabled: false }; my.writeJson('settings.json', my.settings);
      Object.assign(syncStatus, { state: 'off', message: '' }); my.pushState();
      return;
    }
    my.sync.paused = false;
    Object.assign(syncStatus, { state: 'syncing', message: '' }); my.pushState();
    try {
      if (choice === 'restore') await my.sync.restoreFromServer();
      else await my.sync.overwriteServer();
      my.sync.start();
    } catch (e) {
      Object.assign(syncStatus, { state: 'error', message: String(e?.message || e).slice(0, 120) });
    }
    pushAll();
  }

  async function closeOut() {
    tracker.appClosed();
    await flushSync();
  }
};
