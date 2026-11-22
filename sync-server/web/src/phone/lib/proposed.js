// What a scanned pay sheet says to change on your flags, one item for each of your lines (never the same line
// twice): whether it's on your time sheet, the hours it really paid (which you can change), and the day it was
// booked. And a line you'd marked as on your time sheet that this pay period's sheet doesn't have, marked not on it
// again (it wasn't paid on this one: the next sheet it's on marks it, and moves it to the day it was booked).
//
// An item the photo was clear on is done unless you switch it off (`on`). One it wasn't (`check`) asks: the hours
// (`paid`, picked from what the photo read or typed in), the day (`moveOn`), or whether it's on the paper at all
// (`onSheet`); what's not answered is left as it is. `setOf` turns an item into the change it makes.

import { h2, niceDay, dayOfMs } from './hours.js';

export function proposed(r) {
  const items = new Map();
  const item = (flag, ln) => {
    if (!items.has(flag.k)) {
      items.set(flag.k, {
        id: flag.k, k: flag.k, title: `RO ${flag.ro} line ${ln || flag.dms || '?'}`, label: flag.label || '',
        flagged: flag.hours, confirmed: !!flag.confirmed, on: true, check: false, why: [],
        paid: null, readings: [], hoursUnsure: false, move: null, moveOn: null, missing: false, onSheet: null,
      });
    }
    return items.get(flag.k);
  };

  for (const p of r.paid) {
    const it = item(p.flag, p.line.ln);
    it.paid = p.flag.hours;
    it.why.push(`The sheet paid ${h2(p.flag.hours)}, what you flagged`);
  }
  for (const d of r.different) {
    const it = item(d.flag, d.line.ln);
    if (d.unsure) {
      // (what each reading of the photo said)
      const read = [...new Set([d.sold, ...d.line.rows.map((x) => x.alt).filter((x) => x !== null && x !== undefined && d.line.rows.length === 1)])];
      it.readings = read.filter((h) => Math.abs(h - d.flag.hours) > 0.001);
      it.hoursUnsure = true;
      it.check = true;
      it.why.push(`You flagged ${h2(d.flag.hours)}; the photo wasn’t clear (it read ${read.map(h2).join(' or ')}): what does the paper say?`);
    } else {
      it.paid = d.sold;
      it.why.push(`You flagged ${h2(d.flag.hours)}; the sheet paid ${h2(d.sold)}`);
    }
  }
  // (only when it's known which pay period the sheet is: a flag from another one is never touched; done for you
  // only when the whole sheet was read, its rows adding up to its total, else asked: it may be a row the photo lost)
  if (r.period) {
    for (const m of r.missing) {
      if (!m.flag.confirmed) continue;
      const it = item(m.flag);
      it.missing = true;
      it.why.push(`Marked as on your time sheet (${h2(m.flag.hours)}, flagged ${niceDay(dayOfMs(m.flag.flaggedAt))}), but ${r.complete ? 'it isn’t on this sheet' : 'it wasn’t found in the photos, and not every row could be read'}`);
      if (r.complete) it.onSheet = false;
      else it.check = true;
    }
  }
  for (const p of [...r.paid, ...r.different]) {
    const booked = p.line.booked;
    if (!booked || !p.flag.flaggedAt || dayOfMs(p.flag.flaggedAt) === booked) continue;
    const was = new Date(p.flag.flaggedAt);
    const [y, m, d] = booked.split('-').map(Number);
    const at = new Date(y, m - 1, d, was.getHours(), was.getMinutes()).getTime();
    const it = item(p.flag, p.line.ln);
    const sure = p.line.rows.some((x) => x.bookedSure);
    it.move = { from: niceDay(dayOfMs(p.flag.flaggedAt)), to: niceDay(booked), at, sure };
    if (!sure) it.check = true;
  }
  // (nothing to change: a line already marked, paid what you flagged, on the right day)
  const out = [...items.values()].filter((it) => it.check || setOf({ ...it, on: true }));
  // the ones to check on the paper first
  return out.sort((a, b) => Number(b.check) - Number(a.check));
}

/** The hours typed or picked, as a number (none: not answered, or not a number) */
export function hoursOf(v) {
  if (v === null || v === undefined || v === '') return null;
  const n = Number(String(v).replace(',', '.'));
  return Number.isFinite(n) && n >= 0 && n < 100 ? Math.round(n * 100) / 100 : null;
}

/** The change an item makes (none: nothing, as it's answered) */
export function setOf(it) {
  if (!it.check && !it.on) return null;
  if (it.missing) return it.onSheet === false ? { confirmed: false } : null;
  const set = {};
  const paid = hoursOf(it.paid);
  if (paid !== null) {
    if (Math.abs(paid - it.flagged) > 0.001) set.hours = paid;
    if (!it.confirmed || set.hours !== undefined) set.confirmed = true;
  }
  if (it.move && (it.move.sure || it.moveOn === true)) set.flaggedAt = it.move.at;
  return Object.keys(set).length ? set : null;
}

/** Still to answer on an item to check: its hours, its day, or whether it's on the paper */
export function unansweredOf(it) {
  if (!it.check) return 0;
  if (it.missing) return it.onSheet === null ? 1 : 0;
  return (it.hoursUnsure && hoursOf(it.paid) === null ? 1 : 0) + (it.move && !it.move.sure && it.moveOn === null ? 1 : 0);
}
