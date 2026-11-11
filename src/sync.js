// Sync between computers (opt-in).
//
// What syncs: flags, hours/overrides, renamed lines, RO lines and Review/Completed status, the
// shift clock, and which ROs are open in the Workbench (RO numbers and year/make/model only; each
// computer gets the VIN from Xtime). Never: VINs, customer details, saved logins, settings or tab layouts.
//
// How: every piece of data is a separate record with a timestamp. When two computers changed
// things, the records are merged one by one and the newest change to each record wins, so
// nothing else is lost. Deleted things leave a "deleted" marker so they stay deleted.
// Everything is compressed and encrypted (AES-256-GCM) on this computer before it is sent; the
// server only sees a hashed ID and unreadable text. Both come from the store ID + employee number.
const crypto = require('crypto');
const zlib = require('zlib');

const DAY = 24 * 60 * 60 * 1000;
const KEEP_DELETED = 90 * DAY;
const FORMAT = 1;

// ---------- identity & encryption ----------
function deriveKeys(storeId, employeeNumber) {
  const emp = String(employeeNumber || '').trim().replace(/^0+(?=\d)/, '');
  const store = String(storeId || '').trim();
  if (!emp || !store) return null;
  const material = `pts-workbench-sync-v1|${store}|${emp}`;
  return {
    id: crypto.createHash('sha256').update(`id|${material}`).digest('hex'),
    key: Buffer.from(crypto.hkdfSync('sha256', material, 'pts-workbench', 'encrypt', 32)),
  };
}

function encrypt(key, obj) {
  const iv = crypto.randomBytes(12);
  const cipher = crypto.createCipheriv('aes-256-gcm', key, iv);
  const body = Buffer.concat([cipher.update(zlib.deflateRawSync(Buffer.from(JSON.stringify(obj)))), cipher.final()]);
  return `v1.${Buffer.concat([iv, cipher.getAuthTag(), body]).toString('base64')}`;
}

function decrypt(key, text) {
  if (!text || !text.startsWith('v1.')) throw new Error('unknown format');
  const raw = Buffer.from(text.slice(3), 'base64');
  const decipher = crypto.createDecipheriv('aes-256-gcm', key, raw.subarray(0, 12));
  decipher.setAuthTag(raw.subarray(12, 28));
  const plain = Buffer.concat([decipher.update(raw.subarray(28)), decipher.final()]);
  return JSON.parse(zlib.inflateRawSync(plain).toString());
}

// ---------- records ----------
const stable = (v) => {
  if (v === undefined) return 'null';
  if (v === null || typeof v !== 'object') return JSON.stringify(v);
  if (Array.isArray(v)) return `[${v.map(stable).join(',')}]`;
  return `{${Object.keys(v).sort().filter((k) => v[k] !== undefined).map((k) => `${JSON.stringify(k)}:${stable(v[k])}`).join(',')}}`;
};
const hashOf = (v) => crypto.createHash('sha1').update(stable(v)).digest('base64');

// Tracker data → { key: value }. Keys: f/RO/LINE flag · r/RO status · l/RO lines · o/RO/LINE hours
// override · n/RO/LINE renamed line · s/START shift · c/DEVICE when that computer last closed the app ·
// w/RO an RO open in the Workbench
function recordsFrom(data, deviceId) {
  const out = {};
  for (const f of data.flags || []) {
    out[`f/${f.ro}/${f.key}`] = {
      dms: f.dms || '', label: f.label || '', ymm: f.ymm || '',
      hours: f.hours ?? 0, flaggedAt: f.flaggedAt, confirmed: !!f.confirmed,
    };
  }
  for (const [ro, rec] of Object.entries(data.ros || {})) {
    out[`r/${ro}`] = {
      ymm: rec.ymm || '', tag: rec.tag || '', review: rec.review || null,
      completedAt: rec.completedAt || null, onList: rec.onList || null,
    };
    const lines = {};
    for (const [k, l] of Object.entries(rec.lines || {})) {
      const { override, name, ...rest } = l;
      lines[k] = rest;
      if (override != null) out[`o/${ro}/${k}`] = override;
      if (name) out[`n/${ro}/${k}`] = name;
    }
    out[`l/${ro}`] = { order: rec.order || [], lines };
  }
  for (const [ro, w] of Object.entries(data.openROs || {})) out[`w/${ro}`] = { ymm: w?.ymm || '' };
  if (deviceId) out[`v/${deviceId}`] = { openROs: 1 }; // this computer's app syncs open ROs
  for (const s of data.shifts || []) out[`s/${s.start}`] = { start: s.start, end: s.end ?? null, auto: !!s.auto };
  if (deviceId && data.lastClosed) out[`c/${deviceId}`] = data.lastClosed;
  return out;
}

