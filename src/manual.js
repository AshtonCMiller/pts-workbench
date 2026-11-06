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
  async oasis(vin) {
    const book = await this.book(vin);
    const p = book.params;
    const url = `${PTS}/oasis/Index?vin=${encodeURIComponent(book.vin)}&ModelYear=${encodeURIComponent(book.vehicle.year || p.modelYear || '')}` +
      `&DefaultDisplayName=${encodeURIComponent(book.vehicle.name || '')}&OasisEnv=RELEASE&country=${encodeURIComponent(p.country || 'USA')}` +
      `&region=${encodeURIComponent(p.marketGroup || 'NA')}&v=1238`;
    const html = await this.text(url);
    if (!/vehicleinfo-panel|OASIS RESULT/i.test(html)) throw new Error('Ford sent back something other than OASIS');
    this.lastOasis = { vin: book.vin, html, at: Date.now() };
    return { html, url };
  }

  // Connected vehicle data (odometer, fuel, oil life, battery, tires, DTCs…): the same request OASIS makes,
  // using the settings OASIS writes into its page
  async connectedVehicle(vin) {
    const book = await this.book(vin);
    let html = this.lastOasis?.vin === book.vin && Date.now() - this.lastOasis.at < 5 * 60 * 1000 ? this.lastOasis.html : null;
    if (!html) html = (await this.oasis(vin)).html;
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
    if (u.protocol !== 'https:' || !ASSET_HOSTS.test(u.hostname)) return new Response('', { status: 403 });
    try {
      return await this.ses.fetch(u.href, { credentials: 'include' });
    } catch {
      return new Response('', { status: 502 });
    }
  }
}

// Reads `var params = {...}` and the `var …Base = '…'` addresses out of Ford's manual page
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