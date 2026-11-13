// Labor Times, talking to SLTS: SLTS works by posting its form back to itself, so the last page Ford sent is kept and
// the next request posts its form back, filled in the way SLTS's own controls would.
'use strict';

// ---------- talking to SLTS ----------
let page = null; // { doc, url } — the last SLTS page, whose form the next request posts back
let busy = false;

// Ford's inline styles and scripts aren't needed, and this page's security rules would flag each one
const stripStyles = (html) => String(html).replace(/<(script|style)\b[\s\S]*?<\/\1>/gi, '').replace(/\sstyle\s*=\s*(?:"[^"]*"|'[^']*')/gi, '');
const parse = (html) => new DOMParser().parseFromString(stripStyles(html), 'text/html');
function setPage(res) {
  const doc = parse(res.html);
  page = { doc, url: res.url };
  return doc;
}
function formAction() {
  const f = page.doc.querySelector('form');
  try { return new URL(f?.getAttribute('action') || page.url, page.url).href; } catch { return page.url; }
}
// Every field SLTS's form would send (no image buttons: those are added for a click)
function formFields() {
  const out = {};
  const f = page.doc.querySelector('form');
  if (!f) return out;
  for (const n of f.querySelectorAll('input, select, textarea')) {
    const name = n.getAttribute('name');
    if (!name) continue;
    const type = (n.getAttribute('type') || '').toLowerCase();
    if (n.tagName === 'SELECT') {
      // Like a browser: a list box (size > 1) with nothing picked, or an empty list, sends nothing;
      // sending '' for it makes ASP.NET reject the whole post ("Invalid postback argument")
      const listBox = n.hasAttribute('multiple') || Number(n.getAttribute('size') || 1) > 1;
      const opt = n.querySelector('option[selected]') || (listBox ? null : n.querySelector('option'));
      if (opt) out[name] = opt.getAttribute('value') ?? opt.textContent;
    } else if (type === 'image' || type === 'submit' || type === 'button' || type === 'reset') continue;
    else if (type === 'checkbox' || type === 'radio') { if (n.hasAttribute('checked')) out[name] = n.getAttribute('value') || 'on'; }
    else out[name] = n.getAttribute('value') || n.textContent || '';
  }
  return out;
}
async function request(req) {
  const res = await api.slts(req);
  if (res?.error) { const e = new Error(res.message || 'SLTS failed'); e.needsLogin = res.needsLogin; throw e; }
  // An SLTS page without its form is an error page: put what it says in the log
  if (!/<form\b/i.test(res.html || '')) {
    const title = (String(res.html).match(/<title>([\s\S]*?)<\/title>/i) || [])[1] || '';
    const text = String(res.html).replace(/<(script|style)\b[\s\S]*?<\/\1>/gi, ' ').replace(/<[^>]*>/g, ' ').replace(/\s+/g, ' ').trim();
    api.log(`labor: SLTS sent a page with no form (${clean(title)}): ${text.slice(0, 500)}`);
  }
  return res;
}
async function start() {
  setPage(await request({ op: 'start' }));
}
// Post the form back: a dropdown change (eventTarget) or the "View" button (click)
async function postBack(overrides, { eventTarget = '', eventArgument = '', click = false } = {}) {
  if (!page) await start();
  const fields = { ...formFields(), ...overrides, __EVENTTARGET: eventTarget, __EVENTARGUMENT: eventArgument };
  if (click) { fields['ImageButton_View_SearchFor.x'] = '9'; fields['ImageButton_View_SearchFor.y'] = '9'; }
  const res = await request({ op: 'post', method: 'POST', url: formAction(), fields });
  const doc = setPage(res);
  if (!doc.getElementById('DropDownList_SearchType')) { // the session ran out: start over once
    await start();
    const again = await request({ op: 'post', method: 'POST', url: formAction(), fields: { ...formFields(), ...overrides, __EVENTTARGET: eventTarget, __EVENTARGUMENT: '', ...(click ? { 'ImageButton_View_SearchFor.x': '9', 'ImageButton_View_SearchFor.y': '9' } : {}) } });
    setPage(again);
  }
  return page.doc;
}
