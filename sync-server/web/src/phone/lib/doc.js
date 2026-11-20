// The documents your computer syncs (crates/sync: GET / PUT /v1/store/<ID>): "v1." and base64 of IV + tag +
// ciphertext, AES-256-GCM over raw deflate of { format, records }. Each record is { t, d, v } (or a deleted marker
// { t, d, del: 1 }); the newest t wins when computers merge.
//
// The hours document's records: l/RO an RO's lines, o/RO/LINE your hours for a line, n/RO/LINE your name for it,
// r/RO the RO, f/RO/LINE a flag ({ dms, label, ymm, hours, flaggedAt, confirmed }), s/START a shift.
// The RO list is a document of its own that the desktop app writes: { at, ros: [{ ro, tag, ymm, open, mine, tags }] }.

import { enc, fromHex, rosId } from './keys.js';

function b64(bytes) {
  let s = '';
  for (const b of bytes) s += String.fromCharCode(b);
  return btoa(s);
}
const unb64 = (t) => Uint8Array.from(atob(t.replace(/\s/g, '')), (c) => c.charCodeAt(0));

async function squeeze(bytes, kind, how) {
  const stream = new Blob([bytes]).stream().pipeThrough(new (how === 'in' ? DecompressionStream : CompressionStream)(kind));
  return new Uint8Array(await new Response(stream).arrayBuffer());
}

const storeKey = (pair) => crypto.subtle.importKey('raw', fromHex(pair.key), 'AES-GCM', false, ['encrypt', 'decrypt']);

export async function openDoc(pair, data) {
  if (!data.startsWith('v1.')) throw new Error('unknown format');
  const raw = unb64(data.slice(3));
  const iv = raw.slice(0, 12);
  const tag = raw.slice(12, 28);
  const ct = raw.slice(28);
  const sealed = new Uint8Array(ct.length + 16);
  sealed.set(ct);
  sealed.set(tag, ct.length);
  const plain = new Uint8Array(await crypto.subtle.decrypt({ name: 'AES-GCM', iv }, await storeKey(pair), sealed));
  return JSON.parse(new TextDecoder().decode(await squeeze(plain, 'deflate-raw', 'in')));
}

export async function sealDoc(pair, doc) {
  const plain = await squeeze(enc.encode(JSON.stringify(doc)), 'deflate-raw', 'out');
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const sealed = new Uint8Array(await crypto.subtle.encrypt({ name: 'AES-GCM', iv }, await storeKey(pair), plain));
  const out = new Uint8Array(12 + sealed.length);
  // (WebCrypto puts the tag last; the desktop apps want it right after the IV)
  out.set(iv);
  out.set(sealed.slice(-16), 12);
  out.set(sealed.slice(0, -16), 28);
  return `v1.${b64(out)}`;
}

/** The hours document as it is: its version and its records */
export async function readStore(pair) {
  const res = await fetch(`/v1/store/${pair.id}`, { cache: 'no-store' });
  if (!res.ok) throw new Error(`the server said ${res.status}`);
  const got = await res.json();
  if (!got.data) return { version: got.version || 0, doc: { format: 1, records: {} } };
  return { version: got.version, doc: await openDoc(pair, got.data) };
}

/** Saves the hours document over version `base`: { ok, status, version } (409: someone saved first) */
export async function writeStore(pair, base, doc) {
  const res = await fetch(`/v1/store/${pair.id}`, {
    method: 'PUT', headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ base, data: await sealDoc(pair, doc) }),
  });
  return { ok: res.ok, status: res.status, version: res.ok ? (await res.json()).version : null };
}

/** The RO list your computer sent (none yet: null) */
export async function readRos(pair) {
  const res = await fetch(`/v1/store/${await rosId(pair)}`, { cache: 'no-store' });
  if (!res.ok) throw new Error(`the server said ${res.status}`);
  const got = await res.json();
  return got.data ? openDoc(pair, got.data) : null;
}

export const live = (r) => !!r && !r.del && r.v !== undefined && r.v !== null;

/** A record set (newer than what's there, so it wins when your computers merge); null deletes it */
export function put(records, k, v, now) {
  const was = records[k];
  const t = Math.max(now, ((was && was.t) || 0) + 1);
  records[k] = v === null ? { t, d: 'phone', del: 1 } : { t, d: 'phone', v };
}

/** Your flags */
export function flagsOf(doc) {
  const out = [];
  for (const [k, r] of Object.entries(doc.records || {})) {
    if (!k.startsWith('f/') || r.del || !r.v) continue;
    const [, ro, ...rest] = k.split('/');
    out.push({ k, ro, key: rest.join('/'), dms: String(r.v.dms || '').toUpperCase(), label: r.v.label || '',
      hours: Number(r.v.hours) || 0, flaggedAt: Number(r.v.flaggedAt) || 0, confirmed: !!r.v.confirmed });
  }
  return out;
}

/** Changes flags (each change: its record's key and the fields to set), merged with whatever changed meanwhile */
export async function changeFlags(pair, changes) {
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
    const saved = await writeStore(pair, version, doc);
    if (saved.ok) return n;
    if (saved.status !== 409) throw new Error(`the server said ${saved.status}`);
  }
  throw new Error('the server was busy: try again');
}
