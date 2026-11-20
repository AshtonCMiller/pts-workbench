// Pairing: the desktop app's "Pair phone" shows a QR code of this page's address with `#pair=v1.<sync ID>.<key>`
// after it (the part after # never reaches the server). Typing the store ID and employee number works out the same
// thing, the way the desktop app does (crates/sync derive_keys). From those, the push ID and send token (crates/sync
// push_ids) and the RO list's ID (crates/sync Keys::ros): the desktop app works out the same ones.

export const enc = new TextEncoder();
export const hex = (bytes) => [...new Uint8Array(bytes)].map((b) => b.toString(16).padStart(2, '0')).join('');
export const fromHex = (h) => Uint8Array.from(h.match(/../g).map((x) => parseInt(x, 16)));
export const sha256hex = async (s) => hex(await crypto.subtle.digest('SHA-256', enc.encode(s)));

/** "v1.<64 hex>.<64 hex>" (alone, or after #pair=) → { id, key } */
export function readPairCode(text) {
  const m = String(text || '').match(/(?:#pair=|^)(v1\.[a-f0-9]{64}\.[a-f0-9]{64})/);
  if (!m) return null;
  const [, id, key] = m[1].split('.');
  return { id, key };
}

/** The store ID and employee number, worked out as the desktop app does */
export async function derive(storeId, employee) {
  const emp = String(employee).trim().replace(/^0+(?=\d)/, '');
  const st = String(storeId).trim();
  if (!emp || !st) return null;
  const material = `pts-workbench-sync-v1|${st}|${emp}`;
  const id = await sha256hex(`id|${material}`);
  const ikm = await crypto.subtle.importKey('raw', enc.encode(material), 'HKDF', false, ['deriveBits']);
  const key = await crypto.subtle.deriveBits(
    { name: 'HKDF', hash: 'SHA-256', salt: enc.encode('pts-workbench'), info: enc.encode('encrypt') }, ikm, 256);
  return { id, key: hex(key) };
}

/** The push ID and send token for a pairing */
export async function pushIds(pair) {
  return {
    pid: await sha256hex(`pts-push|${pair.id}`),
    token: await sha256hex(`pts-push-token|${pair.key}`),
  };
}

/** The RO list's document ID */
export const rosId = (pair) => sha256hex(`ros|${pair.id}`);
