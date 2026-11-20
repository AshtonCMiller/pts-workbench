// PTS Workbench on a phone.
//
// Pairing: the desktop app's "Pair phone" shows a QR code of this page's address with `#pair=v1.<sync ID>.<key>`
// after it (the part after # never reaches the server). Opened from the camera, or scanned from inside the page,
// it's kept on this phone only. Typing the store ID and employee number works out the same thing, the way the
// desktop app does (crates/sync derive_keys / the Electron app's sync.js).
//
// From those, the push ID and send token (the desktop app works out the same ones): the server keeps the phone's
// subscription under them, and the desktop app sends to it.

const $ = (id) => document.getElementById(id);
const enc = new TextEncoder();
const hex = (bytes) => [...new Uint8Array(bytes)].map((b) => b.toString(16).padStart(2, '0')).join('');
const sha256hex = async (s) => hex(await crypto.subtle.digest('SHA-256', enc.encode(s)));
const JSQR = 'https://cdn.jsdelivr.net/npm/jsqr@1.4.0/dist/jsQR.js';

const ios = /iPhone|iPad|iPod/.test(navigator.userAgent) || (navigator.platform === 'MacIntel' && navigator.maxTouchPoints > 1);
const standalone = navigator.standalone === true || matchMedia('(display-mode: standalone)').matches;
const canPush = 'serviceWorker' in navigator && 'PushManager' in window && 'Notification' in window;

// ---------- what's kept on this phone ----------

const store = {
  get(k) { try { return JSON.parse(localStorage.getItem(k)); } catch { return null; } },
  set(k, v) { try { localStorage.setItem(k, JSON.stringify(v)); } catch { /* private mode */ } },
  drop(k) { try { localStorage.removeItem(k); } catch { /* private mode */ } },
};

