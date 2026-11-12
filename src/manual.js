// Workshop manual data, fetched straight from Ford with this window's PTS login (no PTS page needed):
//   1. /VehicleId/DecodeVehicle?vin=…   → PTS's vehicle ID, year, model and option codes
//   2. /publication/Book/workshop/…     → the manual's settings for that vehicle (book code, etc.)
//   3. TreeAndCover (POST)              → the table of contents
//   4. /publication/proc?…              → one procedure, as HTML
// The app's own viewer (manual/viewer.html) draws all of this in its own style.
'use strict';
const fs = require('fs');
const path = require('path');

const PTS = 'https://www.fordtechservice.dealerconnection.com';
const ASSET_HOSTS = /(^|\.)(dealerconnection\.com|fordservicecontent\.com)$/i;

class NeedsLogin extends Error {
  constructor() { super('Sign in to PTS to open the manual'); this.needsLogin = true; }
}

class ManualService {
  constructor(ses, cacheDir, logFile) {
    this.ses = ses;
    this.cacheDir = cacheDir;
    this.logFile = logFile || null;
    this.books = new Map(); // vin -> Promise<book>
    this.trees = new Map(); // cache key -> Promise<html>
    this.procs = new Map(); // url -> html (this session only)
  }

  // A short record of each request (address, status, where it ended up), for working out problems
  log(line) {
    if (!this.logFile) return;
    try {
      try { if (fs.statSync(this.logFile).size > 400000) fs.rmSync(this.logFile, { force: true }); } catch {}
      fs.mkdirSync(path.dirname(this.logFile), { recursive: true });
      fs.appendFileSync(this.logFile, `${new Date().toISOString()} ${line}\n`);
    } catch { /* logging is optional */ }
  }

  async fetch(url, opts = {}) {
    let res;
    const short = url.length > 220 ? `${url.slice(0, 220)}…` : url;
    try {
      res = await this.ses.fetch(url, { credentials: 'include', redirect: 'follow', ...opts });
    } catch (e) {
      this.log(`${opts.method || 'GET'} ${short} → failed: ${e.message || e}`);
      throw new Error(`Couldn't reach Ford (${e.message || e})`);
    }
    this.log(`${opts.method || 'GET'} ${short} → ${res.status} ${res.headers.get('content-type') || ''}` +
      `${res.url && res.url !== url ? ` (ended at ${res.url.slice(0, 160)})` : ''}`);
    // PTS sends you to its sign-in pages when the login has run out
    let host = '';
    try { host = new URL(res.url || url).hostname; } catch {}
    if (res.status === 401 || res.status === 403 || (host && !ASSET_HOSTS.test(host))) throw new NeedsLogin();
    if (!res.ok) throw new Error(`Ford returned an error (${res.status})`);
    return res;
  }

  async text(url, opts) {
    const res = await this.fetch(url, opts);
    const text = await res.text();
    this.log(`   ${text.length} characters: ${text.replace(/\s+/g, ' ').slice(0, 160)}`);
    if (/<form[^>]+(login|signin|sso|adfs)/i.test(text) && !/var params\s*=|contentDiv|tree/i.test(text)) throw new NeedsLogin();
    return text;
  }

  // ---- 1. the vehicle ----
  async decode(vin) {
    const res = await this.fetch(`${PTS}/VehicleId/DecodeVehicle?vin=${encodeURIComponent(vin)}`, {
      headers: { Accept: 'application/json, text/javascript, */*; q=0.01', 'X-Requested-With': 'XMLHttpRequest' },
    });
    let d;
    const body = await res.text();
    try { d = JSON.parse(body); } catch {
      this.log(`   not JSON: ${body.replace(/\s+/g, ' ').slice(0, 200)}`);
      throw /login|sign ?in/i.test(body) ? new NeedsLogin() : new Error("PTS didn't recognise this VIN");
    }
    const info = d.DecodedInfo || {};
    this.log(`   vehicle fields: ${Object.keys(d).slice(0, 40).join(',')} | DecodedInfo: ${Object.keys(info).slice(0, 40).join(',')}` +
      ` | VehicleID=${d.VehicleID} MasterId=${info.MasterId} Year=${info.Year}`);
    if (!info.Year || String(info.Year) === '0') throw new Error("PTS didn't recognise this VIN");
    // Only what the manual needs: no warranty, owner or dealer details are kept
    return {
      vin: String(d.Vin || vin).toUpperCase(),
      // MasterId is the manual's vehicle ID (VehicleID holds a line code like "CHD"); numbers first
      ids: [info.MasterId, d.VehicleID, d.VehicleId, info.VehicleId, info.vid]
        .filter((x) => x != null && String(x) !== '' && String(x) !== '0').map(String)
        .sort((x, y) => (/^\d+$/.test(y) ? 1 : 0) - (/^\d+$/.test(x) ? 1 : 0)),
      year: String(info.Year),
      displayName: info.DefaultDisplayName || info.MarketName || '',
      marketName: info.MarketName || info.DefaultDisplayName || '',
      buildDate: d.BuildDate || '',
      productType: d.ProductType || '',
      vehicleLine: d.VehicleLine || '',
    };
  }

  // ---- 2. the manual's settings for this vehicle ----
  book(vin) {
    vin = String(vin).toUpperCase();
    if (!this.books.has(vin)) {
      const p = this.loadBook(vin);
      this.books.set(vin, p);
      p.catch(() => this.books.delete(vin)); // try again next time
    }
    return this.books.get(vin);
  }

  async loadBook(vin) {
    const v = await this.decode(vin);
    if (!v.ids.length) throw new Error("PTS didn't return a vehicle ID for this VIN");
    let lastError = null;
    for (const id of [...new Set(v.ids)]) {
      const url = `${PTS}/publication/Book/workshop/${encodeURIComponent(id)}/${encodeURIComponent(vin)}` +
        `?model=${encodeURIComponent(v.displayName)}&modelyear=${encodeURIComponent(v.year)}`;
      try {
        const html = await this.text(url);
        const book = parseBookPage(html);
        if (!book.params.vehicleId || !book.bases.publicationBookTreeAndCoverBase) throw new Error('No manual for this vehicle');
        return {
          vin, url,
          title: book.params.bookTitle || `${v.year} ${v.marketName}`,
          vehicle: { year: v.year, name: v.marketName, buildDate: v.buildDate, productType: v.productType, vehicleLine: v.vehicleLine },
          params: book.params,
          bases: book.bases,
        };
      } catch (e) {
        if (e.needsLogin) throw e;
        this.log(`   vehicle ID ${id} didn't work: ${e.message}`);
        lastError = e;
      }
    }
    throw lastError || new Error('No manual for this vehicle');
  }

