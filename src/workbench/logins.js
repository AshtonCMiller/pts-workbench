// Saved logins for this window only, encrypted with the Windows account (safeStorage): filled in on Xtime / PTS /
// ProDemand sign-in pages, and usernames remembered for two-step sign-ins.
'use strict';
const { safeStorage } = require('electron');

module.exports = function install(my) {
  Object.assign(my, { getLogin, saveLogin, rememberUsername }); // used by the window's other parts

  // ---------- saved logins (this window only; encrypted with the Windows account) ----------
  let pendingUser = null; // username typed on the first step of a two-step sign-in
  const autoTries = new Map(); // "host:step" -> last time "Sign in" was pressed for you
  const readLogins = () => my.readJson('logins.json', []);
  my.readLogins = readLogins;
  const canEncrypt = () => { try { return safeStorage.isEncryptionAvailable(); } catch { return false; } };
  my.canEncrypt = canEncrypt;

  function getLogin(host, identifierOnly) {
    if (!canEncrypt()) return null;
    const rec = readLogins().find((l) => l.host === host);
    if (!rec) return null;
    let password = '';
    if (!identifierOnly) {
      try { password = safeStorage.decryptString(Buffer.from(rec.pw, 'base64')); } catch { return null; }
    }
    // Press "Sign in" at most once per 10 minutes per page, so a wrong password can't loop
    const key = `${host}:${identifierOnly ? 'user' : 'pass'}`;
    const autoSubmit = !!my.settings.autoLogin && Date.now() - (autoTries.get(key) || 0) > 10 * 60 * 1000;
    if (autoSubmit) autoTries.set(key, Date.now());
    return { username: rec.username || '', password, autoSubmit };
  }

  function saveLogin(host, username, password) {
    if (!password || !canEncrypt()) return;
    if (!username && pendingUser?.host === host && Date.now() - pendingUser.at < 10 * 60 * 1000) username = pendingUser.username;
    const list = readLogins();
    const old = list.find((l) => l.host === host);
    if (old && old.username === username) {
      try { if (safeStorage.decryptString(Buffer.from(old.pw, 'base64')) === password) return; } catch { /* replace it */ }
    }
    const rec = { host, username, pw: safeStorage.encryptString(password).toString('base64'), savedAt: Date.now() };
    my.writeJson('logins.json', [...list.filter((l) => l.host !== host), rec]);
    my.notice(`Saved login for ${host}`, { removeHost: host });
    my.pushState();
  }

  function rememberUsername(host, username) {
    if (username) pendingUser = { host, username: String(username).slice(0, 200), at: Date.now() };
  }
};