/** "v1.<64 hex>.<64 hex>" → { id, key } */
function readPairCode(text) {
  const m = String(text || '').match(/(?:#pair=|^)(v1\.[a-f0-9]{64}\.[a-f0-9]{64})/);
  if (!m) return null;
  const [, id, key] = m[1].split('.');
  return { id, key };
}

/** The store ID and employee number, worked out as the desktop app does */
async function derive(storeId, employee) {
  let emp = String(employee).trim();
  emp = emp.replace(/^0+(?=\d)/, '');
  const st = String(storeId).trim();
  if (!emp || !st) return null;
  const material = `pts-workbench-sync-v1|${st}|${emp}`;
  const id = await sha256hex(`id|${material}`);
  const ikm = await crypto.subtle.importKey('raw', enc.encode(material), 'HKDF', false, ['deriveBits']);
  const key = await crypto.subtle.deriveBits(
    { name: 'HKDF', hash: 'SHA-256', salt: enc.encode('pts-workbench'), info: enc.encode('encrypt') }, ikm, 256);
  return { id, key: hex(key) };
}

/** The push ID and send token for a pairing (the desktop app's crates/sync push_ids) */
async function pushIds(pair) {
  return {
    pid: await sha256hex(`pts-push|${pair.id}`),
    token: await sha256hex(`pts-push-token|${pair.key}`),
  };
}

// ---------- the server ----------

async function api(path, body) {
  const res = await fetch(path, body === undefined ? {} : {
    method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(body),
  });
  const data = await res.json().catch(() => ({}));
  if (!res.ok) throw new Error(data.error || `the server said ${res.status}`);
  return data;
}

function b64urlBytes(s) {
  const t = s.replace(/-/g, '+').replace(/_/g, '/');
  return Uint8Array.from(atob(t + '='.repeat((4 - (t.length % 4)) % 4)), (c) => c.charCodeAt(0));
}

function phoneName() {
  const ua = navigator.userAgent;
  if (/iPad/.test(ua)) return 'iPad';
  if (/iPhone/.test(ua)) return 'iPhone';
  if (/Android/.test(ua)) return 'Android phone';
  return 'Phone';
}

// ---------- the views ----------

let current = null;
const order = ['install', 'pair', 'type', 'scan', 'home', 'sheet'];
function show(name) {
  if (current === name) return;
  const back = current && order.indexOf(name) < order.indexOf(current);
  document.querySelectorAll('.view').forEach((v) => v.classList.remove('on', 'back'));
  const v = $(`v-${name}`);
  v.classList.toggle('back', !!back);
  // (restarts its animation)
  void v.offsetWidth;
  v.classList.add('on');
  current = name;
}

let toastTimer;
function toast(msg, bad = false) {
  const t = $('toast');
  t.textContent = msg;
  t.classList.toggle('bad', bad);
  t.classList.add('on');
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => t.classList.remove('on'), 3200);
}

// ---------- notifications ----------

async function registration() {
  return navigator.serviceWorker.register('sw.js', { scope: './' });
}

/** Subscribed (or re-subscribed: the phone's push address can change) and the server told */
async function subscribe(ask) {
  const pair = store.get('pair');
  if (!pair || !canPush) return false;
  if (Notification.permission === 'denied') return false;
  if (Notification.permission !== 'granted') {
    if (!ask) return false;
    const p = await Notification.requestPermission();
    if (p !== 'granted') return false;
  }
  const reg = await registration();
  await navigator.serviceWorker.ready;
  let sub = await reg.pushManager.getSubscription();
  if (!sub) {
    const { key } = await api('/v1/push/key');
    sub = await reg.pushManager.subscribe({ userVisibleOnly: true, applicationServerKey: b64urlBytes(key) });
  }
  const ids = await pushIds(pair);
  await api('/v1/push/subscribe', { ...ids, subscription: sub.toJSON(), name: phoneName() });
  store.set('endpoint', sub.endpoint);
  return true;
}

function showNotifyState(on) {
  const pill = $('n-state');
  pill.textContent = on ? 'On' : 'Off';
  pill.classList.toggle('on', on);
  $('b-notify').style.display = on ? 'none' : '';
  $('b-test').style.display = on ? '' : 'none';
  $('n-about').textContent = on
    ? 'When something on your ROs is waiting on you and you haven’t seen it on your computer, it comes here.'
    : 'When something on your ROs is waiting on you and you haven’t seen it on your computer, it comes to your phone.';
  if (on) return;
  if (!canPush) {
    $('n-about').textContent = ios && !standalone
      ? 'Open PTS Workbench from your Home Screen to turn on notifications.'
      : "This browser can't show notifications. On an iPhone, it needs iOS 16.4 or newer.";
    $('b-notify').style.display = 'none';
  } else if (Notification.permission === 'denied') {
    $('n-about').textContent = ios
      ? 'Notifications are turned off for PTS Workbench. Turn them on in Settings › Notifications › PTS Workbench.'
      : "Notifications are blocked for this site. Allow them in your browser's site settings.";
    $('b-notify').style.display = 'none';
  }
}

function logPush(title, body) {
  const li = document.createElement('li');
  const b = document.createElement('b');
  b.textContent = title;
  const s = document.createElement('span');
  s.textContent = body;
  li.append(b, s);
  $('log').prepend(li);
  while ($('log').children.length > 5) $('log').lastChild.remove();
}

// ---------- pairing ----------

async function paired(pair) {
  store.set('pair', pair);
  // a pairing in the address isn't left there (it'd be in the history, and shared if the page is)
  if (location.hash) history.replaceState(null, '', location.pathname);
  if (ios && !standalone) { show('install'); return; }
  show('home');
  let on = false;
  try { on = await subscribe(false); } catch (e) { toast(`Couldn't reach the server (${e.message}).`, true); }
  showNotifyState(on);
}

let scanning = null;
async function startScan() {
  show('scan');
  $('camera').classList.remove('found');
  $('scan-msg').textContent = 'Point the camera at the code on your computer.';
  try {
    if (!window.jsQR) {
      await new Promise((ok, fail) => {
        const s = document.createElement('script');
        s.src = JSQR; s.onload = ok; s.onerror = () => fail(new Error('the code reader didn’t load'));
        document.head.append(s);
      });
    }
    const stream = await navigator.mediaDevices.getUserMedia({ video: { facingMode: 'environment' }, audio: false });
    const video = $('video');
    video.srcObject = stream;
    await video.play();
    const canvas = document.createElement('canvas');
    const ctx = canvas.getContext('2d', { willReadFrequently: true });
    scanning = { stream, stop: false };
    const look = () => {
      if (!scanning || scanning.stop) return;
      if (video.readyState >= 2 && video.videoWidth) {
        const scale = Math.min(1, 720 / video.videoWidth);
        canvas.width = Math.round(video.videoWidth * scale);
        canvas.height = Math.round(video.videoHeight * scale);
        ctx.drawImage(video, 0, 0, canvas.width, canvas.height);
        const img = ctx.getImageData(0, 0, canvas.width, canvas.height);
        const code = window.jsQR(img.data, img.width, img.height, { inversionAttempts: 'attemptBoth' });
        const pair = code && readPairCode(code.data);
        if (pair) {
          $('camera').classList.add('found');
          if (navigator.vibrate) navigator.vibrate(40);
          stopScan();
          setTimeout(() => paired(pair), 450);
          return;
        }
        if (code) $('scan-msg').textContent = 'That code isn’t from PTS Workbench. Use the one under Pair phone.';
      }
      requestAnimationFrame(look);
    };
    requestAnimationFrame(look);
  } catch (e) {
    stopScan();
    show('pair');
    toast(e && e.name === 'NotAllowedError'
      ? 'The camera wasn’t allowed. You can type it in instead.'
      : `Couldn't start the camera (${e.message || e}).`, true);
  }
}
function stopScan() {
  if (!scanning) return;
  scanning.stop = true;
  scanning.stream.getTracks().forEach((t) => t.stop());
  $('video').srcObject = null;
  scanning = null;
}

// ---------- buttons ----------

$('b-scan').onclick = startScan;
$('b-scan-cancel').onclick = () => { stopScan(); show('pair'); };
$('b-type').onclick = () => { show('type'); setTimeout(() => $('i-store').focus(), 300); };
$('b-type-cancel').onclick = () => show('pair');
$('f-type').onsubmit = async (e) => {
  e.preventDefault();
  const pair = await derive($('i-store').value, $('i-emp').value);
  if (!pair) { toast('Fill in both.', true); return; }
  paired(pair);
};

$('b-notify').onclick = async () => {
  const b = $('b-notify');
  b.disabled = true;
  try {
    const on = await subscribe(true);
    showNotifyState(on);
    if (on) toast('Notifications are on.');
    else if (Notification.permission !== 'denied') toast('Notifications weren’t allowed.', true);
  } catch (e) {
    toast(`Couldn't turn them on (${e.message}).`, true);
  } finally {
    b.disabled = false;
  }
};

$('b-test').onclick = async () => {
  const pair = store.get('pair');
  if (!pair) return;
  const b = $('b-test');
  b.disabled = true;
  try {
    const r = await api('/v1/push/send', { ...(await pushIds(pair)), title: 'PTS Workbench', body: 'This is a test. Notifications work!', tag: 'test' });
    toast(r.sent ? 'Sent. It should show in a moment.' : 'Nothing was sent: turn notifications on first.', !r.sent);
  } catch (e) {
    toast(`Couldn't send it (${e.message}).`, true);
  } finally {
    setTimeout(() => { b.disabled = false; }, 1500);
  }
};

$('b-unpair').onclick = async () => {
  const pair = store.get('pair');
  if (!confirm('Unpair this phone? It won’t get notifications from your computer anymore.')) return;
  try {
    const endpoint = store.get('endpoint');
    if (pair && endpoint) await api('/v1/push/unpair', { ...(await pushIds(pair)), endpoint });
    if (canPush) {
      const reg = await navigator.serviceWorker.getRegistration();
      const sub = reg && await reg.pushManager.getSubscription();
      if (sub) await sub.unsubscribe();
    }
  } catch { /* forgotten here anyway */ }
  store.drop('pair');
  store.drop('endpoint');
  show('pair');
};

if (canPush) {
  navigator.serviceWorker.addEventListener('message', (e) => {
    if (e.data && e.data.kind === 'push') logPush(e.data.title, e.data.body);
  });
}

// ---------- your synced flags (read, and changed once you say so) ----------
//
// The same document the desktop app syncs (crates/sync: GET / PUT /v1/store/<sync ID>): "v1." and base64 of IV + tag
// + ciphertext, AES-256-GCM over raw deflate of { format, records }. A flag is the record "f/<RO>/<line key>":
// { dms (its line letter), label, ymm, hours, flaggedAt, confirmed }.

function b64(bytes) {
  let s = '';
  for (const b of bytes) s += String.fromCharCode(b);
  return btoa(s);
}
function unb64(t) {
  return Uint8Array.from(atob(t.replace(/\s/g, '')), (c) => c.charCodeAt(0));
}
const fromHex = (h) => Uint8Array.from(h.match(/../g).map((x) => parseInt(x, 16)));

async function squeeze(bytes, kind, how) {
  const stream = new Blob([bytes]).stream().pipeThrough(new (how === 'in' ? DecompressionStream : CompressionStream)(kind));
  return new Uint8Array(await new Response(stream).arrayBuffer());
}

async function storeKey(pair) {
  return crypto.subtle.importKey('raw', fromHex(pair.key), 'AES-GCM', false, ['encrypt', 'decrypt']);
}

/** The synced document as it is: its version and its records */
async function readStore(pair) {
  const res = await fetch(`/v1/store/${pair.id}`, { cache: 'no-store' });
  if (!res.ok) throw new Error(`the server said ${res.status}`);
  const got = await res.json();
  if (!got.data) return { version: got.version || 0, doc: { format: 1, records: {} } };
  return { version: got.version, doc: await openDoc(pair, got.data) };
}

async function openDoc(pair, data) {
  if (!data.startsWith('v1.')) throw new Error('unknown format');
  const raw = unb64(data.slice(3));
  const iv = raw.slice(0, 12);
  const tag = raw.slice(12, 28);
  const ct = raw.slice(28);
  const sealed = new Uint8Array(ct.length + 16);
  sealed.set(ct); sealed.set(tag, ct.length);
  const plain = new Uint8Array(await crypto.subtle.decrypt({ name: 'AES-GCM', iv }, await storeKey(pair), sealed));
  return JSON.parse(new TextDecoder().decode(await squeeze(plain, 'deflate-raw', 'in')));
}

async function sealDoc(pair, doc) {
  const plain = await squeeze(enc.encode(JSON.stringify(doc)), 'deflate-raw', 'out');
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const sealed = new Uint8Array(await crypto.subtle.encrypt({ name: 'AES-GCM', iv }, await storeKey(pair), plain));
  const out = new Uint8Array(12 + sealed.length);
  // (WebCrypto puts the tag last; the desktop apps want it right after the IV)
  out.set(iv); out.set(sealed.slice(-16), 12); out.set(sealed.slice(0, -16), 28);
  return `v1.${b64(out)}`;
}

/** Your flags from the synced document */
function flagsOf(doc) {
  const out = [];
  for (const [k, r] of Object.entries(doc.records || {})) {
    if (!k.startsWith('f/') || r.del || !r.v) continue;
    const [, ro, ...rest] = k.split('/');
    out.push({ k, ro, key: rest.join('/'), dms: String(r.v.dms || '').toUpperCase(), label: r.v.label || '',
      hours: Number(r.v.hours) || 0, flaggedAt: Number(r.v.flaggedAt) || 0, confirmed: !!r.v.confirmed });
  }
  return out;
}

/** Changes the flags (each change: its record's key and the fields to set), merged with whatever changed meanwhile */
async function changeFlags(pair, changes) {
  for (let tries = 0; tries < 4; tries++) {
    const { version, doc } = await readStore(pair);
    const now = Date.now();
    let n = 0;
    for (const ch of changes) {
      const rec = doc.records[ch.k];
      if (!rec || rec.del || !rec.v) continue;
      doc.records[ch.k] = { t: Math.max(now, (rec.t || 0) + 1), d: 'phone', v: { ...rec.v, ...ch.set } };
      n++;
    }
    if (!n) return 0;
    const res = await fetch(`/v1/store/${pair.id}`, {
      method: 'PUT', headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ base: version, data: await sealDoc(pair, doc) }),
    });
    if (res.ok) return n;
    if (res.status !== 409) throw new Error(`the server said ${res.status}`);
    // someone saved first: read again, and again
  }
  throw new Error('the server was busy: try again');
}

