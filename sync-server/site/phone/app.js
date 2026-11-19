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
const order = ['install', 'pair', 'type', 'scan', 'home'];
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

// ---------- starting ----------

(async () => {
  const fromLink = readPairCode(location.hash);
  if (fromLink) { paired(fromLink); return; }
  const pair = store.get('pair');
  if (pair) { paired(pair); return; }
  show(ios && !standalone ? 'install' : 'pair');
})();