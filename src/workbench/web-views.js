// Web pages as tabs (Xtime, PTS, ProDemand, Ford's articles): links that open new windows, Ford's PDFs (an FSA's
// technical instructions, bulletin…) as their own tabs, PTS tabs set up for the vehicle, ProDemand's VIN box.
'use strict';
const path = require('path');
const { clipboard, WebContentsView } = require('electron');
const { ASSET_HOSTS, PTS: PTS_ORIGIN } = require('../manual');
const fs = require('fs');
const { pathToFileURL } = require('url');
const { FORD_LINKS, PRODEMAND_FILL, sleep } = require('../app/config');
const { profileFile } = require('../app/files');
const { applySnapshot } = require('../app/snapshot');
const SRC = path.join(__dirname, '..'); // the app's src folder

module.exports = function install(my) {
  Object.assign(my, { hardenWebContents, makeWebView, ensureContentCookies, ensureView, reopenFordTab, fillProDemand }); // used by the window's other parts

  // ---------- web views ----------
  function hardenWebContents(wc) {
    // Popups (e.g. wiring diagrams) open as normal windows that share this window's login.
    wc.setWindowOpenHandler(windowOpenRule);
    wc.on('will-attach-webview', (e) => e.preventDefault());
  }
  function windowOpenRule({ url }) {
      if (!/^https?:/i.test(url)) return { action: 'deny' };
      return {
        action: 'allow',
        overrideBrowserWindowOptions: {
          autoHideMenuBar: true, title: `PTS Workbench - ${my.displayName()}`,
          webPreferences: {
            partition: my.PARTITION, sandbox: true, contextIsolation: true, plugins: true, // plugins = PDF viewer
            preload: path.join(SRC, 'web-preload.js'), nodeIntegrationInSubFrames: true,
          },
        },
      };
  }

  function makeWebView() {
    const view = new WebContentsView({
      webPreferences: {
        partition: my.PARTITION, sandbox: true, contextIsolation: true, nodeIntegration: false, plugins: true, // plugins = PDF viewer
        preload: path.join(SRC, 'web-preload.js'), nodeIntegrationInSubFrames: true,
      },
    });
    my.win.contentView.addChildView(view);
    view.setVisible(false);
    hardenWebContents(view.webContents);
    return view;
  }

  const fordLink = (kind, vin) => my.settings.ptsTransportUrl
    .replace('{LINK}', FORD_LINKS[kind] || FORD_LINKS.pts)
    .replace('{VIN}', encodeURIComponent(vin));
  const hostOf = (url) => { try { return new URL(url).hostname; } catch { return ''; } };
  my.hostOf = hostOf;

  // ---------- Ford's PDFs (an FSA's technical instructions, bulletin, owner letter…) ----------
  // Ford's document server refuses ("Access Denied") PDFs opened in the app's own browser tabs, but
  // hands them to the app's PTS connection (the one that loads Workshop Manual pictures). So a PDF link
  // in an FSA page is downloaded that way and shown from this computer, as its own Service Manual tab.
  const isFordPdf = (url) => { try { const u = new URL(url); return u.protocol === 'https:' && ASSET_HOSTS.test(u.hostname) && /\.pdf$/i.test(u.pathname); } catch { return false; } };
  const PDF_NAMES = [[/_TECHX?$/i, 'Technical instructions'], [/_BULL$/i, 'Dealer bulletin'], [/_MSRA$/i, 'Mobile repair assessment'],
    [/_DELIV_REC$/i, 'Pick-up & delivery record'], [/_Reimbursement/i, 'Reimbursement'], [/_Owner_Letter_SPN$/i, 'Owner letter (Spanish)'],
    [/_Owner_Letter/i, 'Owner letter']];
  function pdfTitle(url) {
    const file = decodeURIComponent(new URL(url).pathname.split('/').pop() || 'Document').replace(/\.pdf$/i, '');
    const id = (file.match(/^(\d{2}[A-Z]\d{2})_/i) || [])[1];
    const known = PDF_NAMES.find(([re]) => re.test(file));
    return (known ? `${id ? `${id} ` : ''}${known[1]}` : file.replace(/_/g, ' ')).slice(0, 40);
  }
  function catchFordPdfs(ws, wc) {
    // a link that loads the PDF into the page (Ford's FSA page shows them in its right-hand frame)
    wc.on('will-frame-navigate', (e) => {
      if (!isFordPdf(e.url)) return;
      e.preventDefault();
      // The frame asking can already be gone (Ford's page swaps its PDF frame out after the first document):
      // reading it then throws "Render frame was disposed", so the page's own address is used instead
      let referrer = '';
      try { referrer = e.frame?.parent?.url || e.frame?.url || ''; } catch { /* frame gone */ }
      try { if (!referrer && !wc.isDestroyed()) referrer = wc.getURL(); } catch { /* page gone */ }
      openFordPdf(ws, e.url, referrer).catch((err) => my.manualService.log(`PDF ${e.url.slice(0, 160)}: ${err.message || err}`));
    });
    // a link that opens the PDF in a new window
    wc.setWindowOpenHandler((details) => {
      if (isFordPdf(details.url)) {
        let referrer = '';
        try { referrer = details.referrer?.url || (!wc.isDestroyed() ? wc.getURL() : ''); } catch { /* page gone */ }
        openFordPdf(ws, details.url, referrer).catch((err) => my.manualService.log(`PDF ${details.url.slice(0, 160)}: ${err.message || err}`));
        return { action: 'deny' };
      }
      return windowOpenRule(details);
    });
  }
  // Ford's document server lets you in with CONTENT_AUTH / CONTENT_PERMISSIONS cookies on fordservicecontent.com.
  // The app gets them the way PTS's home page does when they're missing or out of date. Only names (never
  // values) are logged.
  async function contentCookies() {
    const all = await my.manualSes.cookies.get({}).catch(() => []);
    return all.filter((c) => /^CONTENT_(AUTH|PERMISSIONS)$/i.test(c.name) && (!c.expirationDate || c.expirationDate * 1000 > Date.now() + 60 * 1000));
  }
  // PTS's home page carries a one-time signed address on sso.fordservicecontent.com (a script tag that
  // answers akamaiCallback(...)); loading it is what hands out the cookies. CheckCookie only checks them.
  async function ensureContentCookies(force = false) {
    if (!force && (await contentCookies()).some((c) => /AUTH/i.test(c.name))) return true;
    const where = (u) => { try { const x = new URL(u); return `${x.host}${x.pathname}`; } catch { return '?'; } };
    try {
      const home = await my.manualService.text(`${PTS_ORIGIN}/`, { headers: { Accept: 'text/html,*/*' } });
      const srcs = [...home.matchAll(/<script\b[^>]*\bsrc\s*=\s*["']([^"']+)["']/gi)].map((m) => m[1].replace(/&amp;/g, '&'));
      const sso = srcs.filter((u) => /fordservicecontent\.com/i.test(u) && !/CheckCookie/i.test(u) && /^https:/i.test(u))
        .concat(srcs.filter((u) => /akamaiCallback/i.test(u) && !/CheckCookie/i.test(u)));
      if (!sso.length) {
        my.manualService.log(`content sign-on: no sign-on address on PTS's home page; its scripts: ${[...new Set(srcs.map(where))].join(', ').slice(0, 900)}`);
        return false;
      }
      for (const u of [...new Set(sso)].slice(0, 3)) {
        const abs = new URL(u, `${PTS_ORIGIN}/`).href;
        const res = await my.manualSes.fetch(abs, { credentials: 'include', referrer: `${PTS_ORIGIN}/`, headers: { Accept: 'text/javascript, application/javascript, */*' } });
        const body = await res.text();
        my.manualService.log(`content sign-on ${where(abs)} → ${res.status}: ${body.replace(/\s+/g, ' ').slice(0, 80)}`);
      }
      const have = await contentCookies();
      my.manualService.log(`   now have: ${have.map((c) => c.name).join(', ') || 'none'}`);
      return have.some((c) => /AUTH/i.test(c.name));
    } catch (e) {
      my.manualService.log(`content sign-on failed: ${e.message || e}`);
      return false;
    }
  }
  async function openFordPdf(ws, url, referrer) {
    const title = pdfTitle(url);
    const existing = ws.tabs.find((t) => t.kind === 'doc' && t.src === url);
    if (existing) { my.setActive(ws, existing); my.layout(); my.pushState(); return; }
    my.notice(`Opening ${title}…`);
    try {
      await ensureContentCookies();
      const get = () => my.manualService.fetch(url, { headers: { Accept: 'application/pdf,*/*' }, ...(referrer && /^https:/i.test(referrer) ? { referrer } : {}) });
      let res;
      try { res = await get(); } catch (e) {
        if (!e.needsLogin) throw e;
        // refused: get fresh cookies from Ford's sign-on and try once more
        if (!(await ensureContentCookies(true))) throw e;
        res = await get();
      }
      const buf = Buffer.from(await res.arrayBuffer());
      if (buf.subarray(0, 5).toString() !== '%PDF-') {
        my.manualService.log(`   not a PDF (${buf.length} bytes): ${buf.subarray(0, 200).toString().replace(/\s+/g, ' ')}`);
        throw new Error("Ford didn't send the document");
      }
      const dir = profileFile(my.profile.id, 'docs');
      fs.mkdirSync(dir, { recursive: true });
      // keep the folder small: documents older than a week go
      for (const f of fs.readdirSync(dir)) { try { const p = path.join(dir, f); if (Date.now() - fs.statSync(p).mtimeMs > 7 * 24 * 3600 * 1000) fs.unlinkSync(p); } catch {} }
      const file = path.join(dir, decodeURIComponent(new URL(url).pathname.split('/').pop()).replace(/[^\w.-]+/g, '_') || 'document.pdf');
      fs.writeFileSync(file, buf);
      const t = my.addTab(ws, 'doc', { activate: false });
      t.src = url;
      t.url = pathToFileURL(file).href;
      t.title = title;
      ws.tabs.splice(ws.tabs.indexOf(t), 1);
      ws.tabs.splice(ws.tabs.map((x) => my.inService(x.kind)).lastIndexOf(true) + 1, 0, t);
      my.setActive(ws, t);
      my.saveWorkspaces(); my.layout(); my.pushState();
    } catch (e) {
      my.manualService.log(`PDF ${url.slice(0, 160)}: ${e.message || e}`);
      // Ford's document server wants its CONTENT_AUTH / CONTENT_PERMISSIONS cookies: does the app have them?
      // (names, domains and expiry only; never the values)
      try {
        const all = await my.manualSes.cookies.get({});
        const mine = all.filter((c) => /^CONTENT_/i.test(c.name) || /fordservicecontent/i.test(c.domain || ''));
        my.manualService.log(`   cookies for Ford's document server: ${mine.map((c) => `${c.name} (${c.domain}${c.path || ''}, ${c.expirationDate ? `expires ${new Date(c.expirationDate * 1000).toISOString()}` : 'session'})`).join('; ') || 'none'}`);
      } catch { /* best effort */ }
      my.notice(e.needsLogin ? `Ford refused ${title} (signed out of PTS, or access denied). Sign in to PTS and try again.` : `Couldn't open ${title}: ${e.message || e}`);
    }
  }

  function ensureView(ws, tab) {
    if (tab.view || tab.kind === 'service') return; // Service Manual is only a row of tabs
    if (ws.asleep) { ws.asleep = false; setTimeout(my.pushState, 0); } // woken up by being looked at
    if (tab.kind === 'manual' || tab.kind === 'pced') { tab.view = my.makeManualView(ws, tab); return; } // PC/ED: the same viewer, Ford's PC/ED book
    if (tab.kind === 'oasisview') { tab.view = my.makeManualView(ws, tab, 'oasis/oasis.html'); return; }
    if (tab.kind === 'diag') { tab.view = my.makeManualView(ws, tab, 'diagnostics/diag.html'); return; }
    if (tab.kind === 'tsb') { tab.view = my.makeManualView(ws, tab, 'tsb/tsb.html'); return; }
    if (tab.kind === 'labor') { tab.view = my.makeManualView(ws, tab, 'labor-times/labor.html'); return; }
    if (tab.kind === 'wiring') { tab.view = my.makeManualView(ws, tab, 'wiring/wiring.html'); return; }
    if (tab.kind === 'xview') { tab.view = my.makeManualView(ws, tab, 'xtime-view/xview.html'); return; }
    tab.view = makeWebView();
    const wc = tab.view.webContents;
    if (tab.kind === 'article') { // Ford's own page for an FSA (or other article) opened from OASIS, inside Service Manual
      tab.status = '';
      catchFordPdfs(ws, wc);
      wc.loadURL(tab.url);
    } else if (tab.kind === 'doc') { // a Ford PDF the app downloaded (see openFordPdf)
      tab.status = '';
      wc.loadURL(tab.url);
    } else if (FORD_LINKS[tab.kind]) {
      setupFordTab(ws, tab);
    } else if (tab.kind === 'xtime') {
      tab.status = '';
      // back from sleep: the Xtime page it was on (same Xtime site only)
      const back = tab.resumeUrl && hostOf(tab.resumeUrl) === hostOf(my.settings.xtimeRoUrl) ? tab.resumeUrl : '';
      tab.resumeUrl = null;
      wc.loadURL(back || my.settings.xtimeRoUrl.replace('{RO}', encodeURIComponent(ws.ro)));
    } else if (tab.kind === 'prodemand') {
      wc.on('did-finish-load', () => fillProDemand(ws, tab));
      tab.status = 'Loading ProDemand…';
      wc.loadURL(my.settings.prodemandUrl);
    } else {
      tab.status = '';
      wc.loadURL(my.settings.oasisUrl.replace('{VIN}', encodeURIComponent(ws.vin)));
    }
  }

  // PTS / OTA / Connected Data: open Ford's transport link for this VIN. If PTS sends you to its
  // sign-in page first, open the link again once you're back (at most once every 2 minutes).
  function setupFordTab(ws, tab) {
    const wc = tab.view.webContents;
    const ptsHost = hostOf(my.settings.ptsUrl);
    let viaLogin = false;
    let lastReopen = 0;
    wc.on('did-navigate', (_e, url) => {
      if (hostOf(url) && hostOf(url) !== ptsHost) { viaLogin = true; tab.status = 'Log in to PTS'; my.pushState(); }
    });
    wc.on('did-finish-load', async () => {
      if (hostOf(wc.getURL()) !== ptsHost) return;
      if (viaLogin && Date.now() - lastReopen > 2 * 60 * 1000) {
        viaLogin = false;
        lastReopen = Date.now();
        tab.status = 'Opening vehicle…'; my.pushState();
        wc.loadURL(fordLink(tab.kind, ws.vin));
        return;
      }
      viaLogin = false;
      const token = (tab.vinToken = (tab.vinToken || 0) + 1);
      if (tab.status) { tab.status = ''; my.pushState(); }
      if (tab.cloneSnapshot) {
        const snap = tab.cloneSnapshot;
        delete tab.cloneSnapshot; // only once; a later reload starts fresh
        tab.status = 'Copying page…'; my.pushState();
        await applySnapshot(tab, snap, token);
        if (tab.vinToken === token && tab.status === 'Copying page…') { tab.status = ''; my.pushState(); }
      }
    });
    tab.status = 'Loading PTS…';
    wc.loadURL(fordLink(tab.kind, ws.vin));
  }

  function reopenFordTab(ws, tab) {
    if (!tab.view || tab.view.webContents.isDestroyed()) return;
    tab.vinToken = (tab.vinToken || 0) + 1;
    tab.status = 'Opening vehicle…'; my.pushState();
    tab.view.webContents.loadURL(fordLink(tab.kind, ws.vin));
  }

  // ProDemand: copy the VIN, and type it into ProDemand's VIN box if one can be found
  async function fillProDemand(ws, tab) {
    const wc = tab.view?.webContents;
    if (!wc || wc.isDestroyed()) return;
    const token = (tab.vinToken = (tab.vinToken || 0) + 1);
    try { clipboard.writeText(ws.vin); } catch {}
    for (let attempt = 0; attempt < 10; attempt++) {
      if (tab.vinToken !== token || wc.isDestroyed()) return;
      try {
        if ((await wc.executeJavaScript(`${PRODEMAND_FILL}(${JSON.stringify(ws.vin)})`, true)) === 'filled') {
          tab.status = 'VIN entered (also copied)'; my.pushState(); return;
        }
      } catch { /* still loading */ }
      await sleep(1000);
    }
    if (tab.vinToken === token) { tab.status = 'VIN copied — paste it into ProDemand'; my.pushState(); }
  }
};
