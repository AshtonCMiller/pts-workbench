// Reading CDK's Technician Payroll Report ("Report of Booked Repair Orders") from the text a photo of it gave
// (Tesseract, on the phone), and checking it against the lines you flagged in PTS Workbench.
//
// What's read from each row: the RO number, the booked date (the day it was flagged), the line (LN), its sequence
// number and the sold hours. A row looks like
//
//   820326 10NOV26 11NOV26 S1 95385 A 9 ST ROP 0.00 2.80 4205A14 REAR AXLE ASS
//   RO     BOOKED  CLOSED  DT TECH  LN SEQ (M) HT (SC) PAY ACTUAL SOLD OP-CODE DESCRIPTION
//
// Photos read imperfectly (O for 0, 4 for 1, a letter lost), so: each page is read twice (at two sizes) and the
// two compared; an RO number with a digit lost is filled in from the rows around it (they're in RO order); what
// still doesn't agree is shown as "check the paper", with what was read.

(function (root) {
  const MONTHS = ['JAN', 'FEB', 'MAR', 'APR', 'MAY', 'JUN', 'JUL', 'AUG', 'SEP', 'OCT', 'NOV', 'DEC'];
  // letters a photo makes of digits
  const DIGIT = { O: '0', o: '0', D: '0', Q: '0', U: '0', C: '0', I: '1', l: '1', i: '1', T: '1', '|': '1', '!': '1',
    Z: '2', z: '2', S: '5', s: '5', G: '6', B: '8', A: '4', g: '9', q: '9' };

  const digits = (t) => [...String(t)].map((c) => (/\d/.test(c) ? c : DIGIT[c] || '')).join('');

  /** "04NOV26" (as a photo reads it: "O4NOV26", "TINOV26") → "2026-11-04" (none: not a date) */
  function date(t) {
    const m = String(t).match(/^[^A-Za-z0-9]*([0-9A-Za-z|!]{1,2})([A-Za-z0-9]{3})([0-9A-Za-z]{2})[^A-Za-z0-9]*$/);
    if (!m) return '';
    const mon = m[2].toUpperCase().replace(/0/g, 'O').replace(/1/g, 'I');
    const mi = MONTHS.indexOf(mon);
    if (mi < 0) return '';
    const day = Number(digits(m[1]));
    const yy = digits(m[3]);
    if (!day || day > 31 || yy.length !== 2) return '';
    return `20${yy}-${String(mi + 1).padStart(2, '0')}-${String(day).padStart(2, '0')}`;
  }
  const looksLikeDate = (t) => /^[^A-Za-z0-9]*[0-9A-Za-z|!]{1,2}(JAN|FEB|MAR|APR|MAY|JUN|JUL|AUG|SEP|OCT|NOV|DEC|N0V|0CT)/i.test(t);

  /** The line letter from what a photo made of it ("Cc", "<C°", "¢") ('' when it's not a letter) */
  function lineLetter(t) {
    const s = String(t).replace(/¢/g, 'C').replace(/[^A-Za-z0-9]/g, '');
    if (!s) return '';
    const c = s[0];
    return /[A-Za-z]/.test(c) ? c.toUpperCase() : '';
  }

  const HOURS = /(\d{1,2})[.,](\d{2})(?!\d)/g;

  /** One row of the report from a line of text (none: not a row) */
  function row(text) {
    const line = String(text).replace(/[“”"'`]/g, ' ').trim();
    const toks = line.split(/\s+/);
    // the pay type ("ROP", as "RoP", "rop", "pop", "nop"…), with the hours after it
    const pay = toks.findIndex((t, i) => i > 1 && /^[^A-Za-z]*[A-Za-z][Oo0][Pp][^A-Za-z]*$/.test(t));
    if (pay < 0) return null;
    // the RO: the first thing on the line
    const roRaw = toks[0];
    let ro = digits(roRaw.replace(/[^0-9A-Za-z|!]/g, ''));
    // the dates, the tech number, then the line and its sequence number
    let i = 1;
    const dates = [];
    while (i < pay && looksLikeDate(toks[i])) dates.push(date(toks[i++]));
    if (!dates.length) return null;
    // DT ("S1"), then the tech number
    let tech = -1;
    for (let j = i; j < pay; j++) {
      if (/^\d{4,6}$/.test(digits(toks[j])) && digits(toks[j]) === toks[j].replace(/[^0-9]/g, '') && toks[j].replace(/[^0-9]/g, '').length >= 4) { tech = j; break; }
    }
    let ln = '';
    let seq = '';
    if (tech >= 0 && tech + 1 < pay) {
      ln = lineLetter(toks[tech + 1]);
      const s = (toks[tech + 2] || '').replace(/[^0-9]/g, '');
      if (tech + 2 < pay && /^\d{1,2}$/.test(s)) seq = String(Number(s));
    }
    // the hours: actual, then sold
    const rest = toks.slice(pay + 1).join(' ');
    const nums = [];
    let m;
    HOURS.lastIndex = 0;
    while ((m = HOURS.exec(rest))) nums.push({ v: Number(`${m[1]}.${m[2]}`), at: m.index });
    let sold = null;
    if (nums.length >= 2) {
      sold = nums[1].v;
    } else if (nums.length === 1 && nums[0].at > 0) {
      // (the actual hours unreadable: the one read is the sold, after them)
      sold = nums[0].v;
    }
    return {
      ro,
      roOk: ro.length === 6,
      booked: dates[0] || '',
      ln,
      seq,
      sold,
      raw: line,
    };
  }

  /** The report's own dates, when the header could be read ("Report for 01NOV26 to 15NOV26") */
  function header(text) {
    const m = String(text).match(/Report\s+for\s+(\S+)\s+to\s+(\S+)/i);
    if (!m) return null;
    const from = date(m[1]);
    const to = date(m[2]);
    return from && to ? { from, to } : null;
  }

  /** Every row on a page's text (RO numbers with a digit lost filled in from the rows around them) */
  function parse(text) {
    const rows = String(text).split(/\r?\n/).map(row).filter(Boolean);
    rows.forEach((r, k) => {
      if (r.roOk || !r.ro) return;
      const near = [rows[k - 1], rows[k + 1], rows[k - 2], rows[k + 2]].filter((x) => x && x.roOk);
      const hit = near.find((x) => x.ro.endsWith(r.ro) || x.ro.startsWith(r.ro));
      if (hit) { r.ro = hit.ro; r.roOk = true; r.filled = true; }
    });
    return { rows, period: header(text) };
  }

  const key = (r) => `${r.ro}|${r.ln}|${r.seq}`;

  /** How alike two readings of a row are (0 to 5: RO, booked date, line, sequence, sold hours) */
  function alike(a, b) {
    let n = 0;
    if (a.ro && b.ro) {
      if (a.ro === b.ro) n += 1;
      else if (a.ro.length === b.ro.length && [...a.ro].filter((c, i) => c !== b.ro[i]).length === 1) n += 0.6;
    }
    if (a.booked && a.booked === b.booked) n += 1;
    if (a.ln && a.ln === b.ln) n += 1;
    if (a.seq && a.seq === b.seq) n += 1;
    if (a.sold !== null && a.sold === b.sold) n += 1;
    return n;
  }

  /** The two readings of a page lined up, row by row (they're both in the page's order) */
  function align(a, b) {
    const n = a.length;
    const m = b.length;
    const score = Array.from({ length: n + 1 }, () => new Array(m + 1).fill(0));
    for (let i = n - 1; i >= 0; i--) {
      for (let j = m - 1; j >= 0; j--) {
        const s = alike(a[i], b[j]);
        score[i][j] = Math.max(score[i + 1][j], score[i][j + 1], s >= 2.5 ? s + score[i + 1][j + 1] : 0);
      }
    }
    const pairs = [];
    let i = 0;
    let j = 0;
    while (i < n && j < m) {
      const s = alike(a[i], b[j]);
      if (s >= 2.5 && score[i][j] === s + score[i + 1][j + 1]) { pairs.push([a[i], b[j]]); i++; j++; }
      else if (score[i + 1][j] >= score[i][j + 1]) { pairs.push([a[i], null]); i++; }
      else { pairs.push([null, b[j]]); j++; }
    }
    while (i < n) pairs.push([a[i++], null]);
    while (j < m) pairs.push([null, b[j++]]);
    return pairs;
  }

  /**
   * A page read twice: its rows, each from what both readings said. Where they differ: the RO number seen more
   * often on the page, the date and line that could be read; `sold` is the first reading's hours (or the only
   * one), `alt` the second's when they differ, `sure` when both read the same.
   */
  function combine(a, b) {
    const seen = new Map();
    for (const r of [...a.rows, ...b.rows]) if (r.roOk) seen.set(r.ro, (seen.get(r.ro) || 0) + 1);
    const out = [];
    for (const [x, y] of align(a.rows, b.rows)) {
      if (!x || !y) {
        const r = x || y;
        out.push({ ...r, alt: null, sure: false, reads: 1 });
        continue;
      }
      const r = { ...x, alt: null, sure: false, reads: 2 };
      if (x.ro !== y.ro) {
        const ok = [x, y].filter((z) => z.roOk);
        const best = ok.sort((p, q) => (seen.get(q.ro) || 0) - (seen.get(p.ro) || 0))[0];
        if (best) { r.ro = best.ro; r.roOk = true; }
      }
      if (!r.booked || (y.booked && x.booked !== y.booked && !r.roOk)) r.booked = y.booked || r.booked;
      r.bookedSure = !!x.booked && x.booked === y.booked;
      if (!r.ln) r.ln = y.ln;
      if (!r.seq || (y.seq && x.seq !== y.seq)) r.seq = r.seq && y.seq && x.seq !== y.seq ? r.seq : (r.seq || y.seq);
      if (x.sold === null) r.sold = y.sold;
      else if (y.sold !== null && Math.abs(y.sold - x.sold) > 0.001) r.alt = y.sold;
      else if (y.sold !== null) r.sure = true;
      r.raw2 = y.raw;
      out.push(r);
    }
    // an RO number read only once (or with a digit lost) that's one digit from one the rows around it have
    out.forEach((r, k) => {
      const near = [out[k - 1], out[k + 1]].filter((z) => z && z.roOk && z.ro !== r.ro && (seen.get(z.ro) || 0) >= 2);
      for (const z of near) {
        const oneOff = r.ro.length === z.ro.length && [...r.ro].filter((c, i) => c !== z.ro[i]).length === 1;
        if ((oneOff && (seen.get(r.ro) || 0) <= 1) || (!r.roOk && r.ro && (z.ro.endsWith(r.ro) || z.ro.startsWith(r.ro)))) {
          r.ro = z.ro; r.roOk = true; r.filled = true;
          break;
        }
      }
    });
    return { rows: out, period: a.period || b.period };
  }

  /** Adds a page's rows to those so far (the same row from another photo of the same page counts once) */
  function addPage(sofar, page) {
    const rows = [...sofar.rows];
    for (const r of page.rows) {
      const same = r.roOk && r.ln && r.seq ? rows.findIndex((y) => key(y) === key(r)) : -1;
      if (same < 0) rows.push(r);
      else if (!rows[same].sure && r.sure) rows[same] = r;
    }
    rows.sort((x, y) => (x.ro || '').localeCompare(y.ro || '') || (x.ln || '').localeCompare(y.ln || '') || Number(x.seq || 0) - Number(y.seq || 0));
    return { rows, period: sofar.period || page.period, pages: (sofar.pages || 0) + 1 };
  }

  /** The pay period (1st–15th, or 16th–end): the report's own, else the one most rows were booked in */
  function periodOf(sheet) {
    if (sheet.period) return { ...sheet.period, read: true };
    const count = new Map();
    for (const r of sheet.rows) {
      if (!r.booked) continue;
      const [y, m, d] = r.booked.split('-').map(Number);
      const k = `${y}-${m}-${d <= 15 ? 1 : 2}`;
      count.set(k, (count.get(k) || 0) + 1);
    }
    const best = [...count.entries()].sort((x, y) => y[1] - x[1])[0];
    if (!best) return null;
    const [y, m, half] = best[0].split('-').map(Number);
    const last = new Date(y, m, 0).getDate();
    const mm = String(m).padStart(2, '0');
    return half === 1
      ? { from: `${y}-${mm}-01`, to: `${y}-${mm}-15`, read: false }
      : { from: `${y}-${mm}-16`, to: `${y}-${mm}-${last}`, read: false };
  }

  /** A local date ("2026-11-04") of a time (ms) */
  function dayOf(ms) {
    const d = new Date(ms);
    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
  }

  const near = (a, b) => Math.abs(a - b) < 0.051;

  /** Every way these rows' hours add up (each row its own reading, or its second) */
  function totals(rows) {
    let sums = [0];
    for (const r of rows) {
      const opts = [r.sold || 0];
      if (r.alt !== null && r.alt !== undefined) opts.push(r.alt);
      sums = sums.flatMap((s) => opts.map((o) => Math.round((s + o) * 100) / 100));
      if (sums.length > 64) sums = sums.slice(0, 64);
    }
    return sums;
  }

  /**
   * The sheet checked against your flags (`flags`: { ro, dms, label, hours, flaggedAt }, the period's): each flag
   * paid, paid different hours, or not on the pages scanned; and what's paid on the sheet that you didn't flag.
   */
  function compare(sheet, flags) {
    const period = periodOf(sheet);
    const inPeriod = flags.filter((f) => !period || (dayOf(f.flaggedAt) >= period.from && dayOf(f.flaggedAt) <= period.to));
    const lines = new Map();
    for (const r of sheet.rows) {
      if (!r.roOk || !r.ln) continue;
      const k = `${r.ro}|${r.ln}`;
      if (!lines.has(k)) lines.set(k, { ro: r.ro, ln: r.ln, rows: [], booked: r.booked, used: false });
      const l = lines.get(k);
      l.rows.push(r);
      if (r.booked && (!l.booked || r.booked < l.booked)) l.booked = r.booked;
    }
    const paid = [];
    const different = [];
    const missing = [];
    for (const f of inPeriod) {
      let line = f.dms ? lines.get(`${f.ro}|${f.dms}`) : null;
      // (a line with no DMS letter in the app, or one the photo lost: the RO's line with the same hours)
      if (!line) {
        line = [...lines.values()].find((l) => l.ro === f.ro && !l.used && totals(l.rows).some((t) => near(t, f.hours)) && (!f.dms || !l.ln));
      }
      if (!line) { missing.push({ flag: f }); continue; }
      line.used = true;
      const sums = totals(line.rows);
      const sold = sums[0];
      const unsure = line.rows.some((r) => r.alt !== null && r.alt !== undefined) || line.rows.some((r) => r.sold === null);
      const item = { flag: f, line, sold, unsure };
      if (sums.some((t) => near(t, f.hours))) paid.push({ ...item, sold: f.hours });
      else different.push(item);
    }
    const unflagged = [...lines.values()]
      .filter((l) => !l.used && totals(l.rows)[0] > 0)
      .map((l) => ({ line: l, sold: totals(l.rows)[0], unsure: l.rows.some((r) => r.alt !== null && r.alt !== undefined) }));
    const unread = sheet.rows.filter((r) => !r.roOk || !r.ln || r.sold === null);
    const soldTotal = Math.round(sheet.rows.reduce((s, r) => s + (r.roOk ? r.sold || 0 : 0), 0) * 100) / 100;
    const flaggedTotal = Math.round(inPeriod.reduce((s, f) => s + (f.hours || 0), 0) * 100) / 100;
    return { period, paid, different, missing, unflagged, unread, soldTotal, flaggedTotal, flags: inPeriod.length };
  }

  const api = { parse, row, date, header, combine, addPage, periodOf, compare, totals };
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
  else root.Sheet = api;
})(typeof window !== 'undefined' ? window : globalThis);