  // ---- 3. table of contents (kept on disk; a new manual release gets a new file) ----
  async tree(vin) {
    const book = await this.book(vin);
    const p = book.params;
    const key = `${p.vehicleId}_${p.bookcode}_${p.environment || ''}_${vin}`.replace(/[^A-Za-z0-9_.-]/g, '');
    const file = this.cacheDir ? path.join(this.cacheDir, `tree_${key}.html`) : null;
    if (!this.trees.has(key)) {
      const load = (async () => {
        if (file) {
          try {
            const st = fs.statSync(file);
            if (Date.now() - st.mtimeMs < 7 * 24 * 3600 * 1000) return fs.readFileSync(file, 'utf8');
          } catch { /* not cached yet */ }
        }
        const html = await this.fetchTree(book);
        if (file) {
          try {
            fs.mkdirSync(this.cacheDir, { recursive: true });
            for (const f of fs.readdirSync(this.cacheDir)) { // drop old copies for this VIN
              if (f.startsWith('tree_') && f.endsWith(`_${vin}.html`) && f !== path.basename(file)) fs.rmSync(path.join(this.cacheDir, f), { force: true });
            }
            fs.writeFileSync(file, html);
          } catch { /* cache is optional */ }
        }
        return html;
      })();
      this.trees.set(key, load);
      load.catch(() => this.trees.delete(key));
    }
    return this.trees.get(key);
  }

  async fetchTree(book) {
    const p = book.params;
    const url = `${book.bases.publicationBookTreeAndCoverBase}?bookTitle=${encodeURIComponent(p.bookTitle || '')}` +
      `&WiringBookTitle=${encodeURIComponent(p.WiringBookTitle || '')}`;
    // Same fields Ford's own manual page sends
    const form = new URLSearchParams({
      isMobile: p.isMobile || 'no',
      vin: p.vin || book.vin,
      vehicleId: p.vehicleId,
      modelYear: String(parseInt(p.modelYear, 10) || ''),
      searchNumber: '0',
      channel: p.channelId || '',
      category: p.categoryId || '',
      CategoryDescription: encodeURIComponent(p.categorydesc || ''),
      book: p.bookcode || '',
      booktype: p.booktype || '',
      country: p.country || '',
      language: p.language || '',
      contentmarket: p.contentmarket || '',
      contentlanguage: p.contentlang || '',
      languageOdysseyCode: p.languageOdysseyCode || '',
      contentgroup: p.contentgroup || '',
      WiringBookCode: p.WiringBookCode || '',
      WiringFormat: p.WiringFormat || '',
      fromPageBase: p.origin || PTS,
      strVehLine: '',
      strProdType: '',
    });
    const html = await this.text(url, {
      method: 'POST',
      body: form.toString(),
      headers: { 'Content-Type': 'application/x-www-form-urlencoded; charset=UTF-8', Accept: 'text/html, */*; q=0.01' },
      referrer: book.url,
    });
    if (!/class="tree"|procedure-link/.test(html)) {
      const e = new Error("Ford sent back something other than the manual's contents");
      e.detail = html.replace(/<[^>]+>/g, ' ').replace(/\s+/g, ' ').trim().slice(0, 300);
      throw e;
    }
    return html;
  }

  // ---- 4. one procedure ----
  procUrl(book, { searchNumber, procUid = '', linkType = 'Procedure' }) {
    const p = book.params;
    const q = [
      ['linkType', linkType], ['isMobile', p.isMobile || 'no'], ['userType', p.userType || 'Dealer'],
      ['vehicleId', p.vehicleId], ['modelYear', p.modelYear], ['searchNumber', searchNumber], ['procUid', procUid],
      ['adtLocation', ''], ['channel', p.channelId], ['categoryId', p.categoryId], ['bookCode', p.bookcode],
      ['bookType', p.booktype], ['country', p.country], ['language', p.language], ['contentMarket', p.contentmarket],
      ['contentLanguage', p.contentlang], ['languageOdysseyCode', p.languageOdysseyCode], ['adt', p.adt || ''],
      ['diagTool', p.diagTool || ''], ['otx', p.otx || ''], ['bookTitle', p.bookTitle || ''],
      ['wiringBookCode', p.WiringBookCode || ''], ['wiringBookTitle', p.WiringBookTitle || ''],
      ['wiringFormat', p.WiringFormat || ''], ['goToHash', ''],
    ].map(([k, v]) => `${k}=${encodeURIComponent(v == null ? '' : v)}`).join('&');
    return `${PTS}/publication/proc?${q}`;
  }

  // Where procedures come from: Ford's content server (the /publication/proc page is only an empty
  // shell that asks this address for the procedure)
  procContentUrl(book) {
    const p = book.params;
    const env = encodeURIComponent(p.environment || '');
    if (p.cprBasePath && p.cprBasePath.includes('{0}')) return p.cprBasePath.replace('{0}', `/publication/Proc?environment=${env}`);
    const tree = book.bases.publicationBookTreeAndCoverBase || '';
    const i = tree.indexOf('/publication/');
    if (i > 0) return `${tree.slice(0, i)}/publication/Proc?environment=${env}`;
    return `https://www.fordservicecontent.com/Ford_Content/PublicationRuntimeRefreshPTS//publication/Proc?environment=${env}`;
  }

  async proc(vin, link) {
    const book = await this.book(vin);
    const p = book.params;
    const key = `${link.linkType || 'Procedure'}|${link.searchNumber}|${link.procUid || ''}`;
    const url = this.procContentUrl(book);
    if (!this.procs.has(key)) {
      // Same fields Ford's procedure page sends
      const form = new URLSearchParams({
        isMobile: p.isMobile || 'no',
        usertype: p.userType || 'Dealer',
        userQsPilot: p.userQsPilot || '',
        vehicleId: p.vehicleId,
        modelYear: p.modelYear,
        channel: p.channelId || '',
        category: p.categoryId || '',
        book: p.bookcode || '',
        bookTitle: p.bookTitle || '',
        booktype: p.booktype || '',
        country: p.country || '',
        language: p.language || '',
        contentmarket: p.contentmarket || '',
        contentlanguage: p.contentlang || '',
        languageOdysseyCode: p.languageOdysseyCode || '',
        adt: p.adt || '',
        diagTool: p.diagTool || '',
        otx: p.otx || '',
        searchNumber: link.searchNumber,
        procUid: link.procUid || '',
        adtLocation: '',
        Vid: '',
        byvin: 'NO',
        marketGroup: p.marketGroup || '',
        WiringBookCode: p.WiringBookCode || '',
        WiringBookTitle: p.WiringBookTitle || '',
        WiringFormat: p.WiringFormat || '',
        fromPageBase: p.origin || PTS,
      });
      const html = await this.text(url, {
        method: 'POST',
        body: form.toString(),
        headers: { 'Content-Type': 'application/x-www-form-urlencoded; charset=UTF-8', Accept: 'text/html, */*; q=0.01' },
        referrer: this.procUrl(book, link),
      });
      if (!html.trim()) throw new Error('Ford sent this procedure back empty');
      // keep the latest one, for working out layouts the viewer doesn't handle yet
      if (this.cacheDir) { try { fs.mkdirSync(this.cacheDir, { recursive: true }); fs.writeFileSync(path.join(this.cacheDir, 'last-procedure.html'), html); } catch {} }
      this.procs.set(key, `<div id="contentDiv">${html}</div>`);
      if (this.procs.size > 60) this.procs.delete(this.procs.keys().next().value);
    }
    return { html: this.procs.get(key), base: url };
  }