const newer = (a, b) => (a.t !== b.t ? a.t > b.t : String(a.d) > String(b.d));

// Pick between two copies of the same record
function pick(key, a, b) {
  if (!a) return b;
  if (!b) return a;
  // A shift that was closed for real beats one closed automatically ("forgot to clock out")
  if (key.startsWith('s/') && !a.del && !b.del) {
    const realA = a.v.end != null && !a.v.auto;
    const realB = b.v.end != null && !b.v.auto;
    if (a.v.auto && realB) return b;
    if (b.v.auto && realA) return a;
  }
  return newer(a, b) ? a : b;
}

function mergeRecords(local, remote) {
  const out = {};
  for (const key of new Set([...Object.keys(local), ...Object.keys(remote)])) out[key] = pick(key, local[key], remote[key]);
  return out;
}

// Two computers clocked in separately → one shift. A shift left open on one computer is closed by a
// clock-out made later on another computer.
function normalizeShifts(entries) {
  const list = entries.map((e) => ({ ...e.v, t: e.t })).sort((a, b) => a.start - b.start);
  const out = [];
  let group = null;
  const close = (g) => {
    const open = g.members.filter((m) => m.end == null);
    const closed = g.members.filter((m) => m.end != null);
    let end = null;
    let auto = false;
    if (!open.length) {
      const last = closed.reduce((a, b) => (b.end > a.end ? b : a));
      end = last.end; auto = closed.every((m) => m.auto);
    } else {
      const openT = Math.max(...open.map((m) => m.t));
      const startOpen = Math.max(...open.map((m) => m.start));
      const closers = closed.filter((m) => m.t > openT && m.end >= startOpen);
      if (closers.length) {
        const last = closers.reduce((a, b) => (b.end > a.end ? b : a));
        end = last.end; auto = last.auto;
      }
    }
    return { start: g.start, end, auto };
  };
  for (const s of list) {
    if (group) {
      const sofar = close(group);
      if (sofar.end != null && s.start > sofar.end) { out.push(sofar); group = null; }
    }
    if (!group) group = { start: s.start, members: [s] };
    else group.members.push(s);
  }
  if (group) out.push(close(group));
  return out.map((s) => (s.auto ? s : { start: s.start, end: s.end }));
}

// Merged records → tracker data (VINs and other local-only details are kept from this computer)
function applyRecords(data, recs) {
  const live = {};
  for (const [k, r] of Object.entries(recs)) if (!r.del) live[k] = r;
  const roIds = new Set();
  for (const k of Object.keys(live)) if (k.startsWith('r/') || k.startsWith('l/')) roIds.add(k.slice(2));

  const ros = {};
  for (const ro of roIds) {
    const local = data.ros?.[ro] || {};
    const r = live[`r/${ro}`]?.v || {};
    const l = live[`l/${ro}`]?.v || { order: [], lines: {} };
    const rec = {
      ...(local.vin ? { vin: local.vin } : {}),
      ...(local.touched ? { touched: local.touched } : {}),
      ...(local.seenInXtime ? { seenInXtime: true } : {}),
      ymm: r.ymm || '', tag: r.tag || '', review: r.review || null, completedAt: r.completedAt || null,
      onList: r.onList || null, order: l.order || [], lines: {},
    };
    for (const [k, line] of Object.entries(l.lines || {})) {
      rec.lines[k] = { ...line, override: live[`o/${ro}/${k}`]?.v ?? null, name: live[`n/${ro}/${k}`]?.v ?? null };
    }
    ros[ro] = rec;
  }
  data.ros = ros;

  const existing = new Map((data.flags || []).map((f) => [`${f.ro}/${f.key}`, f]));
  const flags = [];
  for (const [k, r] of Object.entries(live)) {
    if (!k.startsWith('f/')) continue;
    const rest = k.slice(2);
    const slash = rest.indexOf('/');
    const ro = rest.slice(0, slash);
    const key = rest.slice(slash + 1);
    const old = existing.get(`${ro}/${key}`);
    flags.push({ id: old ? old.id : data.nextFlagId++, ro, key, ...r.v });
  }
  data.flags = flags;

  const open = {};
  for (const [k, r] of Object.entries(live)) if (k.startsWith('w/')) open[k.slice(2)] = { ymm: r.v?.ymm || '' };
  data.openROs = open;
  // Who closed each RO. An older version of the app on another computer doesn't know about open ROs and
  // drops them all; only closes from computers whose app syncs open ROs should close them here.
  const aware = new Set(Object.keys(live).filter((k) => k.startsWith('v/')).map((k) => k.slice(2)));
  data.openClosedBy = {};
  for (const [k, r] of Object.entries(recs)) if (k.startsWith('w/') && r.del) data.openClosedBy[k.slice(2)] = aware.has(String(r.d)) ? 'app' : 'old';

  data.shifts = normalizeShifts(Object.entries(live).filter(([k]) => k.startsWith('s/')).map(([, r]) => r));
  data.lastClosedAll = Math.max(0, ...Object.entries(live).filter(([k]) => k.startsWith('c/')).map(([, r]) => Number(r.v) || 0));
}

