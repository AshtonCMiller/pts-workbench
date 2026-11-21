// The phone page's state and what it does, shared by its views: which view shows, the pairing, your hours and RO
// list (refreshed every minute while it's open), notifications, and the pay sheet being checked.

import { reactive, shallowRef, triggerRef, toRaw } from 'vue';
import { store } from './lib/store.js';
import { readPairCode, pushIds } from './lib/keys.js';
import { api, b64urlBytes } from './lib/server.js';
import { readStore, writeStore, readRos, live, put, flagsOf, changeFlags } from './lib/doc.js';
import { shiftsOf } from './lib/hours.js';
import { readPage } from './lib/ocr.js';
import { Sheet } from './lib/sheet.js';

export const ios = /iPhone|iPad|iPod/.test(navigator.userAgent) || (navigator.platform === 'MacIntel' && navigator.maxTouchPoints > 1);
export const standalone = navigator.standalone === true || matchMedia('(display-mode: standalone)').matches;
export const canPush = 'serviceWorker' in navigator && 'PushManager' in window && 'Notification' in window;
// a computer, not a phone or tablet (an iPad says it's a Mac, but has a touch screen)
export const desktop = !standalone
  && !/Android|iPhone|iPad|iPod|Mobile|webOS|BlackBerry|IEMobile|Opera Mini/i.test(navigator.userAgent)
  && !(navigator.platform === 'MacIntel' && navigator.maxTouchPoints > 1);

export const android = /Android/i.test(navigator.userAgent);
// On a phone, it only works once it's installed (iPhone: on the Home Screen; Android: installed from Chrome): until
// then it says how. (Android's installed copy shares what's saved with Chrome, so a pairing carries over; the
// iPhone's doesn't, so it's scanned again there.)
export const mustInstall = (ios || android) && !standalone;

// ---------- installing (Android) ----------

// Chrome on Android offers installing the page (manifest.webmanifest) with its own prompt, which the page can show
// when it likes: kept here when it's offered (it's offered early, so this listens as soon as the page loads)
export const install = reactive({ offer: null, done: standalone });
window.addEventListener('beforeinstallprompt', (e) => {
  e.preventDefault();
  install.offer = e;
});
window.addEventListener('appinstalled', () => {
  install.offer = null;
  install.done = true;
  toast('Workbench is on your Home screen.');
});

export async function installApp() {
  const offer = install.offer;
  if (!offer) return;
  install.offer = null;
  try {
    await offer.prompt();
    const { outcome } = await offer.userChoice;
    if (outcome === 'accepted') install.done = true;
  } catch {
    /* it can only be shown once: Chrome offers it again later */
  }
}

// ---------- the views ----------

const ORDER = ['desktop', 'install', 'pair', 'type', 'scan', 'ros', 'hours', 'home', 'sheet'];
export const TABS = ['ros', 'hours', 'home'];

export const ui = reactive({
  view: null,
  // going to a view earlier in ORDER: it slides in from the other side
  back: false,
  toast: { text: '', bad: false, on: false },
});

export function show(name) {
  if (ui.view === name) return;
  ui.back = !!ui.view && ORDER.indexOf(name) < ORDER.indexOf(ui.view);
  ui.view = name;
  if (TABS.includes(name)) window.scrollTo({ top: 0 });
}

let toastTimer = 0;
export function toast(text, bad = false) {
  Object.assign(ui.toast, { text, bad, on: true });
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => (ui.toast.on = false), 3200);
}

// ---------- your hours and RO list ----------

export const data = reactive({
  pair: store.get('pair'),
  rosDoc: null,
  rosError: '',
  refreshing: false,
  // the ROs opened up on the RO tab (kept while switching tabs)
  open: new Set(),
});
// the hours document ({ version, doc }): big, so not made deeply reactive; changes in place are announced with
// triggerRef
export const hours = shallowRef(null);
export const records = () => (hours.value && hours.value.doc.records) || {};

export async function refresh() {
  const pair = data.pair;
  if (!pair || data.refreshing) return;
  data.refreshing = true;
  try {
    const [h, r] = await Promise.allSettled([readStore(pair), readRos(pair)]);
    if (h.status === 'fulfilled') hours.value = h.value;
    else toast(`Couldn't get your hours (${h.reason.message}).`, true);
    if (r.status === 'fulfilled') {
      data.rosDoc = r.value;
      data.rosError = '';
    } else data.rosError = r.reason.message;
  } finally {
    data.refreshing = false;
  }
}

/** Changes records in the hours document (`edit` changes them in place, returns whether anything changed): shown at
 *  once, then saved, merged with whatever changed meanwhile */
