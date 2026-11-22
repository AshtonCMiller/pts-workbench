// Predictive Pay: what a pay period's check should come to from its flagged hours (spiffs aren't counted). The
// same math as the desktop app's (crates/hours/src/pay.rs): the two have to agree, figure for figure. Set up on the
// computer (Hours › Predictive Pay); the setup syncs here encrypted, in a document of its own (doc.js readPay).
//
// 2026's figures: IRS Publication 15-T's percentage method (Worksheet 1A) and Missouri's Form 4282.

export const PERIODS = 24; // twice a month
const SOCIAL = 0.062;
const MEDICARE = 0.0145;

/** Rounded to the cent (as Rust's f64::round: halves away from zero) */
const cents = (n) => Math.sign(n) * Math.round(Math.abs(n) * 100) / 100;
const round = (n) => Math.sign(n) * Math.round(Math.abs(n));

// [from, base, rate]
const TABLES = {
  married: [[0, 0, 0], [19300, 0, 0.10], [44100, 2480, 0.12], [120100, 11600, 0.22], [230700, 35932, 0.24], [422850, 82048, 0.32], [531750, 116896, 0.35], [788000, 206583.5, 0.37]],
  married2: [[0, 0, 0], [16100, 0, 0.10], [28500, 1240, 0.12], [66500, 5800, 0.22], [121800, 17966, 0.24], [217875, 41024, 0.32], [272325, 58448, 0.35], [400450, 103291.75, 0.37]],
  single: [[0, 0, 0], [7500, 0, 0.10], [19900, 1240, 0.12], [57900, 5800, 0.22], [113200, 17966, 0.24], [209275, 41024, 0.32], [263725, 58448, 0.35], [648100, 192979.25, 0.37]],
  single2: [[0, 0, 0], [8050, 0, 0.10], [14250, 620, 0.12], [33250, 2900, 0.22], [60900, 8983, 0.24], [108938, 20512, 0.32], [136163, 29224, 0.35], [328350, 96489.63, 0.37]],
  head: [[0, 0, 0], [15550, 0, 0.10], [33250, 1770, 0.12], [83000, 7740, 0.22], [121250, 16155, 0.24], [217300, 39207, 0.32], [271750, 56631, 0.35], [656150, 191171, 0.37]],
  head2: [[0, 0, 0], [12075, 0, 0.10], [20925, 885, 0.12], [45800, 3870, 0.22], [64925, 8077.5, 0.24], [112950, 19603.5, 0.32], [140175, 28315.5, 0.35], [332375, 95585.5, 0.37]],
};
const MISSOURI = [[0, 0, 0], [1348, 0, 0.02], [2696, 27, 0.025], [4044, 61, 0.03], [5392, 101, 0.035], [6740, 148, 0.04], [8088, 202, 0.045], [9436, 263, 0.047]];
const MO_DEDUCTION = { single: 16100, spouseWorks: 16100, separate: 16100, spouseHome: 32200, head: 24150 };

function byTable(rows, annual) {
  let row = rows[0];
  for (const r of rows) if (annual >= r[0]) row = r;
  return row[1] + (annual - row[0]) * row[2];
}

const num = (v) => Number(v) || 0;

export function federal(w4 = {}, wages) {
  const status = w4.status || 'single';
  let annual = wages * PERIODS + num(w4.otherIncome) - num(w4.deductions);
  if (!w4.twoJobs) annual -= status === 'married' ? 12900 : 8600;
  const table = TABLES[(TABLES[status] ? status : 'single') + (w4.twoJobs ? '2' : '')];
  const tentative = byTable(table, Math.max(annual, 0)) / PERIODS;
  return cents(Math.max(tentative - num(w4.dependents) / PERIODS, 0) + Math.max(num(w4.extra), 0));
}

export function missouri(mo = {}, wages) {
  const annual = Math.max(wages * PERIODS - (MO_DEDUCTION[mo.status] || MO_DEDUCTION.single), 0);
  return round(byTable(MISSOURI, annual) / PERIODS) + Math.max(num(mo.extra), 0);
}

export function taxes(s, gross, pretax, retirement) {
  const fica = Math.max(gross - pretax, 0);
  const income = Math.max(fica - (s.roth ? 0 : retirement), 0);
  return { federal: federal(s.w4, income), social: cents(fica * SOCIAL), medicare: cents(fica * MEDICARE), state: missouri(s.mo, income) };
}
const total = (t) => t.federal + t.social + t.medicare + t.state;

function stubGross(st) {
  if (num(st.gross) > 0) return num(st.gross);
  if (num(st.rate) > 0 && num(st.hours) > 0) return cents(num(st.rate) * num(st.hours));
  return null;
}
function stubRate(st) {
  if (num(st.rate) > 0) return num(st.rate);
  if (num(st.gross) > 0 && num(st.hours) > 0) return num(st.gross) / num(st.hours);
  return null;
}
const on = (line, i) => num((line.amounts || [])[i]);

function fit(points) {
  if (!points.length) return [0, 0];
  if (points.length === 1) return [points[0][1], 0];
  const n = points.length;
  const mg = points.reduce((s, p) => s + p[0], 0) / n;
  const ma = points.reduce((s, p) => s + p[1], 0) / n;
  const spread = Math.max(...points.map((p) => Math.abs(p[1] - ma)));
  const gs = points.reduce((s, p) => s + (p[0] - mg) ** 2, 0);
  if (spread < 0.005 || gs < 1) return [cents(ma), 0];
  const parts = points.filter((p) => p[0] > 0).map((p) => p[1] / p[0]);
  if (parts.length === n) {
    const mean = parts.reduce((s, x) => s + x, 0) / n;
    if (points.every((p) => Math.abs(cents(p[0] * mean) - p[1]) <= 0.011)) return [0, mean];
  }
  const part = points.reduce((s, p) => s + (p[0] - mg) * (p[1] - ma), 0) / gs;
  const fixed = ma - part * mg;
  if (part < 0) return [cents(ma), 0];
  if (fixed < -0.005) return [0, parts.reduce((s, x) => s + x, 0) / Math.max(parts.length, 1)];
  return [fixed, part];
}

