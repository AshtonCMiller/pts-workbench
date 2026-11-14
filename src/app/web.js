// Web pages in general: printing the part of a page you're working in (PTS and some Xtime screens use frames), sync
// requests, each profile's browser session (looking like plain Chrome), and staying signed in to Xtime between launches.
'use strict';
const { net, session } = require('electron');
const { partitionFor, profileFile, readJsonAt, writeJsonAt } = require('./files');

// =====================================================================================
// Printing. PTS (and some Xtime screens) show their content inside frames, so this prints the part
// of the page you're working in, not the outer page with the content cut off.
// =====================================================================================
const appUiContents = new WeakSet(); // the app's own screens (not web pages)
const isPrintKey = (input) => input.type === 'keyDown' && (input.control || input.meta) && !input.alt && input.key.toLowerCase() === 'p';
async function printPage(wc) {
  const fallback = () => wc.print({}, () => {});
  try {
    const top = wc.mainFrame;
    const probe = `(() => { const d = document; const fs = d.body && d.body.tagName === 'FRAMESET';
      return { focus: d.hasFocus(), area: innerWidth * innerHeight, frameset: fs,
               text: fs ? 0 : ((d.body && d.body.innerText) || '').trim().length }; })()`;
    const info = [];
    for (const f of top.framesInSubtree) {
      let depth = 0;
      for (let p = f.parent; p; p = p.parent) depth++;
      try { info.push({ f, depth, ...(await f.executeJavaScript(probe)) }); } catch { /* another site's frame */ }
    }
    const topInfo = info.find((i) => i.f === top);
    if (!topInfo) return fallback();
    const big = (i) => i.area >= topInfo.area * 0.3 && !i.frameset && i.text > 0;
    // The frame you last clicked in, if it's a big part of the page; otherwise the biggest frame with content
    let pick = info.filter((i) => i.f !== top && i.focus && big(i)).sort((a, b) => b.depth - a.depth)[0];
    if (!pick && (topInfo.frameset || topInfo.text < 200)) pick = info.filter((i) => i.f !== top && big(i)).sort((a, b) => b.area - a.area)[0];
    pick ||= topInfo;
    await pick.f.executeJavaScript('window.print()', true);
  } catch { fallback(); }
}

// Sync requests use Electron's net.fetch, which follows the computer's own network/proxy settings
async function syncRequest(method, url, body) {
  const res = await net.fetch(url, {
    method,
    headers: { 'content-type': 'application/json' },
    body: body ? JSON.stringify(body) : undefined,
  });
  let json = null;
  try { json = await res.json(); } catch { /* not JSON */ }
  return { status: res.status, body: json };
}

function prepareSession(id) {
  const ses = session.fromPartition(partitionFor(id));
  // Present as plain Chrome so the sites treat the app like your normal browser.
  ses.setUserAgent(ses.getUserAgent().replace(/\s(?:Electron|pts-workbench|PTS Workbench)\/\S+/gi, ''));
  keepXtimeSignedIn(ses, id);
}