async function editHours(edit) {
  const pair = data.pair;
  if (hours.value) {
    edit(hours.value.doc.records, Date.now());
    triggerRef(hours);
  }
  for (let tries = 0; tries < 4; tries++) {
    const got = await readStore(pair);
    const now = Date.now();
    if (!edit(got.doc.records, now)) {
      hours.value = got;
      return;
    }
    const saved = await writeStore(pair, got.version, got.doc);
    if (saved.ok) {
      hours.value = { version: saved.version, doc: got.doc };
      return;
    }
    if (saved.status !== 409) throw new Error(`the server said ${saved.status}`);
  }
  throw new Error('the server was busy: try again');
}

export async function save(edit, done) {
  try {
    await editHours(edit);
    if (done) toast(done);
  } catch (e) {
    toast(`Couldn't save it (${e.message}).`, true);
    await refresh();
  }
}

export function flagLine(ro, line) {
  const r = records()[`r/${ro}`];
  const ymm = (live(r) && r.v.ymm) || (data.rosDoc?.ros || []).find((x) => x.ro === ro)?.ymm || '';
  return save((recs, now) => {
    const k = `f/${ro}/${line.key}`;
    if (live(recs[k])) return false;
    put(recs, k, { dms: line.dms || '', label: `${line.diag ? 'Diag: ' : ''}${line.text}`, ymm, hours: line.paid, flaggedAt: now, confirmed: false }, now);
    return true;
  });
}

export function unflagLine(ro, key) {
  return save((recs, now) => {
    const k = `f/${ro}/${key}`;
    if (!live(recs[k])) return false;
    put(recs, k, null, now);
    return true;
  });
}

export function confirmFlags(keys, on) {
  return save((recs, now) => {
    let n = 0;
    for (const k of keys) {
      if (!live(recs[k]) || !!recs[k].v.confirmed === on) continue;
      put(recs, k, { ...recs[k].v, confirmed: on }, now);
      n++;
    }
    return n > 0;
  });
}

export function clockInOrOut() {
  const open = shiftsOf(records()).filter((s) => s.end === null).pop();
  return save((recs, now) => {
    const o = Object.entries(recs)
      .filter(([k, x]) => k.startsWith('s/') && live(x) && x.v.end == null)
      .sort((a, b) => a[1].v.start - b[1].v.start)
      .pop();
    if (open) {
      if (!o) return false;
      put(recs, o[0], { ...o[1].v, end: now }, now);
    } else {
      if (o) return false;
      put(recs, `s/${now}`, { start: now, end: null, auto: false }, now);
    }
    return true;
  }, open ? 'Clocked out.' : 'Clocked in.');
}

// ---------- notifications ----------

export const notify = reactive({ on: false, busy: false, testing: false, log: [] });

function phoneName() {
  const ua = navigator.userAgent;
  if (/iPad/.test(ua)) return 'iPad';
  if (/iPhone/.test(ua)) return 'iPhone';
  if (/Android/.test(ua)) return 'Android phone';
  return 'Phone';
}

// the old page's service worker, shared: the same subscription whichever page asks
const registration = () => navigator.serviceWorker.register('/phone/sw.js', { scope: '/phone/' });

/** Subscribed (or re-subscribed: the phone's push address can change) and the server told */
async function subscribe(ask) {
  const pair = data.pair;
  if (!pair || !canPush) return false;
  if (Notification.permission === 'denied') return false;
  if (Notification.permission !== 'granted') {
    if (!ask) return false;
    if ((await Notification.requestPermission()) !== 'granted') return false;
  }
  const reg = await registration();
  await navigator.serviceWorker.ready;
  let sub = await reg.pushManager.getSubscription();
  if (!sub) {
    const { key } = await api('/v1/push/key');
    sub = await reg.pushManager.subscribe({ userVisibleOnly: true, applicationServerKey: b64urlBytes(key) });
  }
  await api('/v1/push/subscribe', { ...(await pushIds(pair)), subscription: sub.toJSON(), name: phoneName() });
  store.set('endpoint', sub.endpoint);
  return true;
}

export async function turnOnNotifications() {
  notify.busy = true;
  try {
    notify.on = await subscribe(true);
    if (notify.on) toast('Notifications are on.');
    else if (Notification.permission !== 'denied') toast('Notifications weren’t allowed.', true);
  } catch (e) {
    toast(`Couldn't turn them on (${e.message}).`, true);
  } finally {
    notify.busy = false;
  }
}

export async function sendTest() {
  if (!data.pair) return;
  notify.testing = true;
  try {
    const r = await api('/v1/push/send', { ...(await pushIds(data.pair)), title: 'Workbench', body: 'This is a test. Notifications work!', tag: 'test' });
    toast(r.sent ? 'Sent. It should show in a moment.' : 'Nothing was sent: turn notifications on first.', !r.sent);
  } catch (e) {
    toast(`Couldn't send it (${e.message}).`, true);
  } finally {
    setTimeout(() => (notify.testing = false), 1500);
  }
}