/** "$42.10 each check", "2% of the check", "$5.00 + 1.5% of the check" */
export function describe(f) {
  const pct = (p) => `${(p * 100).toFixed(2).replace(/0+$/, '').replace(/\.$/, '')}%`;
  if (Math.abs(f.part) < 0.00005) return `${money(f.fixed)} each check`;
  if (Math.abs(f.fixed) < 0.005) return `${pct(f.part)} of the check`;
  return `${money(f.fixed)} + ${pct(f.part)} of the check`;
}

const fitOn = (f, gross) => Math.max(cents(f.fixed + f.part * gross), 0);

/** What the stubs taught it: { rate, lines, adjust, checks, netOff } */
export function learn(s) {
  const all = s.stubs || [];
  const stubs = all.map((st, i) => [i, stubGross(st)]).filter(([, g]) => g !== null);
  const rate = all.map(stubRate).find((r) => r !== null) ?? null;
  const lines = (s.lines || [])
    .filter((l) => String(l.name || '').trim() || (l.amounts || []).some((a) => a !== null && a !== undefined))
    .map((l) => {
      const [fixed, part] = fit(stubs.map(([i, g]) => [g, on(l, i)]));
      return { name: String(l.name || '').trim() || 'Deduction', pretax: !!l.pretax, fixed, part };
    });

  const sums = [[0, 0], [0, 0], [0, 0], [0, 0]];
  const net = [0, 0];
  for (const [i, gross] of stubs) {
    const st = all[i];
    const pretax = (s.lines || []).filter((l) => l.pretax).reduce((a, l) => a + on(l, i), 0);
    const after = (s.lines || []).filter((l) => !l.pretax).reduce((a, l) => a + on(l, i), 0);
    const retirement = cents((gross * num(s.retirement)) / 100);
    const t = taxes(s, gross, pretax, retirement);
    const pairs = [[st.federal, t.federal], [st.social, t.social], [st.medicare, t.medicare], [st.state, t.state]];
    pairs.forEach(([shown, figured], k) => {
      if (shown !== null && shown !== undefined) {
        sums[k][0] += shown - figured;
        sums[k][1]++;
      }
    });
    if (st.net !== null && st.net !== undefined) {
      const tax = (st.federal ?? t.federal) + (st.social ?? t.social) + (st.medicare ?? t.medicare) + (st.state ?? t.state);
      net[0] += st.net - (gross - pretax - after - retirement - tax);
      net[1]++;
    }
  }
  const avg = ([sum, n]) => (n ? cents(sum / n) : 0);
  const names = ['Federal income tax', 'Social Security', 'Medicare', 'Missouri income tax'];
  return {
    rate,
    lines,
    adjust: { federal: avg(sums[0]), social: avg(sums[1]), medicare: avg(sums[2]), state: avg(sums[3]) },
    checks: [0, 1, 2, 3].filter((k) => sums[k][1]).map((k) => ({ name: names[k], off: avg(sums[k]), stubs: sums[k][1] })),
    netOff: net[1] ? avg(net) : null,
  };
}

/** The check for `hours` flagged: { hours, rate, gross, retirement, lines: [{ name, amount, pretax, how }], taxes,
 *  net } (null: no rate yet) */
export function predict(s, l, hours) {
  if (l.rate === null) return null;
  const rate = l.rate;
  const gross = cents(Math.max(hours, 0) * rate);
  const retirement = cents((gross * Math.max(num(s.retirement), 0)) / 100);
  const lines = l.lines.map((f) => ({ name: f.name, amount: fitOn(f, gross), pretax: f.pretax, how: describe(f) }));
  const pretax = lines.filter((x) => x.pretax).reduce((a, x) => a + x.amount, 0);
  const t = taxes(s, gross, pretax, retirement);
  const adj = (v, by) => (gross > 0 ? cents(Math.max(v + by, 0)) : 0);
  const tx = {
    federal: adj(t.federal, l.adjust.federal),
    social: adj(t.social, l.adjust.social),
    medicare: adj(t.medicare, l.adjust.medicare),
    state: adj(t.state, l.adjust.state),
  };
  const out = lines.reduce((a, x) => a + x.amount, 0);
  return { hours, rate, gross, retirement, lines, taxes: tx, net: cents(gross - retirement - out - total(tx)) };
}

/** A tax's check against the stubs, in words */
export function checkNote(c) {
  if (Math.abs(c.off) < 1) return `${c.name}: your W-4 matches your stubs.`;
  const more = c.off > 0;
  return `${c.name}: your stubs show ${money(Math.abs(c.off))} ${more ? 'more' : 'less'} each check than your W-4 works out to, so it's ${more ? 'added to' : 'taken off'} each guess.`;
}

export function money(n) {
  const c = Math.round(Math.abs(n) * 100);
  const s = `$${Math.floor(c / 100).toLocaleString('en-US')}.${String(c % 100).padStart(2, '0')}`;
  return n < 0 && c ? `-${s}` : s;
}