// ---------- the pay sheet ----------

const TESSERACT = 'https://cdn.jsdelivr.net/npm/tesseract.js@5.1.1/dist/tesseract.min.js';
let reader = null;
let sheet = store.get('sheet') || { rows: [], period: null, pages: 0 };
let flags = null;
let flagsError = '';
let result = null;
let changes = [];

function loadScript(src) {
  return new Promise((ok, fail) => {
    const s = document.createElement('script');
    s.src = src; s.onload = ok; s.onerror = () => fail(new Error('the text reader didn’t load (no internet?)'));
    document.head.append(s);
  });
}

function step(text, pct) {
  $('sh-busy').style.display = '';
  $('sh-step').textContent = text;
  $('sh-bar').style.width = `${Math.round(pct * 100)}%`;
}

/** The photo, upright, gray and with its contrast stretched, `long` pixels on its long side */
async function prepared(bitmap, long) {
  const scale = long / Math.max(bitmap.width, bitmap.height);
  const c = document.createElement('canvas');
  c.width = Math.round(bitmap.width * scale);
  c.height = Math.round(bitmap.height * scale);
  const ctx = c.getContext('2d', { willReadFrequently: true });
  ctx.imageSmoothingQuality = 'high';
  ctx.drawImage(bitmap, 0, 0, c.width, c.height);
  const img = ctx.getImageData(0, 0, c.width, c.height);
  const d = img.data;
  const hist = new Uint32Array(256);
  for (let i = 0; i < d.length; i += 4) {
    const g = (d[i] * 299 + d[i + 1] * 587 + d[i + 2] * 114) / 1000 | 0;
    d[i] = g; hist[g]++;
  }
  const total = d.length / 4;
  let lo = 0; let hi = 255; let acc = 0;
  for (; lo < 255 && (acc += hist[lo]) < total * 0.01; lo++);
  acc = 0;
  for (; hi > 0 && (acc += hist[hi]) < total * 0.01; hi--);
  const span = Math.max(1, hi - lo);
  for (let i = 0; i < d.length; i += 4) {
    const v = Math.max(0, Math.min(255, ((d[i] - lo) * 255) / span));
    d[i] = d[i + 1] = d[i + 2] = v;
  }
  ctx.putImageData(img, 0, 0);
  return c;
}