// ---------- pairing ----------

export async function paired(pair) {
  store.set('pair', pair);
  data.pair = pair;
  // a pairing in the address isn't left there (it'd be in the history, and shared if the page is)
  if (location.hash) history.replaceState(null, '', location.pathname);
  if (mustInstall) {
    show('install');
    return;
  }
  show('ros');
  refresh();
  try {
    notify.on = await subscribe(false);
  } catch (e) {
    notify.on = false;
    toast(`Couldn't reach the server (${e.message}).`, true);
  }
}

export async function unpair() {
  if (!confirm('Unpair this phone? It won’t get notifications from your computer anymore.')) return;
  try {
    const endpoint = store.get('endpoint');
    if (data.pair && endpoint) await api('/v1/push/unpair', { ...(await pushIds(data.pair)), endpoint });
    if (canPush) {
      const reg = await navigator.serviceWorker.getRegistration('/phone/');
      const sub = reg && (await reg.pushManager.getSubscription());
      if (sub) await sub.unsubscribe();
    }
  } catch {
    /* forgotten here anyway */
  }
  store.drop('pair');
  store.drop('endpoint');
  data.pair = null;
  data.rosDoc = null;
  hours.value = null;
  notify.on = false;
  show('pair');
}

// ---------- the pay sheet ----------

const EMPTY_SHEET = () => ({ rows: [], period: null, pages: 0 });

export const sheet = reactive({
  sheet: store.get('sheet') || EMPTY_SHEET(),
  flags: null,
  flagsError: '',
  // while a page is read: { text, pct }
  busy: null,
});

async function loadFlags() {
  sheet.flagsError = '';
  try {
    sheet.flags = flagsOf((await readStore(data.pair)).doc);
  } catch (e) {
    sheet.flags = null;
    sheet.flagsError = e.message === 'unknown format' || e.name === 'OperationError'
      ? 'Your synced hours couldn’t be read. Pair this phone again.'
      : `Your flags couldn’t be loaded (${e.message}).`;
  }
}

export async function scanSheet(file) {
  show('sheet');
  try {
    const page = await readPage(file, (text, pct) => (sheet.busy = { text, pct }));
    if (!page.rows.length) {
      toast('No rows found. Take the picture straight on, with the whole page in it.', true);
      return;
    }
    sheet.sheet = Sheet.addPage(toRaw(sheet.sheet), page);
    store.set('sheet', toRaw(sheet.sheet));
    sheet.busy = { text: 'Checking it against your flags…', pct: 0.97 };
    await loadFlags();
  } catch (e) {
    toast(`Couldn't read it (${e.message}).`, true);
  } finally {
    sheet.busy = null;
  }
}

export function clearSheet() {
  sheet.sheet = EMPTY_SHEET();
  store.drop('sheet');
}

export async function applyChanges(pick) {
  try {
    const n = await changeFlags(data.pair, pick.map((c) => ({ k: c.k, set: { ...c.set } })));
    toast(n ? `Done: ${n} change${n === 1 ? '' : 's'}. Your computer picks ${n === 1 ? 'it' : 'them'} up at its next sync.` : 'Nothing changed (those lines are gone).');
    await loadFlags();
    refresh();
    return true;
  } catch (e) {
    toast(`Couldn't apply them (${e.message}).`, true);
    return false;
  }
}

// ---------- starting ----------

/** On a computer, it says to open it on your phone instead (and `begin` goes on, if you'd rather use it here) */
export function start() {
  if (desktop) show('desktop');
  else begin();
}

let begun = false;
export function begin() {
  if (begun) return;
  begun = true;
  if (canPush) {
    navigator.serviceWorker.addEventListener('message', (e) => {
      if (e.data && e.data.kind === 'push') {
        notify.log.unshift({ id: Date.now() + Math.random(), title: e.data.title, body: e.data.body });
        notify.log.splice(5);
      }
    });
  }
  // kept up to date while it's open
  setInterval(() => {
    if (document.visibilityState === 'visible' && data.pair && TABS.includes(ui.view)) refresh();
  }, 60000);
  document.addEventListener('visibilitychange', () => {
    if (document.visibilityState === 'visible' && data.pair && TABS.includes(ui.view)) refresh();
  });

  const fromLink = readPairCode(location.hash);
  if (fromLink) paired(fromLink);
  else if (data.pair) paired(data.pair);
  else show(mustInstall ? 'install' : 'pair');
}
