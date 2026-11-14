// Feedback: bug reports and feature ideas sent to the sync server, the replies, and the admin view (one employee
// number, set in package.json "ptsWorkbench.feedbackAdmin", can read them all).
'use strict';
const { app, net, safeStorage } = require('electron');
const { FEEDBACK_ADMIN, SYNC_URL } = require('../app/config');
const { profileFile, readJsonAt, writeJsonAt } = require('../app/files');

module.exports = function install(my) {
  // ---------- Feedback: bug reports & feature ideas ----------
  // Sent to the sync server's /v1/reports. A report carries the person's name, what they wrote, the app
  // version and which screen they were on: never VINs, RO numbers or customer details. Each computer has a
  // random reporter ID so people can see their own reports' status. The admin (FEEDBACK_ADMIN) reads them
  // all with a password (the server's ADMIN_KEY), kept on this computer encrypted.
  my.feedbackNew = 0;
  const isFeedbackAdmin = () => !!FEEDBACK_ADMIN && String(my.settings.employeeNumber || '').trim() === FEEDBACK_ADMIN;
  my.isFeedbackAdmin = isFeedbackAdmin;
  function reporterId() {
    const f = profileFile(my.profile.id, 'reporter.json');
    let r = readJsonAt(f, null);
    if (!/^[a-f0-9]{32}$/.test(r?.id || '')) { r = { id: require('crypto').randomBytes(16).toString('hex') }; writeJsonAt(f, r); }
    return r.id;
  }
  function adminKey() {
    const rec = my.readJson('feedback-key.json', null);
    if (!rec?.k) return '';
    try { return my.canEncrypt() ? safeStorage.decryptString(Buffer.from(rec.k, 'base64')) : ''; } catch { return ''; }
  }
  function setAdminKey(key) {
    if (!key) { my.writeJson('feedback-key.json', {}); return; }
    if (!my.canEncrypt()) return;
    my.writeJson('feedback-key.json', { k: safeStorage.encryptString(key).toString('base64') });
  }
  function screenName() {
    if (my.mode !== 'manual') return 'Xtime';
    const ws = my.workspaces.get(my.activeRO);
    const t = ws && my.currentTab(ws);
    if (!t) return 'Workbench';
    return ['Workbench', my.inService(t.kind) ? 'Service Manual' : '', my.TAB_NAMES[t.kind] || t.kind].filter(Boolean).join(' › ');
  }
  const sendFeedback = (data) => { if (my.chrome && !my.chrome.webContents.isDestroyed()) my.chrome.webContents.send('feedback', data); };
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
      my.feedbackNew = list.filter((x) => x.status === 'new').length;
      sendFeedback({ kind: 'all', list });
      my.pushState();
    } catch (e) { sendFeedback({ kind: 'all', error: `Couldn't load feedback: ${e.message || e}` }); }
  }
  // The admin's badge: how many new reports (checked at start and every 15 minutes)
  async function countNewFeedback() {
    if (!isFeedbackAdmin() || !adminKey() || !SYNC_URL) return;
    try {
      const r = await feedbackCall('GET', '/v1/reports', null, true);
      if (r.status === 200) { const n = (r.body.reports || []).filter((x) => x.status === 'new').length; if (n !== my.feedbackNew) { my.feedbackNew = n; my.pushState(); } }
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
          kind, title, body, reporter: reporterId(), name: my.displayName(), version: app.getVersion(),
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
      if (!my.canEncrypt()) { sendFeedback({ kind: 'all', error: "This computer can't store the password safely." }); return; }
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
        my.notice('Saved');
      } catch (e) { my.notice(`Couldn't save: ${e.message || e}`); }
      loadAllFeedback();
    },
    'feedback-delete': async (id) => {
      if (!isFeedbackAdmin() || !Number.isInteger(Number(id))) return;
      try {
        const r = await feedbackCall('DELETE', `/v1/reports/${Number(id)}`, null, true);
        if (r.status !== 200) throw new Error(r.body.error || `server said ${r.status}`);
      } catch (e) { my.notice(`Couldn't delete: ${e.message || e}`); }
      loadAllFeedback();
    },
  };
  my.feedbackHandlers = feedbackHandlers;
  setTimeout(countNewFeedback, 20 * 1000);
  setInterval(countNewFeedback, 15 * 60 * 1000);
};
