// Phone notifications: the PTS Workbench page on a phone (site/phone/) and the desktop app buzzing it.
//
//   GET  /v1/push/key                                   -> { key }       the server's public key, for subscribing
//   POST /v1/push/subscribe { pid, token, subscription, name } -> { ok }  a phone (re)registering itself
//   POST /v1/push/status    { pid, token }              -> { phones: [{ name, since, seen }] }
//   POST /v1/push/send      { pid, token, title, body, tag, url } -> { sent, phones }
//   POST /v1/push/unpair    { pid, token, endpoint? }   -> { removed }   one phone (its endpoint) or all of them
//
// Who's who: the desktop app and the phone both work out, from the person's sync key, a push ID (`pid`, a hash of
// their sync ID) and a send token. The server keeps only a hash of the token, so even with the database in hand
// nobody can send to someone's phone, and it never sees the sync key itself (the phone keeps that to itself).
//
// What's sent: whatever the desktop app puts in a notification (an RO number and what happened to it, never a
// customer, a VIN or a name), encrypted for the phone (RFC 8291) and handed to Apple's, Google's or Mozilla's push
// service, signed with the server's own key (VAPID, RFC 8292), made once and kept in the config table.

const HEX64 = /^[a-f0-9]{64}$/;
// Push services a subscription may point at (anything else is refused, so the server can't be made to call out
// to arbitrary places)
const SERVICES = [
  /(^|\.)push\.apple\.com$/,
  /^fcm\.googleapis\.com$/,
  /^android\.googleapis\.com$/,
  /(^|\.)push\.services\.mozilla\.com$/,
  /(^|\.)notify\.windows\.com$/,
];
// Phones per person (the oldest goes when there'd be more)
const MAX_PHONES = 10;
// How long a push service keeps a notification for a phone that's off (seconds)
const TTL = 6 * 3600;

const enc = new TextEncoder();
const json = (body, status = 200) => new Response(JSON.stringify(body), {
  status,
  headers: { 'content-type': 'application/json', 'cache-control': 'no-store' },
});
const text = (v, max) => String(v ?? '').replace(/\u0000/g, '').trim().slice(0, max);

// ---------- bytes ----------

export function b64url(bytes) {
  let s = '';
  for (const b of new Uint8Array(bytes)) s += String.fromCharCode(b);
  return btoa(s).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}
export function unb64url(s) {
  const t = String(s).replace(/-/g, '+').replace(/_/g, '/');
  const bin = atob(t + '='.repeat((4 - (t.length % 4)) % 4));
  return Uint8Array.from(bin, (c) => c.charCodeAt(0));
}
function concat(...parts) {
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let i = 0;
  for (const p of parts) { out.set(p, i); i += p.length; }
  return out;
}
async function sha256hex(s) {
  const d = new Uint8Array(await crypto.subtle.digest('SHA-256', enc.encode(s)));
  return [...d].map((b) => b.toString(16).padStart(2, '0')).join('');
}
async function hkdf(salt, ikm, info, bytes) {
  const key = await crypto.subtle.importKey('raw', ikm, 'HKDF', false, ['deriveBits']);
  return new Uint8Array(await crypto.subtle.deriveBits({ name: 'HKDF', hash: 'SHA-256', salt, info }, key, bytes * 8));
}

// ---------- the server's key (VAPID) ----------

let cached = null;
async function vapid(env) {
  if (cached) return cached;
  let row = await env.DB.prepare("SELECT value FROM config WHERE key = 'vapid'").first();
  if (!row) {
    // the first time: made here, kept (if two requests race, the first one kept wins)
    const pair = await crypto.subtle.generateKey({ name: 'ECDSA', namedCurve: 'P-256' }, true, ['sign', 'verify']);
    const value = JSON.stringify({
      publicKey: b64url(await crypto.subtle.exportKey('raw', pair.publicKey)),
      privateJwk: await crypto.subtle.exportKey('jwk', pair.privateKey),
    });
    await env.DB.prepare("INSERT INTO config (key, value) VALUES ('vapid', ?) ON CONFLICT(key) DO NOTHING").bind(value).run();
    row = await env.DB.prepare("SELECT value FROM config WHERE key = 'vapid'").first();
  }
  const v = JSON.parse(row.value);
  const privateKey = await crypto.subtle.importKey('jwk', v.privateJwk, { name: 'ECDSA', namedCurve: 'P-256' }, false, ['sign']);
  cached = { publicKey: v.publicKey, privateKey };
  return cached;
}