  // OASIS for this VIN (Ford's server-built page; the app picks the sections it shows)
  // ---- loading ahead: when a vehicle is first opened (see main.js), its tabs' data is fetched in the
  //      background and kept until its tab asks for it once (a reload afterwards fetches fresh) ----
  takeAhead(key) {
    const hit = this.ahead?.get(key);
    if (!hit) return null;
    this.ahead.delete(key);
    return Date.now() - hit.at < 15 * 60 * 1000 ? hit.promise.catch(() => null) : null; // failed ahead: fetch it now
  }
  putAhead(key, fn) {
    if (!this.ahead) this.ahead = new Map();
    const promise = fn();
    this.ahead.set(key, { at: Date.now(), promise });
    promise.catch(() => { if (this.ahead.get(key)?.promise === promise) this.ahead.delete(key); });
    // keep it small: the 30 newest
    while (this.ahead.size > 30) this.ahead.delete(this.ahead.keys().next().value);
    return promise.then(() => true, () => false);
  }
  // skip: tab kinds already loaded (their data is on screen already)
  async prefetch(vin, { pacode = '', skip = new Set() } = {}) {
    const v = String(vin).toUpperCase();
    const t0 = Date.now();
    const fresh = (key) => { const hit = this.ahead?.get(key); return hit && Date.now() - hit.at < 15 * 60 * 1000; };
    const ahead = async (kind, key, label, fn) => {
      if (skip.has(kind) || fresh(key)) return;
      if (await this.putAhead(key, fn)) done.push(label); else failed.push(label);
    };
    const done = [], failed = [];
    try { await this.book(v); } catch (e) { this.log(`ahead ${v}: ${e.message || e}`); return; }
    if (!skip.has('manual')) await this.tree(v).then(() => done.push('manual contents'), () => failed.push('manual contents')); // kept on disk
    await ahead('oasisview', `oasis|${v}`, 'OASIS', () => this.loadOasis(v));
    await ahead('oasisview', `cv|${v}`, 'connected data', () => this.loadConnectedVehicle(v));
    if (pacode) await ahead('tsb', `tsb|${v}|${pacode}`, 'TSBs', () => this.loadTsbList(v, pacode));
    if (!skip.has('oasisview')) await this.svcForm(v).then(() => done.push('SVC list'), () => failed.push('SVC list'));
    await ahead('labor', `slts|${v}`, 'SLTS', () => this.loadSlts(v));
    this.log(`ahead ${v}: ${done.join(', ') || 'nothing new to fetch'}${failed.length ? ` (failed: ${failed.join(', ')})` : ''} in ${((Date.now() - t0) / 1000).toFixed(1)} s`);
  }

  async oasis(vin) {
    return (await this.takeAhead(`oasis|${String(vin).toUpperCase()}`)) || this.loadOasis(vin);
  }
  async loadOasis(vin) {
    const book = await this.book(vin);
    const p = book.params;
    const url = `${PTS}/oasis/Index?vin=${encodeURIComponent(book.vin)}&ModelYear=${encodeURIComponent(book.vehicle.year || p.modelYear || '')}` +
      `&DefaultDisplayName=${encodeURIComponent(book.vehicle.name || '')}&OasisEnv=RELEASE&country=${encodeURIComponent(p.country || 'USA')}` +
      `&region=${encodeURIComponent(p.marketGroup || 'NA')}&v=1238`;
    const picture = this.vehicleImage(vin); // at the same time; usually already on disk
    const html = await this.text(url);
    if (!/vehicleinfo-panel|OASIS RESULT/i.test(html)) throw new Error('Ford sent back something other than OASIS');
    this.lastOasis = { vin: book.vin, html, at: Date.now() };
    return { html, url, image: await picture };
  }

