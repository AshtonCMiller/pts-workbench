// Runs inside the Xtime / PTS / ProDemand pages, isolated from the pages' own scripts.
// It remembers logins: when you sign in, it hands the username and password to the app to save
// (encrypted with your Windows account); next time it fills them in, and can press "Sign in" for you.
// The page itself can't see or call any of this.
const { ipcRenderer } = require('electron');

// Only pages that look like sign-in pages get a username filled on its own, so customer
// "email" fields elsewhere in Xtime are never touched.
const LOGIN_PAGE = /log-?in|sign-?in|logon|auth|sso|okta|oauth|saml|identity|adfs|wsl|account|idp/i;
const looksLikeLoginPage = () => LOGIN_PAGE.test(location.host + location.pathname);

const visible = (e) => !!e && e.isConnected && !e.disabled && e.type !== 'hidden'
  && (e.offsetWidth > 0 || e.offsetHeight > 0 || e.getClientRects().length > 0);
const USER_SEL = 'input[type="email"], input[type="text"], input[type="tel"], input:not([type])';
const SKIP_USER = /search|captcha|otp|one.?time|code|token|pin|vin|zip|phone/i;
const USERISH = /user|login|logon|email|e-mail|ident|account|loginfmt|cdsid|wslx|sign/i;
const describe = (i) => [i.name, i.id, i.autocomplete, i.placeholder, i.getAttribute('aria-label')].join(' ');

function findFields() {
  const pws = [...document.querySelectorAll('input[type="password"]')].filter(visible);
  if (pws.length > 1) return { pw: null, user: null, many: true }; // change-password form: leave it alone
  const pw = pws[0] || null;
  const scope = pw?.form || document;
  const users = [...scope.querySelectorAll(USER_SEL)].filter(visible).filter((i) => !SKIP_USER.test(describe(i)));
  let user = null;
  if (pw) {
    // the text box just before the password box
    const before = users.filter((u) => u.compareDocumentPosition(pw) & Node.DOCUMENT_POSITION_FOLLOWING);
    user = before[before.length - 1] || null;
  } else if (looksLikeLoginPage()) {
    user = users.find((u) => USERISH.test(describe(u))) || (users.length === 1 ? users[0] : null);
  }
  return { pw, user };
}

function setValue(input, value) {
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set;
  input.focus();
  setter.call(input, value);
  input.dispatchEvent(new Event('input', { bubbles: true }));
  input.dispatchEvent(new Event('change', { bubbles: true }));
}

function submit(field) {
  const scope = field.form || document;
  const byType = [...scope.querySelectorAll('button[type="submit"], input[type="submit"]')].find(visible);
  const byText = [...scope.querySelectorAll('button, input[type="button"], [role="button"]')]
    .find((b) => visible(b) && /sign\s*in|log\s*in|log\s*on|next|continue|submit|verify/i.test(b.textContent || b.value || ''));
  const btn = byType || byText;
  if (btn) btn.click();
  else if (field.form?.requestSubmit) field.form.requestSubmit();
  else field.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', code: 'Enter', keyCode: 13, bubbles: true }));
}

// ---------- fill ----------
const handled = new WeakSet();
let asking = false;
async function tryFill() {
  if (asking) return;
  const { pw, user } = findFields();
  const target = pw || user;
  if (!target || handled.has(target)) return;
  handled.add(target);
  asking = true;
  let cred = null;
  try { cred = await ipcRenderer.invoke('cred-get', { identifierOnly: !pw }); } catch { /* no saved login */ }
  asking = false;
  if (!cred) return;
  if (user && cred.username && !user.value) setValue(user, cred.username);
  if (pw && cred.password && !pw.value) setValue(pw, cred.password);
  const ready = pw ? !!pw.value : !!user?.value;
  if (cred.autoSubmit && ready) setTimeout(() => submit(pw || user), 500);
}

// ---------- capture ----------
let lastSent = '';
function capture() {
  const { pw, user } = findFields();
  if (pw && pw.value) {
    const key = `${user?.value || ''}\u0000${pw.value}`;
    if (key === lastSent) return;
    lastSent = key;
    ipcRenderer.send('cred-save', { username: user?.value || '', password: pw.value });
  } else if (!pw && user && user.value) {
    ipcRenderer.send('cred-username', { username: user.value }); // first step of a two-step sign-in
  }
}
document.addEventListener('submit', capture, true);
document.addEventListener('click', (e) => {
  if (e.target?.closest?.('button, input[type="submit"], input[type="button"], [role="button"], a')) capture();
}, true);
document.addEventListener('keydown', (e) => { if (e.key === 'Enter' && e.target?.tagName === 'INPUT') capture(); }, true);

// ---------- watch for sign-in boxes (they often appear after the page loads) ----------
let timer = null;
const soon = () => { clearTimeout(timer); timer = setTimeout(tryFill, 300); };
window.addEventListener('DOMContentLoaded', () => {
  soon();
  new MutationObserver(soon).observe(document.documentElement, {
    childList: true, subtree: true, attributes: true, attributeFilter: ['style', 'class', 'type', 'hidden'],
  });
});