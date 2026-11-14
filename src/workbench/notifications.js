// Notifications: the 🔔 list and Windows notifications: RO assigned, lines approved / declined, parts in, customer
// waiting, flag changes, unread messages, and updates applied without a restart.
'use strict';
const { Notification } = require('electron');
const fs = require('fs');
const S = require('../app/state'); // shared app state (see app/state.js)
const { userFile } = require('../app/files');

module.exports = function install(my) {
  Object.assign(my, { openNoteRo, noteUpdated, noteAssigned, noteApprovals, noteRoChanges, readMsgCount }); // used by the window's other parts

  // ---------- Xtime ----------
  // ---------- Windows notifications ----------
  // RO assigned to you, a line approved (with what Xtime says about parts), and new Xtime messages.
  // Clicking one opens that RO's Xtime page in the Workbench.
  // Every notification is also kept in the app's notifications menu (the bell), on this computer only:
  // the newest 100 from the last 7 days. New Xtime messages aren't kept there (Chat has its own counter).
  my.notes = my.readJson('notifications.json', []).filter((n) => n && n.id && Date.now() - n.at < 7 * 24 * 3600 * 1000);
  let notesTimer = null;
  const saveNotes = () => { clearTimeout(notesTimer); notesTimer = setTimeout(() => my.writeJson('notifications.json', my.notes), 500); };
  my.saveNotes = saveNotes;
  function addNote(note) {
    my.notes = [{ id: `${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`, at: Date.now(), read: false, ...note }, ...my.notes].slice(0, 100);
    saveNotes(); my.pushState();
    return my.notes[0];
  }
  function openNoteRo(ro) {
    try { if (my.win.isMinimized()) my.win.restore(); my.win.show(); my.win.focus(); } catch {}
    if (ro) {
      const r = my.myROs.find((x) => x.ro === ro);
      my.openWorkspace({ ro, vin: r?.vin || my.workspaces.get(ro)?.vin || '', ymm: r ? [r.year, r.make, r.model].filter(Boolean).join(' ') : '', tab: 'xtime' });
      my.layout(); my.pushState();
    } else { my.mode = 'xtime'; my.layout(); my.pushState(); }
  }
  // kind: assigned | approved | declined | parts | customer | warranty | message
  function notifyUser(title, body, ro, kind = '') {
    const note = kind === 'message' ? null : addNote({ title, body, ro: ro || '', kind });
    if (!my.settings.notifications || !Notification.isSupported()) return;
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
    my.notice(`Updated to v${version} without restarting${items.length ? ' · see 🔔 for what\'s new' : ''}`);
    if (!my.settings.notifications || !Notification.isSupported()) return;
    try {
      const n = new Notification({ title: note.title, body, silent: true });
      n.on('click', () => {
        note.read = true; saveNotes();
        try { if (my.win.isMinimized()) my.win.restore(); my.win.show(); my.win.focus(); } catch {}
        if (my.chrome && !my.chrome.webContents.isDestroyed()) my.chrome.webContents.send('show-whats-new', { reason: 'hot', version, groups: note.whatsNew || [] });
        my.pushState();
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
  my.msgCount = null;
  async function readMsgCount() {
    const frame = my.findXtimeDataFrame();
    if (!frame) return;
    let n = null;
    // Unread Dealership messages, from Xtime's chat (works without Xtime's page)
    if (my.chatConfig || my.findInspectFrame()) {
      try {
        const r = await (my.chatConfig ? frame : my.findInspectFrame()).executeJavaScript(`${S.XTIME_CHAT}({"type":"count"}, ${my.chatCfgArg()})`, true);
        if (r?.config) my.keepChatConfig(r.config);
        if (r?.ok && Number.isFinite(r.count)) n = r.count;
      } catch {}
    }
    // Otherwise the counter on Xtime's page, when it's loaded
    const page = my.findInspectFrame();
    if (n == null && page) {
      try { n = await page.executeJavaScript(`(() => { const e = document.querySelector('.messages-count'); return e ? Number(e.getAttribute('data-badge')) || 0 : null; })()`, true); } catch {}
    }
    if (n == null) return;
    if (my.msgCount != null && n > my.msgCount) notifyUser('New Xtime message', n - my.msgCount === 1 ? `1 new message · ${n} unread` : `${n - my.msgCount} new messages · ${n} unread`, null, 'message');
    if (n !== my.msgCount) { my.msgCount = n; my.pushState(); }
  }
  const msgTimer = setInterval(readMsgCount, 20000);
  my.msgTimer = msgTimer;
};