/** The `Authorization` header for a push service (a JWT signed with the server's key, good for 12 hours) */
export async function vapidHeader(keys, endpoint, subject) {
  const aud = new URL(endpoint).origin;
  const head = b64url(enc.encode(JSON.stringify({ typ: 'JWT', alg: 'ES256' })));
  const claims = b64url(enc.encode(JSON.stringify({ aud, exp: Math.floor(Date.now() / 1000) + 12 * 3600, sub: subject })));
  const unsigned = `${head}.${claims}`;
  // (WebCrypto signs ECDSA as r || s, which is what a JWT wants)
  const sig = await crypto.subtle.sign({ name: 'ECDSA', hash: 'SHA-256' }, keys.privateKey, enc.encode(unsigned));
  return `vapid t=${unsigned}.${b64url(sig)}, k=${keys.publicKey}`;
}

// ---------- encrypting for a phone (RFC 8291, aes128gcm) ----------

/** The body to send: `payload` encrypted with the phone's public key and its auth secret */
export async function encryptFor(p256dh, auth, payload) {
  const uaPublic = unb64url(p256dh);
  const authSecret = unb64url(auth);
  const ua = await crypto.subtle.importKey('raw', uaPublic, { name: 'ECDH', namedCurve: 'P-256' }, false, []);
  const mine = await crypto.subtle.generateKey({ name: 'ECDH', namedCurve: 'P-256' }, true, ['deriveBits']);
  const asPublic = new Uint8Array(await crypto.subtle.exportKey('raw', mine.publicKey));
  const shared = new Uint8Array(await crypto.subtle.deriveBits({ name: 'ECDH', public: ua }, mine.privateKey, 256));
  const ikm = await hkdf(authSecret, shared, concat(enc.encode('WebPush: info\0'), uaPublic, asPublic), 32);
  const salt = crypto.getRandomValues(new Uint8Array(16));
  const cek = await hkdf(salt, ikm, enc.encode('Content-Encoding: aes128gcm\0'), 16);
  const nonce = await hkdf(salt, ikm, enc.encode('Content-Encoding: nonce\0'), 12);
  // one record: the payload, then the last-record delimiter
  const plain = concat(enc.encode(payload), new Uint8Array([2]));
  const key = await crypto.subtle.importKey('raw', cek, 'AES-GCM', false, ['encrypt']);
  const sealed = new Uint8Array(await crypto.subtle.encrypt({ name: 'AES-GCM', iv: nonce }, key, plain));
  const rs = new Uint8Array([0, 0, 16, 0]); // record size 4096
  return concat(salt, rs, new Uint8Array([asPublic.length]), asPublic, sealed);
}

// ---------- the routes ----------

function okService(endpoint) {
  try {
    const u = new URL(endpoint);
    return u.protocol === 'https:' && SERVICES.some((r) => r.test(u.hostname));
  } catch {
    return false;
  }
}

async function phonesOf(env, pid, tokenHash) {
  const { results } = await env.DB.prepare(
    'SELECT endpoint, p256dh, auth, name, created_at, seen_at FROM push_subs WHERE pid = ? AND token = ? ORDER BY created_at',
  ).bind(pid, tokenHash).all();
  return results || [];
}

