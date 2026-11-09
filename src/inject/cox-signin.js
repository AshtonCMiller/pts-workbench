// Runs on Cox Automotive's own sign-in page (xtime.signin.coxautoinc.com) inside the hidden Xtime view.
// The app shows its own "Sign in to Xtime" form; this fills in Cox's real page and presses its buttons,
// so Cox's page still does all of the actual sign-in work (tokens, verification codes, redirects).
//   { type: 'state' }               → which step the page is on, any error shown, verification choices
//   { type: 'username', value }     → type the username, press Next
//   { type: 'password', value }     → type the password, press Sign in
//   { type: 'code', value }         → type a verification code, press Verify
//   { type: 'choose', index }       → pick a way to get a verification code
//   { type: 'press', text }         → press the page button with this text (e.g. Accept, Continue)
(async (ACTION) => {
  const $ = (q) => document.querySelector(q);
  const visible = (el) => {
    if (!el) return false;
    if (el.closest('.fakeHidden, [hidden]')) return false;
    const r = el.getBoundingClientRect();
    const cs = getComputedStyle(el);
    return r.width > 0 && r.height > 0 && cs.visibility !== 'hidden' && cs.display !== 'none';
  };
  const text = (el) => (el?.textContent || '').replace(/\s+/g, ' ').trim();
  // React keeps its own copy of input values: set it the way a keypress would
  const type = (el, value) => {
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set;
    el.focus();
    setter.call(el, String(value));
    el.dispatchEvent(new Event('input', { bubbles: true }));
    el.dispatchEvent(new Event('change', { bubbles: true }));
  };
  const wait = (ms) => new Promise((r) => setTimeout(r, ms));
  const press = async (btn) => {
    for (let i = 0; i < 20 && btn && btn.disabled; i++) await wait(100); // enabled once the input registers
    if (!btn || btn.disabled) return false;
    btn.click();
    return true;
  };
  const buttons = () => [...document.querySelectorAll('button, input[type=submit], a.btn')].filter(visible);
  const primaryIn = (root) => [...(root || document).querySelectorAll('button.btn-primary, button[type=submit], input[type=submit]')].find(visible);

  const codeInput = () => [...document.querySelectorAll('#input-verification-code, input[autocomplete="one-time-code"], input[name*="code" i], input[name*="passcode" i]')].find(visible);
  const factorChoices = () => {
    const form = $('#accountVerificationForm') || $('#accountVerificationOptionsForm') || $('#factorEnrollmentOptionsForm');
    if (!form || !visible(form)) return [];
    return [...form.querySelectorAll('button, [role=button], input[type=radio], a')].filter(visible)
      .filter((b) => !/back|cancel|return|sign in with a different/i.test(text(b) || b.value || ''));
  };
  const errorText = () => {
    const els = [...document.querySelectorAll('[role=alert], .alert, .Toastify__toast, .has-error .help-block, .error-message, .errorMessage, .text-danger, [class*="error" i]')]
      .filter(visible).map(text).filter((t) => t && t.length < 300);
    return [...new Set(els)].join(' ');
  };

  // The card's words without its buttons and boxes
  const pageText = (root) => {
    const copy = root.cloneNode(true);
    copy.querySelectorAll('button, input, select, textarea, script, style, svg').forEach((n) => n.remove());
    return text(copy);
  };
  const state = () => {
    const pw = $('#password');
    const user = $('#username');
    const code = codeInput();
    const choices = factorChoices();
    let step = 'other';
    if (code) step = 'code';
    else if (choices.length) step = 'choose';
    else if (pw && visible(pw)) step = 'password';
    else if (user && visible(user)) step = 'username';
    else if (/end user license|terms and conditions/i.test(document.body.innerText) && buttons().some((b) => /accept|agree/i.test(text(b)))) step = 'eula';
    else if (/already signed in|another session|duplicate session/i.test(document.body.innerText)) step = 'duplicate';
    const card = $('.panel-body') || document.body;
    return {
      ok: true,
      step,
      error: errorText(),
      username: user ? user.value : '',
      factors: choices.map((b) => text(b) || b.value || 'Option'),
      // what the page says, for steps the app doesn't handle itself
      message: step === 'other' || step === 'eula' || step === 'duplicate' || step === 'code' ? pageText(card).slice(0, 400) : '',
      buttons: buttons().map(text).filter((t) => t && t.length < 40).slice(0, 6),
    };
  };

  const a = ACTION || { type: 'state' };
  if (a.type === 'username') {
    const el = $('#username');
    if (!el || !visible(el)) return { ok: false, error: 'The username box isn’t showing' };
    type(el, a.value);
    if (!(await press($('#signIn')))) return { ok: false, error: 'Couldn’t press Next' };
  } else if (a.type === 'password') {
    const el = $('#password');
    if (!el || !visible(el)) return { ok: false, error: 'The password box isn’t showing' };
    type(el, a.value);
    if (!(await press($('#signIn') || primaryIn(el.form)))) return { ok: false, error: 'Couldn’t press Sign in' };
  } else if (a.type === 'code') {
    const el = codeInput();
    if (!el) return { ok: false, error: 'The code box isn’t showing' };
    type(el, a.value);
    if (!(await press(primaryIn($('#otpValidationButtons')) || primaryIn(el.form) || primaryIn())))
      return { ok: false, error: 'Couldn’t press Verify' };
  } else if (a.type === 'choose') {
    const b = factorChoices()[a.index];
    if (!b) return { ok: false, error: 'That option isn’t showing any more' };
    b.click();
  } else if (a.type === 'press') {
    const b = buttons().find((x) => text(x).toLowerCase() === String(a.text).toLowerCase());
    if (!b) return { ok: false, error: 'That button isn’t showing any more' };
    b.click();
  }
  if (a.type !== 'state') await wait(400);
  return state();
})