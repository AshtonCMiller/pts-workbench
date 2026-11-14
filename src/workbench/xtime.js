// Reading Xtime: the small background page the app reads through, putting Xtime's own page away once that works, your
// ROs (polling), recovering when Xtime's page fails or its sign-in runs out, and looking up an RO.
'use strict';
const { WebContentsView } = require('electron');
const fs = require('fs');
const S = require('../app/state'); // shared app state (see app/state.js)
const { pushAll } = S;
const { userFile } = require('../app/files');
const { forgetXtimeSignin } = require('../app/web');

module.exports = function install(my) {
  Object.assign(my, { liteReady, startLite, keepChatConfig, unparkXtime, parkCheck, pollXtime, reloadXtime, viewingROFromXtime, lookupRO, startPolling, chatSignin }); // used by the window's other parts

  // ---------- Xtime's sign-in too old (its chat stops first) ----------
  // Xtime stays signed in between launches (app/web.js). When that sign-in has got too old, Xtime still shows ROs but
  // its chat stops, the same as on Xtime's website. Then the kept sign-in is thrown away and Xtime's page reloaded, so
  // it asks to sign in again (the app's sign-in form, or straight through with a saved password). Once per sign-in:
  // First the kept sign-in only (what closing the app used to throw away). If chat still says so once Xtime has
  // reloaded without asking to sign in, every Cox / Xtime cookie goes (a fully fresh sign-in). After that it isn't
  // repeated until chat has worked, so a chat problem that isn't the sign-in can't keep signing you out.
  let chatSigninStep = 0;   // 0: nothing done yet, 1: kept sign-in thrown away, 2: everything thrown away, 3: Xtime asked to sign in
  let chatSigninAt = 0;
  const xlog = (line) => { try { fs.appendFileSync(userFile('xtime.log'), `${new Date().toISOString()} ${line}\n`); } catch {} };
  function chatSignin(r) {
    if (r?.ok) { if (chatSigninStep) xlog('chat works again'); chatSigninStep = 0; return; }
    if (!r?.signedOut || my.xtimeSignin || chatSigninStep >= 2) return;
    if (chatSigninStep === 1 && Date.now() - chatSigninAt < 60 * 1000) return; // give Xtime's page a minute after reloading
    chatSigninStep++;
    chatSigninAt = Date.now();
    const all = chatSigninStep === 2;
    xlog(`chat says Xtime's sign-in is too old (${r.error || ''}): ${all ? 'still, after reloading: throwing away every Cox / Xtime cookie' : 'throwing away the kept sign-in'}`);
    forgetXtimeSignin(my.profile.id, { all }).catch(() => []).then((gone) => {
      xlog(`removed ${gone.length} cookies (${gone.join(', ') || 'none'}); reloading Xtime`);
      reloadXtime('Xtime sign-in is too old: signing in again…');
    });
  }
  // Xtime asking to sign in (the app's form showing) means it worked: nothing more until chat works again
  setInterval(() => { if (my.xtimeSignin && chatSigninStep && chatSigninStep < 3) { xlog('Xtime asked to sign in'); chatSigninStep = 3; } }, 3000);

  // ---------- the small background page ----------
  // A blank-ish page on Xtime's Inspect site (its robots.txt). It shares the window's Xtime login, so the
  // same requests Xtime's page makes work from it, without loading or running Xtime's page itself.
  // The full Xtime page is then only loaded to sign in, to show it, or when asked for.
  my.xtimeLite = null;
  my.liteLoading = false;
  const liteUrl = () => `https://${my.settings.xtimeDataHost}/robots.txt`;
  function liteReady() {
    try {
      return !!my.xtimeLite && !my.xtimeLite.webContents.isDestroyed() && !my.liteLoading
        && new URL(my.xtimeLite.webContents.getURL()).hostname === my.settings.xtimeDataHost;
    } catch { return false; }
  }
  function startLite() {
    if (!my.xtimeLite) {
      my.xtimeLite = new WebContentsView({ webPreferences: { partition: my.PARTITION, sandbox: true, contextIsolation: true, nodeIntegration: false, backgroundThrottling: false } });
      my.hardenWebContents(my.xtimeLite.webContents);
      my.xtimeLite.webContents.setWindowOpenHandler(() => ({ action: 'deny' }));
      my.xtimeLite.webContents.on('did-start-loading', () => { my.liteLoading = true; });
      my.xtimeLite.webContents.on('did-stop-loading', () => { my.liteLoading = false; });
    }
    try { my.xtimeLite.webContents.loadURL(liteUrl()); } catch {}
  }

  // Xtime's chat settings (dealer code, your Xtime username), saved from the full page so chat
  // works without it. Nothing about customers.
  my.chatConfig = my.readJson('xtime-chat.json', null);
  function keepChatConfig(c) {
    if (!c || !c.dealerCode || !c.userName) return;
    const next = { dealerCode: c.dealerCode, userName: c.userName, env: c.env || '', chatgatewayUrl: c.chatgatewayUrl || '', storeId: c.storeId || my.settings.storeId };
    if (JSON.stringify(next) !== JSON.stringify(my.chatConfig)) { my.chatConfig = next; my.writeJson('xtime-chat.json', my.chatConfig); }
  }
  const chatCfgArg = () => JSON.stringify(my.chatConfig || { storeId: my.settings.storeId });
  my.chatCfgArg = chatCfgArg;

  // The full Xtime page: put away (blank) while the app's dashboard is in use and everything works without it
  my.xtimeParked = false;
  my.liteTriedAt = Date.now();
  function unparkXtime(status) {
    if (!my.xtimeParked) return;
    my.xtimeParked = false;
    if (status) { my.xtimeStatus = status; my.pushState(); }
    try { my.xtimeView.webContents.loadURL(my.settings.xtimeUrl); } catch {}
  }
  function parkCheck() {
    if (my.xtimeParked || !my.settings.xtimeDashboard || my.xtimeSignin || !liteReady() || !my.chatConfig) return;
    if (!my.findInspectFrame() || my.xtimeView.webContents.isLoading()) return;      // still signing in / getting to Inspect
    if (!my.lastPoll || Date.now() - my.lastPoll > 3 * 60 * 1000 || my.xtimeFailures) return; // the background page must be working
    if (Date.now() - my.xtimeLoadedAt < 30 * 1000) return;
    my.xtimeParked = true;
    try { my.xtimeView.webContents.loadURL('about:blank'); } catch {}
  }

  let polling = false;
  async function pollXtime() {
    if (polling) return;
    polling = true;
    try { await pollXtimeOnce(); } finally { polling = false; }
  }

  // Reload the Xtime page: it signs back in on its own while the overall login is still good,
  // otherwise it shows the sign-in page, which the saved password can fill in.
  function reloadXtime(status) {
    my.lastRecovery = Date.now();
    my.xtimeFailures = 0;
    if (status) { my.xtimeStatus = status; my.pushState(); }
    my.xtimeParked = false;
    try { my.xtimeView.webContents.loadURL(my.settings.xtimeUrl); } catch {}
  }

  // When Xtime is stuck on an error page (e.g. a sign-in step that can't be reloaded), start it fresh
  const onLoginPage = () => {
    try {
      const u = new URL(my.xtimeView.webContents.getURL());
      return u.protocol === 'https:' && u.hostname !== my.settings.xtimeDataHost;
    } catch { return false; }
  };
  my.onLoginPage = onLoginPage;

  function xtimeFailed(message, expired) {
    my.xtimeFailures++;
    if (expired && my.xtimeParked) { reloadXtime('Xtime session expired — reconnecting…'); return; }
    if ((expired || my.xtimeFailures >= 2) && Date.now() - my.lastRecovery > 10 * 60 * 1000) {
      reloadXtime('Xtime session expired — reconnecting…');
      return;
    }
    my.xtimeStatus = expired ? 'Xtime session expired — log in to Xtime' : message;
    my.pushState();
  }

  // Employee number → Xtime technician ID (and your name for the title bar), from the store's list.
  // Looked up once per employee number; the result is saved with the settings.
  async function resolveTechId(frame, force = false) {
    const num = String(my.settings.employeeNumber || '').trim();
    if (!num) return;
    if (!force && my.settings.techIdFor === num && my.settings.techId) return;
    let res = null;
    try {
      res = await frame.executeJavaScript(
        `${S.XTIME_EMPLOYEE}(${JSON.stringify(num)}, ${JSON.stringify(my.settings.storeId)}, ${JSON.stringify(my.settings.xtimeEmployeesPath)})`, true);
    } catch { res = null; }
    if (res?.ok) {
      my.settings.techId = res.id;
      my.settings.techIdFor = num;
      my.writeJson('settings.json', my.settings);
      my.employeeStatus = `Found: ${res.name || 'employee ' + num}${res.active ? '' : ' (marked inactive in Xtime)'}`;
      if (res.name && res.name !== my.xtimeName) { my.xtimeName = res.name; my.updateTitle(); my.saveWorkspaces(); }
    } else {
      my.employeeStatus = res?.error || "Couldn't look up the employee number yet";
    }
    pushAll();
  }

  async function pollXtimeOnce() {
    const frame = my.findXtimeDataFrame();
    if (!frame) {
      if (my.xtimeParked) { unparkXtime('Opening Xtime…'); if (!my.liteLoading) startLite(); return; }
      if (onLoginPage()) {
        // Xtime has sent the page to its sign-in screen; a saved login fills it in
        my.xtimeStatus = 'Log in to Xtime to load your ROs';
        my.pushState();
      } else {
        // An error page, or a page that never finished: start Xtime fresh (at most every 2 minutes)
        my.xtimeFailures++;
        if (Date.now() - my.lastRecovery > 2 * 60 * 1000) reloadXtime('Xtime page failed to load — reloading…');
        else { my.xtimeStatus = 'Xtime page failed to load — retrying shortly'; my.pushState(); }
      }
      return;
    }
    await resolveTechId(frame);
    if (!my.settings.techId) {
      if (my.xtimeSignin?.step === 'starting') my.xtimeSignin = null; // nothing to load until the employee number is set
      my.xtimeStatus = my.settings.employeeNumber ? (my.employeeStatus || 'Looking up your employee number…') : 'Enter your employee number in ⚙ Settings';
      my.pushState();
      return;
    }
    try {
      const res = await frame.executeJavaScript(
        `${S.XTIME_EXTRACT}(${JSON.stringify(my.settings.techId)}, ${JSON.stringify(my.settings.xtimeApiPath)}, ${JSON.stringify(String(my.settings.xtimeEmployeesPath || '').replace('{STORE}', encodeURIComponent(my.settings.storeId)))})`, true);
      if (!res?.ok) { xtimeFailed(res?.error || 'Could not read Xtime', !!res?.expired); return; }
      my.xtimeFailures = 0;
      my.myROs = res.mine;
      if (my.syncWaiting.size) setTimeout(my.retrySyncedROs, 0); // ROs opened on another computer, waiting for Xtime
      if (my.xtimeSignin?.step === 'loading' || my.xtimeSignin?.step === 'starting') my.xtimeSignin = null; // the data is here: close the dialog
      my.noteAssigned(my.myROs);
      my.noteRoChanges(my.myROs);
      // Xtime's field names (names only, no values), so dashboard columns that don't match can be fixed
      if (res.fieldNames && !my.loggedXtimeFields) {
        my.loggedXtimeFields = true;
        try { fs.writeFileSync(userFile('xtime-fields.txt'), `${new Date().toISOString()}\n${res.fieldNames.join('\n')}\n`); } catch {}
      }
      // Move ROs to Review when they go to Pending Review or close (skip an empty dashboard: likely a glitch)
      if (res.total > 0) my.tracker.syncStatus(my.settings.techId, my.myROs);
      my.lastPoll = Date.now();
      my.xtimeStatus = `${my.myROs.length} of ${res.total} ROs are yours`;
      // The name from the employee list wins; the RO data's name is only a fallback without an employee number
      if (res.techName && !my.settings.employeeNumber && res.techName !== my.xtimeName) { my.xtimeName = res.techName; my.updateTitle(); my.saveWorkspaces(); pushAll(); }
      for (const r of my.myROs) {
        const ws = my.workspaces.get(r.ro);
        if (ws) ws.ymm = [r.year, r.make, r.model].filter(Boolean).join(' ');
      }
      my.pushState(); // show the list right away, then fill in the lines

      let failed = 0;
      const mineSet = new Set(my.myROs.map((r) => r.ro));
      const others = [...my.workspaces.values()].filter((w) => !mineSet.has(w.ro)).map((w) => ({ ro: w.ro, other: true }));
      for (const r of [...my.myROs, ...others]) {
        try {
          const d = await frame.executeJavaScript(
            `${S.XTIME_DETAILS}(${JSON.stringify(r.ro)}, ${JSON.stringify(my.settings.xtimeDetailsPath)})`, true);
          if (d?.ok) {
            if (!r.other) my.noteApprovals(r, d.lines);
            my.tracker.updateLines(r.ro, d.lines, r.other ? '' : [r.year, r.make, r.model].filter(Boolean).join(' '));
          }
          else if (!r.other) failed++; // someone else's RO may have closed; don't count it as a problem
        } catch { failed++; }
      }
      if (failed) my.xtimeStatus += ` · lines unavailable for ${failed}`;
    } catch (e) {
      xtimeFailed('Could not read Xtime (still loading?)', false);
    }
    my.pushState();
  }

  // RO number from an address like /RepairOrder/Details/820367 (built from the RO details path setting)
  function viewingROFromXtime() {
    const frames = my.xtimeView?.webContents?.mainFrame?.framesInSubtree || [];
    const tpl = String(my.settings.xtimeDetailsPath || '/RepairOrder/Details/{RO}');
    const re = new RegExp(tpl.replace(/[.*+?^$()|[\]\\]/g, '\\$&').replace('\\{RO\\}', '([A-Za-z0-9-]+)').replace('{RO}', '([A-Za-z0-9-]+)'), 'i');
    for (const f of frames) {
      try {
        const m = new URL(f.url).pathname.match(re);
        if (m) return decodeURIComponent(m[1]);
      } catch { /* about:blank etc. */ }
    }
    return null;
  }

  async function lookupRO(ro) {
    if (!/^\w{1,20}$/.test(ro)) return;
    my.lookup = { ro, status: 'loading', message: 'Loading lines from Xtime…' };
    my.pushState();
    const frame = my.findXtimeDataFrame();
    if (!frame) {
      my.lookup = { ro, status: 'error', message: 'Xtime isn\'t logged in, so its lines can\'t be loaded. Add them below by hand.' };
      my.pushState();
      return;
    }
    try {
      const d = await frame.executeJavaScript(
        `${S.XTIME_DETAILS}(${JSON.stringify(ro)}, ${JSON.stringify(my.settings.xtimeDetailsPath)}, ${JSON.stringify(my.settings.xtimeApiPath)})`, true);
      if (d?.ok) {
        my.tracker.updateLines(ro, d.lines, d.vehicle?.ymm || '');
        const shown = my.tracker.roSummary(ro)?.lines.filter((l) => !l.manual).length ?? 0;
        const hidden = d.lines.length - shown;
        my.lookup = {
          ro, status: 'ok',
          message: !d.lines.length ? 'Xtime has no lines for this RO. Add them below by hand.'
            : hidden ? `Xtime has ${d.lines.length} lines on this RO; ${hidden} declined or not approved, so not shown.` : '',
        };
      } else {
        const why = d?.notOnDashboard ? 'it isn\'t on the Xtime dashboard any more (closed?)' : (d?.error || 'unknown error');
        my.lookup = { ro, status: 'error', message: `Couldn't load RO ${ro} from Xtime: ${why}. Add its lines below by hand.` };
      }
    } catch (e) {
      my.lookup = { ro, status: 'error', message: `Couldn't load RO ${ro} from Xtime. Add its lines below by hand.` };
    }
    pushAll();
  }

  function startPolling() {
    clearInterval(my.pollTimer);
    my.pollTimer = setInterval(pollXtime, Math.max(15, Number(my.settings.pollSeconds) || 60) * 1000);
  }
};