// ---------- engine ----------
class SyncEngine {
  // tracker: the Tracker. identity(): { enabled, storeId, employeeNumber }. url: server address.
  // load()/save(state): this computer's sync bookkeeping. request(method, url, body): HTTP.
  // onApplied(): called after changes from other computers were put into the tracker
  constructor({ tracker, identity, url, load, save, request, now = () => Date.now(), onStatus = () => {}, onApplied = () => {} }) {
    Object.assign(this, { tracker, identity, url: String(url || '').replace(/\/+$/, ''), saveState: save, request, now, onStatus, onApplied });
    this.state = { deviceId: crypto.randomBytes(8).toString('hex'), version: 0, journal: {}, maxT: 0, who: null, joining: true, ...(load() || {}) };
    this.status = { state: 'off', message: '', lastSync: this.state.lastSync || null };
    this.applying = false;
    this.running = null;
    this.again = false;
    this.timer = null;
    this.paused = false; // while you choose between restoring from the server and overwriting it
  }

  configured() { return !!this.url; }

  keys() {
    const who = this.identity();
    if (!who?.enabled) return null;
    return deriveKeys(who.storeId, who.employeeNumber);
  }

  setStatus(patch) { Object.assign(this.status, patch); this.onStatus(this.status); }

  stampT() {
    const t = Math.max(this.now(), (this.state.maxT || 0) + 1);
    this.state.maxT = t;
    return t;
  }

  // Compare the tracker with the journal and timestamp whatever changed on this computer
  stamp() {
    const current = recordsFrom(this.tracker.data, this.state.deviceId);
    const j = this.state.journal;
    let changed = false;
    for (const [k, v] of Object.entries(current)) {
      const h = hashOf(v);
      if (!j[k] || j[k].del || j[k].h !== h) {
        // Joining sync: what's already on this computer counts as oldest, so it only fills gaps
        // in the cloud copy and never overrules newer changes made on other computers.
        const t = this.state.joining ? 1 : this.stampT();
        j[k] = { t, d: this.state.deviceId, h };
        changed = true;
      }
    }
    for (const [k, e] of Object.entries(j)) {
      if (!(k in current) && !e.del) { j[k] = { t: this.stampT(), d: this.state.deviceId, del: 1 }; changed = true; }
    }
    this.state.joining = false; // only what was here when joining counts as oldest
    // forget old deleted markers
    const cutoff = this.now() - KEEP_DELETED;
    for (const [k, e] of Object.entries(j)) if (e.del && e.t < cutoff) delete j[k];
    if (changed) this.state.dirty = true;
    return current;
  }

  localRecords() {
    const current = this.stamp();
    const out = {};
    for (const [k, e] of Object.entries(this.state.journal)) {
      out[k] = e.del ? { t: e.t, d: e.d, del: 1 } : { t: e.t, d: e.d, v: current[k] };
    }
    return out;
  }