async function readPage(file) {
  show('sheet');
  $('sh-stats').innerHTML = '';
  step('Getting the text reader ready…', 0.03);
  if (!window.Tesseract) await loadScript(TESSERACT);
  let pass = 0;
  if (!reader) {
    reader = await Tesseract.createWorker('eng', 1, {
      logger: (m) => {
        if (m.status === 'recognizing text') step(`Reading the page (${pass} of 2)…`, 0.1 + (pass - 1 + m.progress) * 0.42);
        else if (/load|initializ/i.test(m.status || '')) step('Getting the text reader ready…', 0.05 + (m.progress || 0) * 0.05);
      },
    });
    await reader.setParameters({ tessedit_pageseg_mode: '6', preserve_interword_spaces: '1' });
  }
  const bitmap = await createImageBitmap(file);
  const long = Math.max(bitmap.width, bitmap.height);
  const texts = [];
  for (const size of [Math.min(long, 2016), Math.min(Math.round(long * 1.5), 3200)]) {
    pass++;
    step(`Reading the page (${pass} of 2)…`, 0.1 + (pass - 1) * 0.42);
    const { data } = await reader.recognize(await prepared(bitmap, size));
    texts.push(data.text);
  }
  const page = Sheet.combine(Sheet.parse(texts[0]), Sheet.parse(texts[1]));
  if (!page.rows.length) {
    $('sh-busy').style.display = 'none';
    toast('No rows found. Take the picture straight on, with the whole page in it.', true);
    render();
    return;
  }
  sheet = Sheet.addPage(sheet, page);
  store.set('sheet', sheet);
  step('Checking it against your flags…', 0.97);
  await loadFlags();
  $('sh-busy').style.display = 'none';
  render();
}

