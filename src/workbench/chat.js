// The Dealership chat panel: Xtime's chat read through the Xtime page, and messages marked read (ids only, no text).
'use strict';
const fs = require('fs');
const S = require('../app/state'); // shared app state (see app/state.js)
const { userFile } = require('../app/files');

module.exports = function install(my) {
  Object.assign(my, { chatRun }); // used by the window's other parts

  // Messages you've read in the app (Xtime message ids only, no text): shown as read right away,
  // even before Xtime's own lists catch up. Kept 60 days.
  const READ_KEEP = 60 * 24 * 60 * 60 * 1000;
  const readCache = new Map(Object.entries(my.readJson('chat-read.json', {}) || {}).filter(([, t]) => Date.now() - t < READ_KEEP));
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
    const frame = my.findXtimeDataFrame();
    if (!frame || !my.chatConfig) { readLog(`${channelId} skipped: ${!frame ? 'Xtime not loaded' : 'chat settings not known yet'}`); return; }
    let r = null;
    try { r = await frame.executeJavaScript(`${S.XTIME_CHAT}(${JSON.stringify({ type: 'read', channelId })}, ${my.chatCfgArg()})`, true); } catch (e) { r = { ok: false, error: e.message }; }
    const shownUnread = (messages || []).filter((m) => m.unread && !m.mine).length;
    readLog(`${channelId} unread here ${shownUnread} · Xtime said unread ${r?.tried ?? (r?.ids || []).length} ${r?.shape || ''} · ${r?.ok ? 'receipts sent' + (r.ids?.length ? ` for ${r.ids.join(',')}` : ' (none needed)') : `FAILED: ${r?.error || 'no answer'}`}`);
    if (!r?.ok) { readBusy.delete(channelId); return; }                 // try again next time it's opened
    const now = Date.now();
    // Everything others sent in this conversation counts as read here now
    for (const id of [...(r.ids || []), ...(messages || []).filter((m) => !m.mine).map((m) => m.id)]) if (id) readCache.set(String(id), now);
    for (const [id, t] of readCache) if (now - t > READ_KEEP) readCache.delete(id);
    my.writeJson('chat-read.json', Object.fromEntries(readCache));
    if (my.chrome && !my.chrome.webContents.isDestroyed()) my.chrome.webContents.send('chat', { kind: 'read', channelId, ids: [...readCache.keys()] });
    setTimeout(my.readMsgCount, 3000); // the unread counter catches up
  }

  async function chatRun(action, kind) {
    const reply = (r) => { if (my.chrome && !my.chrome.webContents.isDestroyed()) my.chrome.webContents.send('chat', { kind, channelId: action.channelId, ...r }); };
    // Chat settings come from Xtime's full page the first time; after that the background page is enough
    const frame = my.chatConfig ? my.findXtimeDataFrame() : (my.findInspectFrame() || my.findXtimeDataFrame());
    if (!frame) return reply({ ok: false, error: 'Xtime is still loading' });
    try {
      const r = await frame.executeJavaScript(`${S.XTIME_CHAT}(${JSON.stringify(action)}, ${my.chatCfgArg()})`, true);
      if (r?.config) my.keepChatConfig(r.config);
      if (r?.needPage) my.unparkXtime(); // e.g. the first time: load Xtime's page once to learn its chat settings
      if (r?.fieldNames?.length && !my.loggedChatFields.has(kind)) { // field names only, for mapping; no message text
        my.loggedChatFields.add(kind);
        try { fs[my.loggedChatFields.size === 1 ? 'writeFileSync' : 'appendFileSync'](userFile('xtime-chat-fields.txt'), `${new Date().toISOString()} ${kind}\n${r.fieldNames.join('\n')}\n`); } catch {}
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
};