  // Called after every tracker save
  noteLocalChange() {
    if (this.applying || this.paused || !this.keys() || !this.configured()) return;
    this.stamp();
    this.persist();
    if (this.state.dirty) { clearTimeout(this.soon); this.soon = setTimeout(() => this.syncNow(), 3000); }
  }

  persist() { try { this.saveState(this.state); } catch { /* best effort */ } }

  start(intervalMs = 60 * 1000) {
    clearInterval(this.timer);
    this.timer = setInterval(() => this.syncNow(), intervalMs);
    return this.syncNow();
  }

  stop() { clearInterval(this.timer); this.timer = null; clearTimeout(this.soon); }

  // Turning sync on (again): this computer joins fresh, so it fills gaps without overruling the cloud
  rejoin() {
    this.state = { ...this.state, who: null, version: 0, journal: {}, joining: true, dirty: true };
    this.persist();
  }

  // Turning sync on: is there already a copy on the server for this person? (counts only, for the question
  // "restore from the server or overwrite it?"). → { exists, flags, shifts, ros, updatedAt } or throws.
  async peek() {
    if (!this.configured()) throw new Error('No sync server is set up in this version');
    if (!String(this.identity().employeeNumber || '').trim()) throw new Error('Enter your employee number first');
    const keys = deriveKeys(this.identity().storeId, this.identity().employeeNumber);
    const got = await this.request('GET', `${this.url}/v1/store/${keys.id}`);
    if (got?.status === 404) return { exists: false };
    if (!got || got.status !== 200) throw new Error(`server said ${got?.status}`);
    if (!got.body?.data) return { exists: false };
    const recs = decrypt(keys.key, got.body.data).records || {};
    const live = Object.entries(recs).filter(([, r]) => !r.del);
    const count = (p) => live.filter(([k]) => k.startsWith(p)).length;
    const newest = Math.max(0, ...live.map(([, r]) => (r.t > 1 ? r.t : 0)));
    return { exists: live.length > 0, flags: count('f/'), shifts: count('s/'), ros: count('r/'), updatedAt: newest || null };
  }

  // "Restore": replace this computer's hours, flags and clock with the server's copy
  async restoreFromServer() {
    const keys = this.keys();
    if (!keys) throw new Error('Sync is off');
    const got = await this.request('GET', `${this.url}/v1/store/${keys.id}`);
    if (!got || got.status !== 200) throw new Error(`server said ${got?.status}`);
    const remote = got.body?.data ? (decrypt(keys.key, got.body.data).records || {}) : {};
    this.applying = true;
    try {
      applyRecords(this.tracker.data, remote);
      const journal = {};
      let maxT = this.state.maxT || 0;
      for (const [k, r] of Object.entries(remote)) {
        journal[k] = r.del ? { t: r.t, d: r.d, del: 1 } : { t: r.t, d: r.d, h: hashOf(r.v) };
        if (r.t > maxT) maxT = r.t;
      }
      this.state = { ...this.state, who: keys.id, version: got.body.version || 0, journal, maxT, joining: false, dirty: false };
      this.tracker.save();
    } finally { this.applying = false; }
    try { this.onApplied(); } catch { /* best effort */ }
    this.persist();
    return this.syncNow(); // anything restore tidied up (e.g. joined shifts) goes back, and status updates
  }

  // "Overwrite": this computer's copy replaces the server's (what's only on the server is deleted there)
  async overwriteServer() {
    const keys = this.keys();
    if (!keys) throw new Error('Sync is off');
    const got = await this.request('GET', `${this.url}/v1/store/${keys.id}`);
    if (!got || (got.status !== 200 && got.status !== 404)) throw new Error(`server said ${got?.status}`);
    const remote = got.status === 200 && got.body?.data ? (decrypt(keys.key, got.body.data).records || {}) : {};
    // Newer than anything on the server, so every local record wins and every server-only record is deleted
    const top = Math.max(this.now(), this.state.maxT || 0, ...Object.values(remote).map((r) => r.t || 0));
    this.state = { ...this.state, who: keys.id, version: 0, journal: {}, maxT: top, joining: false, dirty: true };
    this.stamp();
    for (const k of Object.keys(remote)) {
      if (!this.state.journal[k]) this.state.journal[k] = { t: this.stampT(), d: this.state.deviceId, del: 1 };
    }
    this.persist();
    return this.syncNow();
  }

