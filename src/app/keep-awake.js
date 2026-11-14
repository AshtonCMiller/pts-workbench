// Keep awake: while any open window has it on (Settings), Windows doesn't put the computer or screen to sleep.
'use strict';
const { powerSaveBlocker } = require('electron');

// Keep awake: while any open window has it on, Windows doesn't put the computer or screen to sleep
// (the same thing a "keep awake" browser extension or a playing video does). Released when the app closes.
const keepAwakeFor = new Set(); // windows (profile IDs) that want it
let keepAwakeId = null;
function setKeepAwake(owner, on) {
  if (on) keepAwakeFor.add(owner); else keepAwakeFor.delete(owner);
  if (keepAwakeFor.size && keepAwakeId === null) keepAwakeId = powerSaveBlocker.start('prevent-display-sleep');
  else if (!keepAwakeFor.size && keepAwakeId !== null) { powerSaveBlocker.stop(keepAwakeId); keepAwakeId = null; }
}

module.exports = { setKeepAwake };