  // PTS's picture of the vehicle: it comes with the manual's contents (Ford's "tree and cover"), not the manual page
  async vehicleImage(vin) {
    try {
      const book = await this.book(vin);
      const html = await this.tree(vin);
      const img = html.match(/<img\b[^>]*\bid=["']VehicleImage["'][^>]*>/i)?.[0] || html.match(/<img\b[^>]*\bclass=["'][^"']*vehicle-image[^>]*>/i)?.[0] || '';
      const src = (img.match(/\bsrc=["']([^"']+)["']/i)?.[1] || '').replace(/&amp;/g, '&').trim();
      if (!src) { this.log('   no vehicle picture with the manual contents'); return ''; }
      const abs = new URL(src, book.bases.publicationBookTreeAndCoverBase || PTS).href;
      if (!ASSET_HOSTS.test(new URL(abs).hostname)) this.log(`   vehicle picture is on a site the app doesn't fetch from: ${abs.slice(0, 200)}`);
      return abs;
    } catch (e) {
      this.log(`   vehicle picture: ${e.message || e}`);
      return '';
    }
  }

  // Connected vehicle data (odometer, fuel, oil life, battery, tires, DTCs…): the same request OASIS makes,
  // using the settings OASIS writes into its page
  async connectedVehicle(vin) {
    return (await this.takeAhead(`cv|${String(vin).toUpperCase()}`)) || this.loadConnectedVehicle(vin);
  }
  async loadConnectedVehicle(vin) {
    const book = await this.book(vin);
    let html = this.lastOasis?.vin === book.vin && Date.now() - this.lastOasis.at < 5 * 60 * 1000 ? this.lastOasis.html : null;
    if (!html) html = (await this.loadOasis(vin)).html;
    const v = (name) => (html.match(new RegExp(`var\\s+${name}\\s*=\\s*["']([^"']*)["']`)) || [])[1] || '';
    const fuelType = (html.match(/setItem\(\s*["']CVFuelType["']\s*,\s*["']([^"']*)["']/) || [])[1] || 'G';
    const modem = ((html.match(/MODEM:\s*<\/label>\s*<label[^>]*>([^<]*)</i) || [])[1] || '').trim() || null;
    const country = v('countryCode') || book.params.country || 'USA';
    const vehicleVin = v('currentVin') || book.vin;
    const paCode = v('cvPACode') || book.params.pacode || '';
    if (!paCode) throw new Error("OASIS didn't include the dealer code connected vehicle data needs");
    const flavor = v('flavor') || 'DEALERS';
    const modelYear = Number(book.vehicle.year || book.params.modelYear) || 0;
    const body = {
      vin: vehicleVin, country, masterID: Number(v('masterID') || book.params.vehicleId) || 0, language: v('langISO') || 'EN-US',
      isDetailed: true, includeAllECU: true, fromDate: v('startDate'), toDate: v('endDate'), paCode: flavor === 'FLEET' ? '4FORD' : paCode,
      progFromDate: v('ProgStartDate') || v('startDate'), progEndDate: v('endDate'), ignoreMetaData: true, flavor, fuelType,
      region: v('cvRegion') || book.params.marketGroup || 'NA', syncModemVal: modem, userRole: 'DEALER', featureCode: '',
      productType: book.vehicle.productType || null, vehicleLine: book.vehicle.vehicleLine || null, modelYear: String(modelYear || ''),
      optionalParameter: JSON.stringify({ battery12VVinCheck: modelYear >= 2025 ? 'true' : 'false' }),
    };
    const url = `${PTS}/ConnectedVehicle/getCVGroupApiData?vin=${encodeURIComponent(vehicleVin)}&countryCode=${encodeURIComponent(country)}`;
    let res;
    try {
      res = await this.ses.fetch(url, { method: 'POST', credentials: 'include', body: JSON.stringify(body),
        headers: { 'Content-Type': 'application/json', Accept: 'application/json, */*', 'X-Requested-With': 'XMLHttpRequest' } });
    } catch (e) { throw new Error(`Couldn't reach Ford (${e.message || e})`); }
    const text = await res.text();
    this.log(`POST ${url} → ${res.status} · ${text.length} characters: ${text.replace(/\s+/g, ' ').slice(0, 160)}`);
    let json = null;
    try { json = JSON.parse(text); } catch {}
    if (!json) {
      if (/<form[^>]+(login|signin|sso|adfs)/i.test(text)) throw new NeedsLogin();
      throw new Error(`Ford didn't send connected vehicle data (${res.status})`);
    }
    return { country, fuelType, capability: json.capabilityData || null, data: json.connectedVehicleData || null, status: res.status };
  }

  // ---- TSBs / GSBs / SSMs for this VIN (Ford's TSB_SSM "DisplayArticles" page, as PTS shows it) ----
  // pacode: the dealership's P&A code (the digits of the Warranty Dealer Code, e.g. 03890), from Settings
  async tsbList(vin, pacode) {
    return (await this.takeAhead(`tsb|${String(vin).toUpperCase()}|${pacode}`)) || this.loadTsbList(vin, pacode);
  }
  async loadTsbList(vin, pacode) {
    const book = await this.book(vin);
    const p = book.params;
    const q = new URLSearchParams({ vin: book.vin, year: book.vehicle.year || p.modelYear || '', model: book.vehicle.name || '',
      pacode, country: p.country || 'USA', masterId: p.vehicleId || '', prodType: p.prodType || 'C', classic: 'false' });
    const url = `${PTS}/vdirsnet/TSB_SSM/DisplayArticles.aspx?${q.toString().replace(/\+/g, '%20')}`;
    const html = await this.text(url);
    if (!/id=["']articles["']|class=["'][^"']*\barticle\b/i.test(html)) {
      this.log(`   TSB list: unexpected page (${html.replace(/<[^>]*>/g, ' ').replace(/\s+/g, ' ').trim().slice(0, 200)})`);
    }
    return { html, url };
  }

  // One article (TSB, GSB, SSM) → { html, url } or { pdf: true, url }
  async tsbArticle(url) {
    let u;
    try { u = new URL(url, `${PTS}/vdirsnet/TSB_SSM/`); } catch { throw new Error('Not a TSB address'); }
    if (u.origin !== PTS || !/^\/vdirsnet\/tsb_ssm\//i.test(u.pathname)) throw new Error('Not a TSB address');
    let href = u.href;
    for (let hop = 0; hop < 3; hop++) {
      let res;
      try { res = await this.fetch(href, { headers: { Accept: 'text/html,application/pdf,*/*' } }); } catch (e) {
        if (!e.needsLogin) throw e; // Ford sometimes refuses the first request and allows the next
        await new Promise((r) => setTimeout(r, 400));
        res = await this.fetch(href, { headers: { Accept: 'text/html,application/pdf,*/*' } });
      }
      const at = res.url || href;
      if (/pdf/i.test(res.headers.get('content-type') || '')) return { pdf: true, url: at };
      const html = await res.text();
      const text = html.replace(/<(script|style)\b[\s\S]*?<\/\1>/gi, ' ').replace(/<[^>]*>/g, ' ').replace(/\s+/g, ' ').trim();
      // a page that only holds the article in a frame: follow it
      const frame = (html.match(/<i?frame\b[^>]*\bsrc\s*=\s*["']([^"']+)["']/i) || [])[1];
      if (frame && text.length < 1500) {
        let next = null;
        try { next = new URL(frame.replace(/&amp;/g, '&'), at); } catch { /* none */ }
        if (next && next.protocol === 'https:' && ASSET_HOSTS.test(next.hostname)) { href = next.href; continue; }
      }
      this.log(`   TSB article: ${text.length} characters of text`);
      return { html, url: at };
    }
    throw new Error("Couldn't find the article in Ford's page");
  }

  // ---- Labor times (Ford SLTS WebView: an ASP.NET page that works by posting its form back to itself).
  //      The Labor Times tab drives the form; this side only fetches pages inside SLTS for it. ----
  sltsUrl(path) {
    let u;
    try { u = new URL(path, `${PTS}/vdirsnet/SLTSVinWebView/`); } catch { return null; }
    if (u.origin !== PTS || !/^\/vdirsnet\/sltsvinwebview\//i.test(u.pathname)) return null;
    return u.href;
  }

  isSltsPage(html, vin) {
    return /DropDownList_SearchType/.test(html) && (!vin || html.toUpperCase().includes(vin.toUpperCase()));
  }

  // Opens SLTS for this VIN (GenerateT.aspx). The vehicle's SLTS model code (e.g. "A8") is remembered per VIN.
  async sltsStart(vin) {
    return (await this.takeAhead(`slts|${String(vin).toUpperCase()}`)) || this.loadSlts(vin);
  }
  async loadSlts(vin) {
    const book = await this.book(vin);
    const p = book.params;
    const known = this.sltsCodes?.get(book.vin);
    const query = (code) => new URLSearchParams({ strVin: book.vin, strYear: book.vehicle.year || p.modelYear || '',
      strModelDesc: book.vehicle.name || '', strGsltsVehCode: code || '', usercountry: p.country || 'USA',
      isolang: p.isoLang || p.isolang || 'EN-US', masterid: p.vehicleId || '' }).toString();
    // GenerateT.aspx works out the model code itself when it's left empty
    const tries = [`GenerateT.aspx?${query(known || '')}`];
    if (known) tries.push(`GenerateT.aspx?${query('')}`);
    let last = '';
    for (const t of tries) {
      const url = this.sltsUrl(t);
      let res, html;
      try {
        res = await this.fetch(url, { headers: { Accept: 'text/html,*/*' }, referrer: `${PTS}/` });
        html = await res.text();
      } catch (e) {
        if (e.needsLogin) throw e;
        last = e.message; this.log(`   SLTS: ${t.split('?')[0]} failed: ${e.message}`); continue;
      }
      if (this.isSltsPage(html, book.vin)) {
        const code = (res.url.match(/strGsltsVehCode=([^&]+)/i) || [])[1] || (html.match(/id="hidden_VinInfo"[^>]*value="\d{4}\|([^|"]+)\|/) || [])[1];
        if (code) { if (!this.sltsCodes) this.sltsCodes = new Map(); this.sltsCodes.set(book.vin, decodeURIComponent(code)); }
        this.log(`   SLTS opened with ${t.split('?')[0]} (model code ${code || '?'})`);
        return { html, url: res.url || url };
      }
      last = html.replace(/<[^>]*>/g, ' ').replace(/\s+/g, ' ').trim().slice(0, 200);
      this.log(`   SLTS: ${t.split('?')[0]} gave something else (${res.url}): ${last}`);
    }
    throw new Error(`Couldn't open SLTS for this vehicle${last ? ` (${last.slice(0, 120)})` : ''}`);
  }

  // One SLTS page: a GET (parts list, print pages) or the form posted back (a search)
  async sltsRequest({ url, fields, method } = {}) {
    const href = this.sltsUrl(url);
    if (!href) throw new Error('Not an SLTS address');
    const opts = { headers: { Accept: 'text/html,*/*' }, referrer: href };
    if (method === 'POST') {
      const body = new URLSearchParams();
      for (const [k, v] of Object.entries(fields || {})) body.append(k, v == null ? '' : String(v));
      Object.assign(opts, { method: 'POST', body: body.toString() });
      opts.headers['Content-Type'] = 'application/x-www-form-urlencoded';
    }
    const res = await this.fetch(href, opts);
    const html = await res.text();
    this.log(`   ${html.length} characters`);
    if (/<form[^>]+(login|signin|sso|adfs)/i.test(html) && !/SLTS|Labor/i.test(html)) throw new NeedsLogin();
    return { html, url: res.url || href };
  }

  // ---- Software validation codes (SVC) for field service actions: the same requests Ford's
  //      "FSA Software Verification Form" makes ----
  // The form page lists the FSAs that need an SVC for this VIN ("**" = some vehicles in the FSA don't)
  async svcForm(vin) {
    const book = await this.book(vin);
    const hit = this.svcForms?.get(book.vin);
    if (hit && Date.now() - hit.at < 10 * 60 * 1000) return hit;
    const url = `${PTS}/FSASoftwareVerification?vin=${encodeURIComponent(book.vin)}&vid=${encodeURIComponent(book.params.vehicleId || '')}` +
      `&modelYear=${encodeURIComponent(book.vehicle.year || book.params.modelYear || '')}`;
    const html = await this.text(url);
    const list = (name) => {
      const m = html.match(new RegExp(`var\\s+${name}\\s*=\\s*\\[([^\\]]*)\\]`));
      return m ? [...m[1].matchAll(/["']([^"']+)["']/g)].map((x) => x[1].toUpperCase()) : [];
    };
    const fsas = [];
    for (const m of html.matchAll(/<li\b[^>]*class=["'][^"']*custom-dropdown-item[^"']*["'][^>]*>([\s\S]*?)<\/li>/gi)) {
      const value = (m[0].match(/data-value=["']([^"']*)["']/i)?.[1] || '').trim();
      const text = m[1].replace(/<[^>]*>/g, ' ').replace(/\s+/g, ' ').trim();
      const num = (value || text).replace(/\*+$/, '').trim().toUpperCase();
      if (num) fsas.push({ num, some: /\*\*$/.test(value || text) });
    }
    const out = { at: Date.now(), vin: book.vin, url, fsas, ids: list('IDS_array'), jv: list('JV_FSA_array') };
    this.log(`   SVC form: ${fsas.map((f) => f.num + (f.some ? '**' : '')).join(', ') || 'no FSAs need an SVC'}`);
    if (!this.svcForms) this.svcForms = new Map();
    this.svcForms.set(book.vin, out);
    return out;
  }

  // Codes already retrieved, kept on this computer so they show straight away next time
  svcSaved(vin) {
    const all = this.readSvcFile();
    const out = {};
    for (const [k, v] of Object.entries(all)) if (k.startsWith(`${vin}|`)) out[k.slice(vin.length + 1)] = v;
    return out;
  }
  readSvcFile() {
    if (!this.cacheDir) return {};
    try { return JSON.parse(fs.readFileSync(path.join(this.cacheDir, 'svc.json'), 'utf8')) || {}; } catch { return {}; }
  }
  saveSvc(vin, fsa, code) {
    if (!this.cacheDir) return;
    const all = this.readSvcFile();
    const cutoff = Date.now() - 180 * 24 * 3600 * 1000;
    for (const [k, v] of Object.entries(all)) if (!v || v.at < cutoff) delete all[k];
    all[`${vin}|${fsa}`] = { code, at: Date.now() };
    try { fs.mkdirSync(this.cacheDir, { recursive: true }); fs.writeFileSync(path.join(this.cacheDir, 'svc.json'), JSON.stringify(all)); } catch {}
  }

  // Ask Ford for the SVC: { status: 'ok', code } | { status: 'incomplete', modules } | { status: 'error', message }
  async svc(vin, fsa, dealerCode) {
    const book = await this.book(vin);
    const form = await this.svcForm(vin);
    fsa = String(fsa || '').toUpperCase().replace(/\*+$/, '');
    if (form.jv.includes(fsa)) return { status: 'error', message: 'This FSA is checked a different way. Use the form in PTS.', usePts: true, url: form.url };
    const q = new URLSearchParams({ vin: book.vin, data: fsa, vid: book.params.vehicleId || '', modelYear: book.vehicle.year || book.params.modelYear || '',
      language: 'EN', dealerCode, idsFlag: form.ids.includes(fsa) ? 'true' : 'false' });
    const res = await this.fetch(`${PTS}/FSASoftwareVerification/FsaVerificationFunc?${q}`, {
      headers: { Accept: 'application/json, text/javascript, */*; q=0.01', 'X-Requested-With': 'XMLHttpRequest' } });
    const text = await res.text();
    this.log(`   SVC ${fsa}: ${text.replace(/\s+/g, ' ').slice(0, 300)}`);
    let d;
    try { d = JSON.parse(text); } catch {
      if (/<form[^>]+(login|signin|sso|adfs)/i.test(text)) throw new NeedsLogin();
      return { status: 'error', message: "Ford's answer couldn't be read" };
    }
    if (!Array.isArray(d)) return { status: 'error', message: "Ford's answer couldn't be read" };
    const str = (x) => (x == null ? '' : typeof x === 'string' ? x : JSON.stringify(x));
    if (d[0] === true || d[0] === 'true') {
      const code = str(d[4]).trim();
      if (!code || code === 'null' || code === 'undefined') return { status: 'error', message: 'Ford said the updates are done but sent no code' };
      this.saveSvc(book.vin, fsa, code);
      return { status: 'ok', code };
    }
    if (d[0] === false || d[0] === 'false') return { status: 'incomplete', modules: str(d[1]), detail: str(d[2]) };
    return { status: 'error', message: [str(d[1]), str(d[2])].filter((x) => x && x !== 'null').join(' — ') || 'Ford returned an error', code: str(d[3]) };
  }

  // ---- Diagnostics search (Ford's "Enhanced Search": DTCs or words → TSB/GSB/SSM, pinpoint tests,
  //      procedures, PC/ED, wiring, videos). The same two requests Ford's Diagnostics tab makes:
  //   1. /Diagnostics/UnifiedSearch?VIN=… → the vehicle's article numbers and book codes (written into the page)
  //   2. /Diagnostics/GetAllSearchResults (POST) → the results
  async diagContext(vin) {
    const book = await this.book(vin);
    this.diagCtx ||= new Map();
    const cached = this.diagCtx.get(book.vin);
    if (cached && Date.now() - cached.at < 30 * 60 * 1000) return cached;
    const year = book.vehicle.year || book.params.modelYear || '';
    const url = `${PTS}/Diagnostics/UnifiedSearch?VIN=${encodeURIComponent(book.vin)}&VID=${encodeURIComponent(book.params.vehicleId)}` +
      `&BuildDate=${encodeURIComponent(book.vehicle.buildDate || '')}&ModelYear=${encodeURIComponent(year)}`;
    const html = await this.text(url);
    const decode = (t) => t.replace(/&quot;/g, '"').replace(/&#39;/g, "'").replace(/&amp;/g, '&');
    const str = (name) => { const m = html.match(new RegExp(`var\\s+${name}\\s*=\\s*"([^"]*)"`)); return m ? decode(m[1]) : ''; };
    // var X = DeserializeJSON("[&quot;…&quot;]")  (or '…')
    // (the page also has commented-out versions of some of these lines: those are skipped)
    const list = (name) => {
      let out = [];
      for (const m of html.matchAll(new RegExp(`^([^\\n]*?)var\\s+${name}\\s*=\\s*DeserializeJSON\\((["'])([\\s\\S]*?)\\2\\)`, 'gm'))) {
        if (m[1].includes('//')) continue;
        try { const v = JSON.parse(decode(m[3]) || '[]'); if (Array.isArray(v)) out = v; } catch { /* not a list */ }
      }
      return out;
    };
    if (!/GetAllSearchResults|UnifiedSearchBar|ArticleNumbers/i.test(html) && !/var\s+ESVIN/.test(html)) {
      throw new Error("Ford's diagnostics search didn't load for this vehicle");
    }
    const lang = (str('ArticleLanguage') || str('LanguageOdysseyCode') || 'ENUSA').toLowerCase();
    const ctx = {
      at: Date.now(), vin: book.vin, url,
      articles: list('ArticleNumbers').map(String),
      warranty: list('WarrantyArticleNumbers').map(String),
      wsmBooks: list('WSMBookCode').map(String),
      wiringBooks: list('WiringBookCode').map(String),
      pcedBooks: list('PCEDBookCodes').map(String),
      wsmTitles: list('WSMBookTitles'),
      pcedTitles: list('PCEDBookTitles'),
      buildDate: str('BuildDate') || book.vehicle.buildDate || '',
      vehicleId: (html.match(/var\s+VehicleID\s*=\s*(\d+)/) || [])[1] || book.params.vehicleId,
      modelYear: str('ModelYear') || year,
      lang, wsmLang: (str('WSMLanguage') ? lang : lang), country: str('Country') || book.params.country || 'USA',
      contentMarket: str('ContentMarket') || 'US', isoLang: str('ISOLanguageCode') || 'EN-US',
    };
    this.log(`   diagnostics: ${ctx.articles.length} articles, WSM ${ctx.wsmBooks.join(',')}, wiring ${ctx.wiringBooks.join(',')}, PC/ED ${ctx.pcedBooks.join(',')}`);
    this.diagCtx.set(book.vin, ctx);
    return ctx;
  }

  // Ford narrows results to the vehicle's options with the "FeaturesAttributes" cookie PTS sets when a VIN is
  // identified; it's only used when it belongs to this vehicle (same build date)
  async featureAttributes(ctx) {
    try {
      const cookies = await this.ses.cookies.get({ name: 'FeaturesAttributes' });
      const c = cookies.find((x) => /dealerconnection\.com$/i.test(x.domain.replace(/^\./, '')));
      if (!c) return null;
      let raw = c.value;
      try { raw = decodeURIComponent(raw); } catch {}
      const out = {};
      for (const pair of raw.split('&')) {
        const [k, v] = pair.split('=');
        if (!k || v == null || !/^[A-Za-z0-9_]+$/.test(k)) continue;
        out[k] = v.includes('|') ? v.split('|') : v;
      }
      if (!out.BuildDate || String(out.BuildDate) !== String(ctx.buildDate)) return null;
      const arr = (v) => (Array.isArray(v) ? v : v ? [v] : []);
      return {
        featureCodes: [...arr(out.PrimaryFeatureCodes), ...arr(out.MinorFeatureCodes)].sort().slice(0, 300),
        vehicleLine: out.VehicleLine ? [String(out.VehicleLine).toLowerCase() + (out.VehicleType || '')] : undefined,
        buildDate: out.BuildDate,
      };
    } catch { return null; }
  }

  // text: words; dtcs: ["P0420", "U3000:49", …]; sources: ['tsb','gsb','ssm','wsmppt','wsmproc','pced','wiring','tsv']
  async diagSearch(vin, { text = '', dtcs = [], sources, rows = 50, start = 0, exact = false } = {}) {
    const ctx = await this.diagContext(vin);
    const langFor = (type) => (type === 'tsv' ? 'enusa' : ctx.lang);
    const all = ['tsb', 'gsb', 'ssm', 'wsmppt', 'wsmproc', 'wiring', 'tsv', ...(ctx.pcedBooks.length ? ['pced'] : [])];
    const params = {
      searchTerm: String(text || '').trim(),
      articleNumber: [...new Set([...ctx.articles, ...ctx.wsmBooks, ...ctx.pcedBooks, ...ctx.wiringBooks, ...ctx.warranty]
        .filter(Boolean).map((x) => String(x).replace(/\//g, '')))],
      dtc: dtcs,
      source: (sources || all).filter((t) => all.includes(t)).map((type) => ({ language: langFor(type), type })),
      start, rows, descriptionFlag: true,
      symptomCodes: [],
      modelAndYear: [`${ctx.modelYear} ${(await this.book(vin)).vehicle.name || ''}`.trim()],
    };
    if (exact) params.searchType = 'exact';
    const fa = await this.featureAttributes(ctx);
    if (fa) { params.featureCodes = fa.featureCodes; if (fa.vehicleLine) params.vehicleLine = fa.vehicleLine; params.buildDate = fa.buildDate; }
    const res = await this.fetch(`${PTS}/Diagnostics/GetAllSearchResults`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/x-www-form-urlencoded; charset=UTF-8', Accept: 'application/json, text/javascript, */*; q=0.01',
        'X-Requested-With': 'XMLHttpRequest', Referer: ctx.url },
      body: new URLSearchParams({ searchRequest: JSON.stringify(params) }).toString(),
    });
    const body = await res.text();
    let json;
    try { json = JSON.parse(body); } catch {
      if (/<form[^>]+(login|signin|sso|adfs)/i.test(body)) throw new NeedsLogin();
      throw new Error("Ford's diagnostics search didn't answer");
    }
    if (!this.loggedDiagFields && Array.isArray(json.data) && json.data[0]) {
      this.loggedDiagFields = true;
      this.log(`   search result fields: ${Object.keys(json.data[0]).join(',')} | filter: ${Object.keys(json.filter || {}).join(',')}`);
    }
    const counts = {};
    for (const s of json.filter?.source || []) counts[s.name] = (counts[s.name] || 0) + (Number(s.count) || 0);
    const dtcInfo = (json.filter?.dtcInfo || []).map((x) => ({ name: x.name, count: x.count, description: x.description || x.desc || '' }));
    return { ctx: { vehicleId: ctx.vehicleId, wsmTitles: ctx.wsmTitles, pcedTitles: ctx.pcedTitles, country: ctx.country,
      contentMarket: ctx.contentMarket, isoLang: ctx.isoLang }, counts, dtcInfo, results: (json.data || []).map((r) => diagResult(r, ctx)) };
  }

  // Newer ("SPA") pinpoint tests keep their steps in a separate file, fetched when the test is opened
  async spaSteps(vin, { file, procUid }) {
    const book = await this.book(vin);
    const p = book.params;
    const url = book.bases.publicationProcContentBase;
    if (!url) throw new Error("Couldn't find where this test's steps come from");
    const form = new URLSearchParams({
      moduleName: '', isMobile: p.isMobile || 'no', usertype: p.userType || 'Dealer', userQsPilot: p.userQsPilot || '',
      vehicleId: p.vehicleId, modelYear: p.modelYear, channel: p.channelId || '', category: p.categoryId || '',
      book: p.bookcode || '', bookTitle: p.bookTitle || '', booktype: p.booktype || '', country: p.country || '',
      language: p.language || '', contentmarket: p.contentmarket || '', contentlanguage: p.contentlang || '',
      languageOdysseyCode: p.languageOdysseyCode || '', diagTool: p.diagTool || '', otx: p.otx || '',
      searchNumber: `${file}_spappt`, adtLocation: '', procUid: procUid || '', Vid: '', byVin: 'NO',
      marketGroup: p.marketGroup || '', WiringBookCode: p.WiringBookCode || '',
      WiringBookTitle: encodeURIComponent(p.WiringBookTitle || ''), WiringFormat: p.WiringFormat || '', fromPageBase: p.origin || PTS,
    });
    const html = await this.text(url, {
      method: 'POST', body: form.toString(),
      headers: { 'Content-Type': 'application/x-www-form-urlencoded; charset=UTF-8', Accept: 'text/html, */*; q=0.01' },
    });
    if (this.cacheDir) { try { fs.writeFileSync(path.join(this.cacheDir, 'last-test-steps.html'), html); } catch {} }
    return { html, base: url };
  }

  // Pinpoint tests: report the test and each answered step to Vehicle History, exactly as Ford's
  // manual does (POST /Events/ajaxLoggerForSPAppt), so warranty can see the test was run and the results
  async reportPinpoint(vin, ev) {
    const book = await this.book(vin);
    const clean = (v, n = 500) => String(v == null ? '' : v).replace(/\s+/g, ' ').trim().slice(0, n);
    const step = ev.step ? [{
      id: clean(ev.step.id, 20), title: clean(ev.step.title), question: clean(ev.step.question),
      answer: clean(ev.step.answer, 20), outcome: clean(ev.step.outcome, 1000),
      values: { inputData: null }, mode: ev.step.mode === 'MULTI' ? 'MULTI' : 'SINGLE',
    }] : null;
    const body = JSON.stringify({
      eventActionpptName: 'PPT_Letter_Title',
      eventCustompptId: clean(ev.procUid, 40),
      eventCustompptValue: step ? step[0].title : clean(ev.title),
      eventpinpointsteps: step,
      vin: book.params.vin || book.vin,
      pptname: clean(ev.pptName, 20),
    });
    const res = await this.fetch(`${PTS}/Events/ajaxLoggerForSPAppt`, {
      method: 'POST', body,
      headers: { 'Content-Type': 'application/json', 'X-Requested-With': 'XMLHttpRequest', Accept: '*/*' },
      referrer: this.procUrl(book, { searchNumber: ev.searchNumber || '', procUid: ev.procUid || '' }),
    });
    const text = await res.text().catch(() => '');
    this.log(`   pinpoint ${clean(ev.pptName, 20)} ${step ? `step ${step[0].id} ${step[0].answer}` : 'opened'} → ${text.replace(/\s+/g, ' ').slice(0, 120)}`);
    if (/<form[^>]+(login|signin|sso|adfs)/i.test(text)) throw new NeedsLogin();
    return true;
  }

  // Pictures and other files inside procedures, fetched with the PTS login
  async asset(url) {
    let u;
    try { u = new URL(url); } catch { return new Response('', { status: 400 }); }
    if (u.protocol !== 'https:' || !ASSET_HOSTS.test(u.hostname)) {
      this.log(`picture blocked (not a Ford site): ${u.href.slice(0, 160)}`);
      return new Response('', { status: 403 });
    }
    // As Ford's pages load them: with a Ford page as the referrer. Ford's servers sometimes refuse the
    // first request (403) and allow the next; fordservicecontent.com's catalog also wants its sign-on
    // cookies (onDenied gets them), so a refused picture is tried once more.
    const get = () => this.ses.fetch(u.href, { credentials: 'include', referrer: `${PTS}/` });
    try {
      let res = await get();
      if (res.status === 401 || res.status === 403) {
        if (/fordservicecontent\.com$/i.test(u.hostname) && this.onDenied) await this.onDenied().catch(() => {});
        else await new Promise((r) => setTimeout(r, 300));
        res = await get();
      }
      if (!res.ok) this.log(`picture ${res.status}: ${u.href.slice(0, 200)}`);
      return res;
    } catch (e) {
      this.log(`picture failed (${e.message || e}): ${u.href.slice(0, 200)}`);
      return new Response('', { status: 502 });
    }
  }
}

// Reads `var params = {...}` and the `var …Base = '…'` addresses out of Ford's manual page
// One search result, trimmed to what the app shows and needs to open it
function diagResult(r, ctx = {}) {
  const clean = (t) => String(t || '').replace(/<[^>]*>/g, ' ').replace(/&nbsp;/g, ' ').replace(/&amp;/g, '&').replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'").replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/\s+/g, ' ').trim();
  const out = { source: r.source || '', type: r.type || '', title: clean(r.title), description: clean(r.description).slice(0, 400),
    date: r.effectiveDate || r.publicationDate || '', applies: r.qsMatched !== false };
  if (['wsmppt', 'wsmproc', 'pced'].includes(out.source)) {
    // As Ford's page works it out: GSI books → procUid = uid, searchNumber = procuid; ODY books ("G…" procuid) the other way round
    let procUid = r.uid || '', searchNumber = r.procuid || '';
    if (String(searchNumber).charAt(0).toLowerCase() === 'g') { procUid = r.procuid; searchNumber = r.uid; }
    if (out.source === 'pced' && !r.uid) { procUid = r.procuid; if (/_s[45]p/.test(procUid)) searchNumber = `spapced_${r.procuid}`; }
    const bar = out.title.indexOf(' |');
    if (bar > 0) out.title = `${out.title.slice(0, bar)} ${out.title.slice(out.title.indexOf('|') + 1)}`.replace(/\s+/g, ' ').trim();
    Object.assign(out, { searchNumber: String(searchNumber || ''), procUid: String(procUid || ''), ppt: r.pptid ? String(r.pptid) : '', book: r.bookCode || r.articleNumber || '' });
  } else if (['tsb', 'gsb', 'ssm'].includes(out.source)) {
    out.article = String(r.articleNumber || '');
    out.url = `/vdirsnet/TSB_SSM/DisplayArticle.aspx?content=/vdirsnet/TSB_SSM/DisplayArticle.aspx&id=${encodeURIComponent(out.article)}` +
      `&lang=${encodeURIComponent(ctx.isoLang || 'EN-US')}&country=${encodeURIComponent(ctx.country || 'USA')}&market=${encodeURIComponent(ctx.contentMarket || 'US')}`;
  } else if (out.source === 'tsv') {
    out.url = /^https:/i.test(String(r.uid || '')) ? r.uid : '';
  } else if (out.source === 'wiring') {
    // The same wiring addresses Ford's search builds
    const book = String(r.articleNumber || ''), conn = String(r.wiringConnector || ''), lang = ctx.lang || 'enusa';
    const vid = encodeURIComponent(ctx.vehicleId || '1');
    if (r.type === 'cfc') {
      out.title = `${conn}: ${out.title}`;
      out.url = `/wiring/face/?book=${encodeURIComponent(book)}&vehicleId=${vid}&cell=150&page=&item=${encodeURIComponent(book.toLowerCase() + 'cf' + conn.toLowerCase())}&bookType=svg&languageCode=${lang}`;
    } else {
      let cell = String(r.wiringCell || ''), base = '/wiring/page';
      if (r.type === 'celttl') { if (cell === '150') base = '/wiring/connectors'; out.title = `${cell} ${out.title}`; }
      else if (r.type === 'component_search') out.title = `${out.title}: ${cell}-${r.wiringPage || ''}`;
      else if (r.type === 'conn_search') out.title = `${conn}: ${cell}-${r.wiringPage || ''} ${out.title}`;
      else if (/_index$/.test(r.type || '')) cell = '151';
      out.url = `${base}?book=${encodeURIComponent(book)}&vehicleId=${vid}&cell=${encodeURIComponent(cell)}&page=${encodeURIComponent(r.wiringPage || 1)}` +
        `&bookType=svg&languageCode=${lang}&wiringBookCode=${encodeURIComponent((ctx.wiringBooks || []).join(','))}&title=${encodeURIComponent(out.title)}` +
        `${conn ? `&item=${encodeURIComponent(conn)}` : ''}`;
    }
    out.url += '&comefrom=enhancesearch';
    out.description = '';
  }
  return out;
}

function parseBookPage(html) {
  const params = {};
  const start = html.indexOf('var params = {');
  if (start >= 0) {
    const end = html.indexOf('};', start);
    const block = html.slice(start, end < 0 ? start + 20000 : end);
    for (const m of block.matchAll(/^\s*([A-Za-z_][A-Za-z0-9_]*)\s*:\s*(['"])(.*?)\2\s*,?\s*$/gm)) params[m[1]] = m[3];
  }
  const bases = {};
  for (const m of html.matchAll(/var\s+([A-Za-z_][A-Za-z0-9_]*(?:Base|BaseUrl))\s*=\s*'([^']*)'/g)) bases[m[1]] = m[2];
  return { params, bases };
}

module.exports = { ManualService, NeedsLogin, parseBookPage, PTS, ASSET_HOSTS };