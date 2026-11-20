// Working things out from the hours document's records (see doc.js)

import { live } from './doc.js';

/** An RO's lines that count (approved, a concern, or done; not declined), as the desktop's sidebar lists them */
export function linesOf(records, ro) {
  const l = records[`l/${ro}`];
  if (!live(l)) return null;
  const lines = l.v.lines || {};
  return (l.v.order || Object.keys(lines))
    .map((k) => lines[k] && { ...lines[k], key: k })
    .filter(Boolean)
    .filter((x) => (x.approved || x.primary || x.completed) && !x.declined)
    .map((x) => {
      const over = records[`o/${ro}/${x.key}`];
      const name = records[`n/${ro}/${x.key}`];
      const flag = records[`f/${ro}/${x.key}`];
      return {
        ...x,
        text: (live(name) && name.v) || x.complaint || x.service || '',
        paid: live(over) ? Number(over.v) : Number(x.hours) || 0,
        flag: live(flag) ? flag.v : null,
      };
    });
}

/** The pay period (1st–15th, 16th–end) `offset` periods from the one `t` is in: [start, end) as times */
export function payPeriod(t, offset) {
  const d = new Date(t);
  const idx = d.getFullYear() * 24 + d.getMonth() * 2 + (d.getDate() <= 15 ? 0 : 1) + offset;
  const y = Math.floor(idx / 24);
  const r = idx - y * 24;
  const m = Math.floor(r / 2);
  const second = r % 2 === 1;
  const start = new Date(y, m, second ? 16 : 1).getTime();
  const end = second ? new Date(y, m + 1, 1).getTime() : new Date(y, m, 16).getTime();
  return { start, end };
}

export function shiftsOf(records) {
  return Object.entries(records)
    .filter(([k, r]) => k.startsWith('s/') && live(r))
    .map(([k, r]) => ({ k, start: Number(r.v.start), end: r.v.end == null ? null : Number(r.v.end) }))
    .sort((a, b) => a.start - b.start);
}

/** Time clocked in between two times (shifts from different computers that overlap counted once) */
export function worked(shifts, from, to, now) {
  const spans = shifts
    .map((s) => [Math.max(s.start, from), Math.min(s.end ?? now, to)])
    .filter(([a, b]) => b > a)
    .sort((a, b) => a[0] - b[0]);
  let total = 0;
  let cur = null;
  for (const [a, b] of spans) {
    if (cur && a <= cur[1]) cur[1] = Math.max(cur[1], b);
    else {
      if (cur) total += cur[1] - cur[0];
      cur = [a, b];
    }
  }
  if (cur) total += cur[1] - cur[0];
  return total;
}

// ----- how things are written -----

export const fh = (n) => `${(Math.round((n || 0) * 100) / 100).toFixed(2)}`;
export const h2 = (n) => `${fh(n)} h`;
export const hm = (ms) => {
  const m = Math.round(ms / 60000);
  return `${Math.floor(m / 60)} h ${String(m % 60).padStart(2, '0')} m`;
};
export const clockTime = (ms) => new Date(ms).toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' });
export const shortDay = (t) => new Date(t).toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
export function niceDay(iso) {
  if (!iso) return '';
  const [y, m, d] = iso.split('-').map(Number);
  return shortDay(new Date(y, m - 1, d));
}
export const dayOfMs = (ms) => {
  const d = new Date(ms);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
};
export function ago(ms, now = Date.now()) {
  const m = Math.max(0, Math.round((now - ms) / 60000));
  if (m < 1) return 'just now';
  if (m < 60) return `${m} min ago`;
  if (m < 1440) return `${Math.round(m / 60)} h ago`;
  return shortDay(ms);
}