export async function push(request, env, url) {
  const path = url.pathname.replace(/\/+$/, '');

  if (path === '/v1/push/key' && request.method === 'GET') {
    return json({ key: (await vapid(env)).publicKey });
  }
  if (request.method !== 'POST') return json({ error: 'method not allowed' }, 405);
  if (Number(request.headers.get('content-length') || 0) > 16 * 1024) return json({ error: 'too large' }, 413);
  let b;
  try { b = await request.json(); } catch { return json({ error: 'bad json' }, 400); }
  const pid = String(b?.pid || '');
  const token = String(b?.token || '');
  if (!HEX64.test(pid) || !HEX64.test(token)) return json({ error: 'bad request' }, 400);
  const tokenHash = await sha256hex(`push-token|${token}`);
  const now = Date.now();

  if (path === '/v1/push/subscribe') {
    const sub = b?.subscription || {};
    const endpoint = String(sub.endpoint || '');
    const p256dh = String(sub.keys?.p256dh || '');
    const auth = String(sub.keys?.auth || '');
    if (!okService(endpoint) || endpoint.length > 1024 || !/^[A-Za-z0-9_-]{80,100}$/.test(p256dh) || !/^[A-Za-z0-9_-]{16,32}$/.test(auth)) {
      return json({ error: 'bad subscription' }, 400);
    }
    await env.DB.prepare(`INSERT INTO push_subs (endpoint, pid, token, p256dh, auth, name, created_at, seen_at)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?)
      ON CONFLICT(endpoint) DO UPDATE SET pid = excluded.pid, token = excluded.token, p256dh = excluded.p256dh,
        auth = excluded.auth, name = excluded.name, seen_at = excluded.seen_at`)
      .bind(endpoint, pid, tokenHash, p256dh, auth, text(b?.name, 40), now, now).run();
    // too many phones: the oldest go
    await env.DB.prepare(`DELETE FROM push_subs WHERE pid = ? AND endpoint NOT IN
      (SELECT endpoint FROM push_subs WHERE pid = ? ORDER BY seen_at DESC LIMIT ?)`).bind(pid, pid, MAX_PHONES).run();
    return json({ ok: true });
  }

  if (path === '/v1/push/status') {
    const phones = await phonesOf(env, pid, tokenHash);
    return json({ phones: phones.map((p) => ({ name: p.name, since: p.created_at, seen: p.seen_at })) });
  }

  if (path === '/v1/push/unpair') {
    const endpoint = b?.endpoint ? String(b.endpoint) : null;
    const r = endpoint
      ? await env.DB.prepare('DELETE FROM push_subs WHERE pid = ? AND token = ? AND endpoint = ?').bind(pid, tokenHash, endpoint).run()
      : await env.DB.prepare('DELETE FROM push_subs WHERE pid = ? AND token = ?').bind(pid, tokenHash).run();
    return json({ removed: r.meta.changes || 0 });
  }

  if (path === '/v1/push/send') {
    const phones = await phonesOf(env, pid, tokenHash);
    if (!phones.length) return json({ sent: 0, phones: 0 });
    const payload = JSON.stringify({
      title: text(b?.title, 80) || 'PTS Workbench',
      body: text(b?.body, 240),
      tag: text(b?.tag, 64),
      url: '/phone/',
    });
    const keys = await vapid(env);
    const subject = `${url.origin}/phone/`;
    let sent = 0;
    await Promise.all(phones.map(async (p) => {
      try {
        const res = await fetch(p.endpoint, {
          method: 'POST',
          headers: {
            authorization: await vapidHeader(keys, p.endpoint, subject),
            'content-encoding': 'aes128gcm',
            'content-type': 'application/octet-stream',
            ttl: String(TTL),
            urgency: 'high',
          },
          body: await encryptFor(p.p256dh, p.auth, payload),
        });
        if (res.ok) {
          sent++;
        } else if (res.status === 404 || res.status === 410) {
          // the phone unsubscribed (or the app was removed): forgotten
          await env.DB.prepare('DELETE FROM push_subs WHERE endpoint = ?').bind(p.endpoint).run();
        } else {
          console.log('push refused', res.status, new URL(p.endpoint).hostname, (await res.text()).slice(0, 200));
        }
      } catch (e) {
        console.log('push failed', new URL(p.endpoint).hostname, String(e));
      }
    }));
    return json({ sent, phones: phones.length });
  }

  return json({ error: 'not found' }, 404);
}