async function loadFlags() {
  const pair = store.get('pair');
  flagsError = '';
  try {
    flags = flagsOf((await readStore(pair)).doc);
  } catch (e) {
    flags = null;
    flagsError = e.message === 'unknown format' || e.name === 'OperationError'
      ? 'Your synced hours couldn’t be read. Pair this phone again.'
      : `Your flags couldn’t be loaded (${e.message}).`;
  }
}

const h2 = (n) => `${(Math.round(n * 100) / 100).toFixed(2)} h`;
function niceDay(iso) {
  if (!iso) return '';
  const [y, m, d] = iso.split('-').map(Number);
  return new Date(y, m - 1, d).toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
}
const dayOfMs = (ms) => {
  const d = new Date(ms);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
};
const esc = (t) => String(t).replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));

/** What the scan says to change: confirmed, the hours paid, the day it was booked */
function proposed(r) {
  const out = [];
  const add = (flag, set, text, why, on) => out.push({ k: flag.k, set, text, why, on, id: `${flag.k}|${Object.keys(set).join()}` });
  for (const p of r.paid) {
    if (!p.flag.confirmed) add(p.flag, { confirmed: true }, `RO ${p.flag.ro} line ${p.line.ln}: paid`, `Marked as on your time sheet (${h2(p.flag.hours)})`, true);
  }
  for (const d of r.different) {
    add(d.flag, { hours: d.sold, confirmed: true }, `RO ${d.flag.ro} line ${d.line.ln}: ${h2(d.flag.hours)} → ${h2(d.sold)}`,
      d.unsure ? 'What the sheet says was paid, but the photo wasn’t clear: check the paper first' : 'What the sheet says was paid', !d.unsure);
  }
  for (const p of [...r.paid, ...r.different]) {
    const booked = p.line.booked;
    if (!booked || !p.flag.flaggedAt || dayOfMs(p.flag.flaggedAt) === booked) continue;
    const was = new Date(p.flag.flaggedAt);
    const [y, m, d] = booked.split('-').map(Number);
    const at = new Date(y, m - 1, d, was.getHours(), was.getMinutes()).getTime();
    const sure = p.line.rows.some((x) => x.bookedSure);
    add(p.flag, { flaggedAt: at }, `RO ${p.flag.ro} line ${p.line.ln}: ${niceDay(dayOfMs(p.flag.flaggedAt))} → ${niceDay(booked)}`,
      sure ? 'Moved to the day it was booked' : 'Moved to the day it was booked (the date was hard to read: check it)', sure);
  }
  return out;
}