  syncNow() {
    if (this.running) { this.again = true; return this.running; }
    this.running = this.run().finally(() => {
      this.running = null;
      if (this.again) { this.again = false; this.syncNow(); }
    });
    return this.running;
  }

  async run() {
    if (this.paused) return;
    if (!this.configured()) { this.setStatus({ state: 'unconfigured', message: 'No sync server is set up in this version' }); return; }
    const keys = this.keys();
    if (!keys) {
      const who = this.identity();
      this.setStatus(who?.enabled
        ? { state: 'error', message: 'Enter your employee number in Settings to sync' }
        : { state: 'off', message: '' });
      return;
    }
    // A different person on this computer: start a fresh history for them (the hours file is merged in)
    if (this.state.who !== keys.id) {
      this.state = { ...this.state, who: keys.id, version: 0, journal: {}, maxT: this.state.maxT || 0, dirty: true, joining: true };
    }
    this.setStatus({ state: 'syncing', message: '' });
    try {
      let local = this.localRecords();
      let got = await this.request('GET', `${this.url}/v1/store/${keys.id}`);
      for (let attempt = 0; attempt < 5; attempt++) {
        if (!got || got.status !== 200 && got.status !== 409) throw new Error(`server said ${got?.status}`);
        const remoteDoc = got.body?.data ? decrypt(keys.key, got.body.data) : { records: {} };
        const remote = remoteDoc.records || {};
        const merged = mergeRecords(local, remote);

        // Bring this computer up to date if anything came from elsewhere
        const fromElsewhere = Object.entries(merged).some(([k, r]) => r !== local[k]);
        if (fromElsewhere) {
          this.applying = true;
          try {
            applyRecords(this.tracker.data, merged);
            this.state.journal = {};
            for (const [k, r] of Object.entries(merged)) {
              this.state.journal[k] = r.del ? { t: r.t, d: r.d, del: 1 } : { t: r.t, d: r.d, h: hashOf(r.v) };
              if (r.t > (this.state.maxT || 0)) this.state.maxT = r.t;
            }
            this.tracker.save();
          } finally { this.applying = false; }
          try { this.onApplied(); } catch { /* best effort */ }
        }
        local = this.localRecords(); // includes anything the merge itself tidied up (e.g. joined shifts)

        const needsUpload = Object.keys(local).some((k) => !remote[k] || remote[k].t !== local[k].t || remote[k].d !== local[k].d)
          || Object.keys(remote).some((k) => !local[k]);
        if (!needsUpload) {
          this.state.version = got.body.version || 0;
          break;
        }
        const put = await this.request('PUT', `${this.url}/v1/store/${keys.id}`, {
          base: got.body.version || 0,
          data: encrypt(keys.key, { format: FORMAT, records: local }),
        });
        if (put?.status === 200) { this.state.version = put.body.version; break; }
        if (put?.status === 409) { got = put; continue; } // someone else saved first: merge their copy and retry
        throw new Error(`server said ${put?.status}`);
      }
      this.state.dirty = false;
      this.state.lastSync = this.now();
      this.persist();
      this.setStatus({ state: 'ok', message: '', lastSync: this.state.lastSync, changed: true });
    } catch (e) {
      this.persist();
      const msg = String(e?.message || e);
      this.setStatus({
        state: 'error',
        message: /fetch|network|ENOTFOUND|ECONN|ETIMEDOUT|net::/i.test(msg) ? "Can't reach the sync server" : msg.includes('authenticate') || msg.includes('unknown format')
          ? "Couldn't read the synced data" : msg.slice(0, 120),
      });
    }
  }

  // Before closing: send anything not yet sent (give up after a few seconds)
  async flush(timeoutMs = 4000) {
    if (!this.keys() || !this.configured()) return;
    this.stamp();
    if (!this.state.dirty) return;
    await Promise.race([this.syncNow(), new Promise((r) => setTimeout(r, timeoutMs))]);
  }
}

module.exports = { SyncEngine, deriveKeys, encrypt, decrypt, recordsFrom, mergeRecords, applyRecords, normalizeShifts };