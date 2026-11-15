// Talks to Xtime directly from the app (no scripts run inside Xtime's pages): plain web requests to the Inspect site
// with your Xtime login (the window's cookies), the way Xtime's own page makes them. A Rust version would do the same
// with an HTTP client and a cookie jar.
//
// The requests themselves are in this folder, one file each:
//   dashboard.js  your ROs from Xtime's dashboard (RO, tag, vehicle, mode, waiting flags, dashboard columns)
//   employee.js   your Xtime technician ID from your employee number
//   vehicle.js    one RO's vehicle (VIN, year/make/model)
//   details.js    one RO's lines (for hours and flags)
//   ro.js         one RO for the Xtime View tab, and marking a line done
//   inspect.js    the inspection form, and adding / editing / deleting lines
//   chat.js       the Dealership chat: conversations, unread count, sending, read receipts (with xml.js, websocket.js)
// Each takes this client (x) first: x.fetch(url, options) works like the page's fetch (addresses on the Inspect site
// can be written as "/api/…"), x.xsrf() is Xtime's anti-forgery token, x.mem is remembered between calls.
// Customer information is never kept: each request returns only what the app shows.
'use strict';

const MODULES = {
  dashboard: require('./dashboard'),
  employee: require('./employee'),
  vehicle: require('./vehicle'),
  details: require('./details'),
  ro: require('./ro'),
  inspect: require('./inspect'),
  chat: require('./chat'),
};

class XtimeClient {
  // ses: the window's session (its cookies are the Xtime login); settings(): the window's settings (xtimeDataHost);
  // log(line): the window's xtime.log
  constructor(ses, settings, log) {
    this.ses = ses;
    this.settings = settings;
    this.log = log || (() => {});
    this.mem = {}; // the store's staff list, dispatch codes, inspection form scripts… (what Xtime's page keeps in memory)
  }

  host() { return String(this.settings().xtimeDataHost || 'www.inspect.app.coxautoinc.com'); }
  base() { return `https://${this.host()}/`; }

  // Xtime's anti-forgery token (its page sends it with every change as X-XSRF-TOKEN)
  async xsrf() {
    try {
      const [c] = await this.ses.cookies.get({ url: this.base(), name: 'XSRF-TOKEN' });
      return c ? decodeURIComponent(c.value) : '';
    } catch { return ''; }
  }

  // Like the page's fetch: the address may be relative to the Inspect site; your Xtime cookies go with it.
  // Answers { ok, status, url, redirected, headers, json(), text() }. "redirected" is also true when Xtime sent the
  // request on to another site (its sign-in pages), which is how a run-out login shows.
  async fetch(url, opts = {}) {
    const abs = new URL(url, this.base());
    if (abs.protocol !== 'https:') throw new Error('Xtime addresses must be https');
    const method = String(opts.method || 'GET').toUpperCase();
    const headers = { ...(opts.headers || {}) };
    headers.Referer ||= this.base();
    // changes carry the Inspect site as their origin, as they do from Xtime's page (also to Xtime's login and chat services)
    if (method !== 'GET' && method !== 'HEAD') headers.Origin ||= `https://${this.host()}`;
    const t0 = Date.now();
    let res;
    try {
      res = await this.ses.fetch(abs.href, { method, headers, body: opts.body, credentials: 'include', redirect: 'follow', cache: 'no-store' });
    } catch (e) {
      this.log(`direct ${method} ${abs.pathname} → failed: ${e.message || e}`);
      throw e;
    }
    let endHost = '';
    try { endHost = new URL(res.url || abs.href).host; } catch {}
    const redirected = !!res.redirected || (endHost && endHost !== abs.host);
    // only what went wrong is written down (the dashboard is asked every minute)
    if (!res.ok || redirected) this.log(`direct ${method} ${abs.pathname} → ${res.status}${redirected ? ` (sent on to ${endHost})` : ''} ${Date.now() - t0} ms`);
    return {
      ok: res.ok, status: res.status, url: res.url, redirected, headers: res.headers,
      json: () => res.json(), text: () => res.text(),
    };
  }

  // Runs one of the requests above: xtime.run('ro', store, ro, { type: 'view' }) → what it answers
  async run(name, ...args) {
    const fn = MODULES[name];
    if (!fn) throw new Error(`No Xtime request called ${name}`);
    try { return await fn(this, ...args); } catch (e) {
      this.log(`direct ${name}: ${e && e.stack || e}`);
      return { ok: false, error: `Xtime request failed: ${e.message || e}` };
    }
  }

  // Forget what's remembered (after signing in again as someone else, or another store)
  reset() { this.mem = {}; }
}

module.exports = { XtimeClient };
