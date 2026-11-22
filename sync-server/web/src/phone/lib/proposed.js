// What a scanned pay sheet says to change on your flags: confirmed, the hours paid, the day it was booked; and a
// line you'd marked as on your time sheet that this pay period's sheet doesn't have, marked not on it again (it
// wasn't paid on this one: the next sheet it's on marks it, and moves it to the day it was booked).
// Each: { k (the flag's record), set (its fields to change), text, why, on (switched on to start with), check (the
// photo wasn't clear: to check on the paper, so it starts off and is shown first), id }

import { h2, niceDay, dayOfMs } from './hours.js';

export function proposed(r) {
  const out = [];
  const add = (flag, set, text, why, on) => out.push({ k: flag.k, set, text, why, on, check: !on, id: `${flag.k}|${Object.keys(set).join()}` });
  for (const p of r.paid) {
    if (!p.flag.confirmed) add(p.flag, { confirmed: true }, `RO ${p.flag.ro} line ${p.line.ln}: paid`, `Marked as on your time sheet (${h2(p.flag.hours)})`, true);
  }
  for (const d of r.different) {
    add(d.flag, { hours: d.sold, confirmed: true }, `RO ${d.flag.ro} line ${d.line.ln}: ${h2(d.flag.hours)} → ${h2(d.sold)}`,
      d.unsure ? 'What the sheet says was paid, but the photo wasn’t clear: check the paper first' : 'What the sheet says was paid', !d.unsure);
  }
  // (only when it's known which pay period the sheet is: a flag from another one is never touched)
  if (r.period) {
    for (const m of r.missing) {
      if (!m.flag.confirmed) continue;
      add(m.flag, { confirmed: false }, `RO ${m.flag.ro} line ${m.flag.dms || '?'}: not on the sheet`,
        `Marked as not on your time sheet (${h2(m.flag.hours)}, flagged ${niceDay(dayOfMs(m.flag.flaggedAt))}). Scan every page first.`, true);
    }
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
  // the ones to check on the paper first
  return out.sort((a, b) => Number(b.check) - Number(a.check));
}
