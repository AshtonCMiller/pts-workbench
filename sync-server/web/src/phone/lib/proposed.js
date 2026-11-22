// What a scanned pay sheet says to change on your flags: confirmed, the hours paid, the day it was booked; and a
// line you'd marked as on your time sheet that this pay period's sheet doesn't have, marked not on it again (it
// wasn't paid on this one: the next sheet it's on marks it, and moves it to the day it was booked).
// Each: { k (the flag's record), set (its fields to change), text, why, on (to be done), check (the photo wasn't
// clear: a question, with `choices`, shown first; `pick`: the one picked, none yet), id }

import { h2, niceDay, dayOfMs } from './hours.js';

export function proposed(r) {
  const out = [];
  // a change it's sure of: `text` says what it does (its switch on: it's done)
  const add = (flag, set, text, why) => out.push({ k: flag.k, set, text, why, on: true, check: false, id: `${flag.k}|${Object.keys(set).join()}` });
  // one to check on the paper: a question, and its answers (`choices`: each { label, set }, the change it makes, or
  // none: leave it); not answered yet (`pick`)
  const ask = (flag, set, text, why, choices) => out.push({ k: flag.k, set, text, why, choices, pick: null, on: null, check: true, id: `${flag.k}|${Object.keys(set).join()}` });
  const name = (f, ln) => `RO ${f.ro} line ${ln || f.dms || '?'}`;
  for (const p of r.paid) {
    if (!p.flag.confirmed) add(p.flag, { confirmed: true }, `Mark ${name(p.flag, p.line.ln)} as on your time sheet`, `The sheet paid ${h2(p.flag.hours)}, what you flagged`);
  }
  for (const d of r.different) {
    if (d.unsure) {
      // (what each reading of the photo said, and what you flagged)
      const read = [...new Set([d.sold, ...d.line.rows.map((x) => x.alt).filter((x) => x !== null && x !== undefined && d.line.rows.length === 1)])]
        .filter((h) => Math.abs(h - d.flag.hours) > 0.001);
      ask(d.flag, { hours: d.sold, confirmed: true }, `${name(d.flag, d.line.ln)}: how many hours does the paper say?`,
        `You flagged ${h2(d.flag.hours)}; the photo wasn’t clear (it read ${read.map(h2).join(' or ')})`,
        [...read.map((h) => ({ label: h2(h), set: { hours: h, confirmed: true } })),
          { label: `${h2(d.flag.hours)}, as flagged`, set: { confirmed: true } }]);
    } else {
      add(d.flag, { hours: d.sold, confirmed: true }, `Change ${name(d.flag, d.line.ln)} from ${h2(d.flag.hours)} to ${h2(d.sold)}`,
        'What the sheet paid (and marked as on your time sheet)');
    }
  }
  // a line you'd marked as on your time sheet that isn't on this one (only when it's known which pay period the
  // sheet is: a flag from another one is never touched). Done for you only when the whole sheet was read (its rows
  // add up to its total); else it may be a row the photo lost, so it's asked.
  if (r.period) {
    for (const m of r.missing) {
      if (!m.flag.confirmed) continue;
      const what = `${h2(m.flag.hours)}, flagged ${niceDay(dayOfMs(m.flag.flaggedAt))}`;
      if (r.complete) {
        add(m.flag, { confirmed: false }, `Mark ${name(m.flag)} as not on your time sheet`, `It isn’t on this sheet (${what})`);
      } else {
        ask(m.flag, { confirmed: false }, `Is ${name(m.flag)} on the paper?`,
          `It wasn’t found in the photos (${what}), but not every row could be read`,
          [{ label: 'Yes, it’s on it', set: null }, { label: 'No: unmark it', set: { confirmed: false } }]);
      }
    }
  }
  for (const p of [...r.paid, ...r.different]) {
    const booked = p.line.booked;
    if (!booked || !p.flag.flaggedAt || dayOfMs(p.flag.flaggedAt) === booked) continue;
    const was = new Date(p.flag.flaggedAt);
    const [y, m, d] = booked.split('-').map(Number);
    const at = new Date(y, m - 1, d, was.getHours(), was.getMinutes()).getTime();
    const from = niceDay(dayOfMs(p.flag.flaggedAt));
    const to = niceDay(booked);
    if (p.line.rows.some((x) => x.bookedSure)) {
      add(p.flag, { flaggedAt: at }, `Move ${name(p.flag, p.line.ln)} from ${from} to ${to}`, 'The day the sheet says it was booked');
    } else {
      ask(p.flag, { flaggedAt: at }, `${name(p.flag, p.line.ln)}: which day was it booked?`,
        `You flagged it ${from}; the photo read ${to}, but the date was hard to read`,
        [{ label: `${to}: move it`, set: { flaggedAt: at } }, { label: `${from}: leave it`, set: null }]);
    }
  }
  // the ones to check on the paper first
  return out.sort((a, b) => Number(b.check) - Number(a.check));
}