function render() {
  const n = sheet.pages || 0;
  const period = Sheet.periodOf(sheet);
  $('sh-title').textContent = period ? `Pay sheet · ${niceDay(period.from)} – ${niceDay(period.to)}` : 'Pay sheet';
  $('sh-about').textContent = n
    ? `${n} page${n === 1 ? '' : 's'} scanned, ${sheet.rows.length} rows read.${flagsError ? ` ${flagsError}` : ''}`
    : 'Scan each page of your Technician Payroll Report.';
  $('b-sheet-more').textContent = n ? 'Scan another page' : 'Scan a page';
  $('b-sheet-clear').style.display = n ? '' : 'none';
  const lists = $('sh-lists');
  lists.innerHTML = '';
  $('sh-changes').style.display = 'none';
  if (!n) { $('sh-stats').innerHTML = ''; return; }
  result = Sheet.compare(sheet, flags || []);
  const stat = (v, label, cls) => `<div class="stat ${cls}"><b>${v}</b><span>${label}</span></div>`;
  $('sh-stats').innerHTML = flags
    ? stat(`${result.paid.length}<span class="count">of ${result.flags}</span>`, 'flagged lines paid', result.paid.length === result.flags ? 'good' : '')
      + stat(result.different.length, 'paid different hours', result.different.length ? 'warn' : '')
      + stat(result.missing.length, 'not on these pages', result.missing.length ? 'bad' : '')
      + stat(h2(result.soldTotal), `sold · you flagged ${h2(result.flaggedTotal)}`, '')
    : stat(h2(result.soldTotal), 'sold on these pages', '');

  const section = (title, items, open, row) => {
    if (!items.length) return;
    const d = document.createElement('details');
    d.className = 'card';
    d.open = open;
    d.innerHTML = `<summary><h2>${esc(title)}<span class="count">${items.length}</span></h2></summary><ul class="items"></ul>`;
    const ul = d.querySelector('ul');
    items.forEach((it, i) => {
      const li = document.createElement('li');
      li.style.animationDelay = `${Math.min(i, 12) * 0.03}s`;
      li.innerHTML = row(it);
      ul.append(li);
    });
    lists.append(d);
  };
  const raw = (rows) => rows.map((x) => `<code>${esc(x.raw)}</code>`).join('');
  section('Paid different hours', result.different, true, (d) => `<div class="what"><b>RO ${esc(d.flag.ro)} · line ${esc(d.line.ln)}</b>
    <span>${esc(d.flag.label)} · booked ${esc(niceDay(d.line.booked))}</span>
    <span>You flagged ${h2(d.flag.hours)}${d.unsure ? ' · the photo wasn’t clear: check the paper' : ''}</span>${d.unsure ? raw(d.line.rows) : ''}</div>
    <div class="hrs">${h2(d.sold)}</div>`);
  section('Not on these pages', result.missing, true, (m) => `<div class="what"><b>RO ${esc(m.flag.ro)} · line ${esc(m.flag.dms || '?')}</b>
    <span>${esc(m.flag.label)} · flagged ${esc(niceDay(dayOfMs(m.flag.flaggedAt)))}</span>
    <span>On another page, or not paid yet</span></div><div class="hrs">${h2(m.flag.hours)}</div>`);
  section('Paid, but not flagged here', result.unflagged, false, (u) => `<div class="what"><b>RO ${esc(u.line.ro)} · line ${esc(u.line.ln)}</b>
    <span>Booked ${esc(niceDay(u.line.booked))}${u.unsure ? ' · check the paper' : ''}</span></div><div class="hrs">${h2(u.sold)}</div>`);
  section('Paid', result.paid, false, (p) => `<div class="what"><b>RO ${esc(p.flag.ro)} · line ${esc(p.line.ln)}</b>
    <span>${esc(p.flag.label)} · booked ${esc(niceDay(p.line.booked))}${p.flag.confirmed ? ' · already marked' : ''}</span></div>
    <div class="hrs">${h2(p.sold)}</div>`);
  section('Rows that couldn’t be read', result.unread, false, (r) => `<div class="what"><b>${r.ro ? `RO ${esc(r.ro)}…` : 'A row'}</b>
    <span>Check this one on the paper</span><code>${esc(r.raw)}</code></div>`);

  // what to change, shown first
  changes = flags ? proposed(result) : [];
  const list = $('sh-change-list');
  list.innerHTML = '';
  if (changes.length) {
    $('sh-changes').style.display = '';
    changes.forEach((c, i) => {
      const li = document.createElement('li');
      li.style.animationDelay = `${Math.min(i, 12) * 0.03}s`;
      li.innerHTML = `<label class="switch"><input type="checkbox" ${c.on ? 'checked' : ''}><i></i></label>
        <div class="what"><b>${esc(c.text)}</b><span>${esc(c.why)}</span></div>`;
      li.querySelector('input').onchange = (e) => { c.on = e.target.checked; applyLabel(); };
      list.append(li);
    });
    applyLabel();
  }
}

