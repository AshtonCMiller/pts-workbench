// Signing in to Xtime from the app's own form (the Cox sign-in page runs behind it), and getting Xtime to its Inspect
// page, where the app reads ROs.
'use strict';
const S = require('../app/state'); // shared app state (see app/state.js)
const { COX_SIGNIN_HOST } = require('../app/config');

module.exports = function install(my) {
  Object.assign(my, { signinRun, checkSignin, waitForStep, findInspectFrame, findXtimeDataFrame }); // used by the window's other parts

  // ---------- signing in to Xtime from the app's own form ----------
  // When Xtime sends its page to Cox's sign-in, the app shows its own form and fills in Cox's real page.
  my.xtimeSignin = null; // { step, error, factors, message, buttons, username, busy } while signing in
  function findSigninFrame() {
    const main = my.xtimeView?.webContents?.mainFrame;
    if (!main) return null;
    return (main.framesInSubtree || []).find((f) => { try { return COX_SIGNIN_HOST.test(new URL(f.url).hostname); } catch { return false; } }) || null;
  }
  async function signinRun(action) {
    const frame = findSigninFrame();
    if (!frame) return null;
    try { return await frame.executeJavaScript(`${S.COX_SIGNIN}(${JSON.stringify(action || { type: 'state' })})`, true); } catch { return null; }
  }
  let signinChecking = false;
  async function checkSignin() {
    if (signinChecking || !my.xtimeView || my.xtimeView.webContents.isDestroyed()) return;
    signinChecking = true;
    try {
      const frame = findSigninFrame();
      if (!frame) {
        // Landed on Scheduling soon after signing in: go to Inspect
        if (autoInspectUntil > Date.now() && onScheduling() && !my.xtimeView.webContents.isLoading()) switchToInspect();
        if (my.xtimeSignin && !['loading', 'starting'].includes(my.xtimeSignin.step)) { // signed in: make sure Xtime is on Inspect (it lands on Scheduling), then read it again
          // The dialog stays up ("Loading Inspect…") until the dashboard has data again
          my.xtimeSignin = { step: 'loading', since: Date.now(), busy: false, error: '' }; my.pushState();
          autoInspectUntil = Date.now() + 90 * 1000;
          setTimeout(() => ensureInspect(2), 4000);
        } else if (['loading', 'starting'].includes(my.xtimeSignin?.step) && !my.xtimeSignin.slow && Date.now() - my.xtimeSignin.since > 90 * 1000) {
          my.xtimeSignin = { ...my.xtimeSignin, slow: true }; my.pushState(); // taking long: offer a reload
        }
        return;
      }
      const st = await signinRun({ type: 'state' });
      if (!st?.ok) return;
      // Xtime's sign-in page often shows for a moment while Xtime signs in by itself (or a saved login is
      // filled in), then moves on. Typing into it then gets lost, so the form only shows once the page has
      // stayed put for a few seconds; until then the dialog says it's connecting.
      const key = `${String(frame.url).split(/[?#]/)[0]}|${st.step}`;
      if (key !== signinSettle.key || my.xtimeView.webContents.isLoading()) signinSettle = { key, since: Date.now() };
      const settled = Date.now() - signinSettle.since >= 4000 || Date.now() - my.signinActedAt < 60 * 1000;
      if (!settled) {
        if (my.xtimeSignin?.step !== 'starting') { my.xtimeSignin = { step: 'starting', since: Date.now(), busy: false, error: '' }; my.pushState(); }
        return;
      }
      let host = '';
      try { host = new URL(frame.url).hostname; } catch {}
      const saved = host ? my.getLogin(host, true) : null;
      const next = { step: st.step, error: st.error, factors: st.factors, message: st.message, buttons: st.buttons,
        username: st.username || saved?.username || '', savedPassword: !!(saved && hasSavedPassword(host)), canRemember: my.canEncrypt(), busy: !!my.xtimeSignin?.busy, host };
      if (JSON.stringify(next) !== JSON.stringify(my.xtimeSignin)) { my.xtimeSignin = next; my.pushState(); }
    } catch { /* checked again shortly */ } finally { signinChecking = false; }
  }
  let signinSettle = { key: '', since: 0 };
  my.signinActedAt = 0; // you pressed something in the sign-in form: show its next steps right away
  const signinTimer = setInterval(checkSignin, 1500);
  my.signinTimer = signinTimer;
  const hasSavedPassword = (host) => { try { return !!my.getLogin(host)?.password; } catch { return false; } };

  // After signing in, Xtime opens Scheduling; the app's data comes from Inspect, so go there the same way
  // Scheduling's top-left menu does ("INSPECT": login.<xtime domain>/?pt=svp&view=svp&StoreID=<store>)
  let autoInspectUntil = Date.now() + 90 * 1000; // also right after the app starts (it may sign in on its own)
  let lastInspectSwitch = 0;
  const onScheduling = () => {
    try {
      const u = new URL(my.xtimeView.webContents.getURL());
      return /(^|\.)xtime\.app\.coxautoinc\.com$/i.test(u.hostname) && (/^\/panama\//i.test(u.pathname) || /^x\d*\./i.test(u.hostname));
    } catch { return false; }
  };
  function switchToInspect() {
    if (Date.now() - lastInspectSwitch < 20000) return false; // once at a time
    let domain = 'xtime.app.coxautoinc.com';
    try { domain = new URL(my.xtimeView.webContents.getURL()).hostname.replace(/^[^.]+\./, '') || domain; } catch {}
    lastInspectSwitch = Date.now();
    my.xtimeStatus = 'Opening Xtime Inspect…'; my.pushState();
    try { my.xtimeView.webContents.loadURL(`https://login.${domain}/?pt=svp&view=svp&StoreID=${encodeURIComponent(my.settings.storeId)}`); } catch {}
    return true;
  }
  function ensureInspect(triesLeft) {
    if (!my.xtimeView || my.xtimeView.webContents.isDestroyed()) return;
    if (findInspectFrame()) { my.pollXtime(); return; }
    if (findSigninFrame()) return; // still signing in (e.g. a verification step)
    if (my.xtimeView.webContents.isLoading()) { if (triesLeft > 0) setTimeout(() => ensureInspect(triesLeft), 2000); return; }
    if (!(onScheduling() && switchToInspect())) my.reloadXtime('Opening Xtime Inspect…');
    if (triesLeft > 1) setTimeout(() => ensureInspect(triesLeft - 1), 8000);
  }

  async function waitForStep(steps, ms = 8000) {
    const end = Date.now() + ms;
    while (Date.now() < end) {
      const st = await signinRun({ type: 'state' });
      if (st?.ok && (steps.includes(st.step) || st.error)) return st;
      if (!findSigninFrame()) return { ok: true, step: 'done' };
      await new Promise((r) => setTimeout(r, 300));
    }
    return await signinRun({ type: 'state' });
  }

  // Xtime's Inspect frame inside the full Xtime page (only there while that page is loaded)
  function findInspectFrame() {
    const main = my.xtimeView?.webContents?.mainFrame;
    if (!main) return null;
    return main.framesInSubtree.find((f) => {
      try { return new URL(f.url).hostname === my.settings.xtimeDataHost; } catch { return false; }
    }) || null;
  }
  // Where the app's Xtime requests run: the small background page on the Inspect site when it's ready
  // (no Xtime page needed), otherwise the full Xtime page's Inspect frame
  function findXtimeDataFrame() {
    if (my.liteReady()) return my.xtimeLite.webContents.mainFrame;
    return findInspectFrame();
  }
};