// ---------- staying signed in to Xtime between launches ----------
// Xtime (Cox) keeps its sign-in in "session" cookies, which are thrown away when the app closes, so it asked
// to sign in on every launch. While the app runs, each of those cookies is saved again with an end date, so
// the next launch picks up where the last one left off. Xtime itself still decides how long a sign-in lasts:
// when it ends one (signing out, or its own time limit), the cookie goes away like it would in a browser.
// Only Cox/Xtime cookies, only in this window's own profile (each person's window has its own sign-in).
// The cookies kept this way are listed in xtime-kept-cookies.json (names and addresses only), so that when Xtime's
// sign-in turns out to be too old (its chat is the first thing to stop), forgetXtimeSignin() can throw exactly those
// away, the same as closing the app used to, and Xtime asks to sign in again.
const KEEP_HOSTS = /(^|\.)(coxautoinc\.com|xtime\.com)$/i;
const KEEP_DAYS = 14; // pushed forward every time Xtime updates the cookie
const keptSessions = new WeakSet();
const keptFile = (id) => profileFile(id, 'xtime-kept-cookies.json');
const keptById = new Map(); // profile id -> Map("url|name" -> { url, name })
const keptList = (id) => {
  if (!keptById.has(id)) keptById.set(id, new Map((readJsonAt(keptFile(id), []) || []).map((k) => [`${k.url}|${k.name}`, k])));
  return keptById.get(id);
};
function keepXtimeSignedIn(ses, id) {
  if (keptSessions.has(ses)) return;
  keptSessions.add(ses);
  let saveTimer = null;
  const remember = (url, name) => {
    const kept = keptList(id);
    if (kept.has(`${url}|${name}`)) return;
    kept.set(`${url}|${name}`, { url, name });
    clearTimeout(saveTimer);
    saveTimer = setTimeout(() => writeJsonAt(keptFile(id), [...keptList(id).values()]), 1000);
  };
  const keep = async (c) => {
    if (!c || c.session === false || c.expirationDate) return;         // already has an end date
    const host = String(c.domain || '').replace(/^\./, '');
    if (!host || !KEEP_HOSTS.test(host)) return;
    const details = {
      url: `${c.secure ? 'https' : 'http'}://${host}${c.path || '/'}`,
      name: c.name, value: c.value, path: c.path || '/',
      secure: !!c.secure, httpOnly: !!c.httpOnly,
      expirationDate: Math.floor(Date.now() / 1000) + KEEP_DAYS * 24 * 60 * 60,
    };
    if (!c.hostOnly) details.domain = c.domain;                        // host-only cookies stay host-only
    if (c.sameSite && c.sameSite !== 'unspecified') details.sameSite = c.sameSite;
    try { await ses.cookies.set(details); remember(details.url, c.name); } catch { /* left as a session cookie */ }
  };
  ses.cookies.on('changed', (_e, cookie, cause, removed) => {
    if (removed || cause === 'expired' || cause === 'evicted') return;
    keep(cookie);
  });
  // ones set before this ran (e.g. a second window of the same profile)
  ses.cookies.get({}).then((all) => all.forEach(keep)).catch(() => {});
}

// Xtime's sign-in is too old: throw away the cookies kept between launches (only those: what closing the app used to
// throw away), so Xtime's page asks to sign in again. Saved passwords and everything else stay.
// all: every Cox / Xtime cookie (a fully fresh sign-in, if throwing away the kept ones wasn't enough; this may also
// forget "remember this device", so Xtime can ask for a code again).
// Returns what was removed, as "domain name" (no values), for the log.
async function forgetXtimeSignin(id, { all = false } = {}) {
  const ses = session.fromPartition(partitionFor(id));
  const kept = keptList(id);
  const list = new Map([...kept.values()].map((k) => [`${k.url}|${k.name}`, k]));
  kept.clear(); // the new sign-in's cookies get listed again as they're kept
  writeJsonAt(keptFile(id), []);
  // Also every Cox / Xtime cookie ending within the keeping time: ones kept before the list existed (or before it was
  // saved) end then too, since each one is pushed forward to that every time it's kept
  const until = Date.now() / 1000 + KEEP_DAYS * 24 * 60 * 60 + 3600;
  for (const c of await ses.cookies.get({}).catch(() => [])) {
    const host = String(c.domain || '').replace(/^\./, '');
    if (!KEEP_HOSTS.test(host)) continue;
    if (!all && !(c.expirationDate && c.expirationDate <= until)) continue;
    const url = `${c.secure ? 'https' : 'http'}://${host}${c.path || '/'}`;
    list.set(`${url}|${c.name}`, { url, name: c.name });
  }
  const gone = [];
  for (const k of list.values()) {
    try { await ses.cookies.remove(k.url, k.name); gone.push(`${new URL(k.url).hostname} ${k.name}`); } catch { /* already gone */ }
  }
  // the pages' own saved sign-in pieces (Xtime keeps its chat key in session storage)
  if (all) {
    const origins = new Set([...list.values()].map((k) => { try { return new URL(k.url).origin.replace(/^http:/, 'https:'); } catch { return ''; } }).filter(Boolean));
    for (const origin of origins) { try { await ses.clearStorageData({ origin, storages: ['localstorage', 'cachestorage', 'serviceworkers'] }); } catch {} }
  }
  try { await ses.cookies.flushStore(); } catch {}
  return gone;
}

module.exports = { appUiContents, isPrintKey, printPage, syncRequest, prepareSession, forgetXtimeSignin };