function applyLabel() {
  const n = changes.filter((c) => c.on).length;
  const b = $('b-apply');
  b.textContent = n ? `Apply ${n} change${n === 1 ? '' : 's'}` : 'Nothing to apply';
  b.disabled = !n;
}

$('b-sheet').onclick = () => $('f-sheet').click();
$('b-sheet-more').onclick = () => $('f-sheet').click();
$('f-sheet').onchange = async (e) => {
  const file = e.target.files && e.target.files[0];
  e.target.value = '';
  if (!file) return;
  try { await readPage(file); } catch (err) {
    $('sh-busy').style.display = 'none';
    toast(`Couldn't read it (${err.message}).`, true);
    render();
  }
};
$('b-sheet-clear').onclick = () => {
  sheet = { rows: [], period: null, pages: 0 };
  store.drop('sheet');
  render();
};
$('b-sheet-done').onclick = () => show('home');
$('b-apply').onclick = async () => {
  const pick = changes.filter((c) => c.on);
  if (!pick.length) return;
  const b = $('b-apply');
  b.disabled = true;
  b.textContent = 'Applying…';
  try {
    const n = await changeFlags(store.get('pair'), pick);
    toast(n ? `Done: ${n} change${n === 1 ? '' : 's'}. Your computer picks ${n === 1 ? 'it' : 'them'} up at its next sync.` : 'Nothing changed (those lines are gone).');
    await loadFlags();
    render();
  } catch (e) {
    toast(`Couldn't apply them (${e.message}).`, true);
    applyLabel();
  }
};

// ---------- starting ----------

(async () => {
  const fromLink = readPairCode(location.hash);
  if (fromLink) { paired(fromLink); return; }
  const pair = store.get('pair');
  if (pair) { paired(pair); return; }
  show(ios && !standalone ? 'install' : 'pair');
})();
