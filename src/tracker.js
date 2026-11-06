// Shift clock, repair lines and flag hours. Pure logic (no Electron), so it can be tested on its own.
//
// Clock: you clock in and out for the day. If you forget to clock out, the next time the app
// opens on a later day it clocks you out at the moment the app was last closed.
// Flags: you flag a line yourself; the flag records the line, its paid hours and when you flagged it.
// Nothing here is ever sent anywhere; it is only saved on this computer.

const DAY = 24 * 60 * 60 * 1000;

const startOfDay = (t) => { const d = new Date(t); d.setHours(0, 0, 0, 0); return d.getTime(); };
const overlap = (s1, e1, s2, e2) => Math.max(0, Math.min(e1, e2) - Math.max(s1, s2));
const round2 = (n) => Math.round(n * 100) / 100;

// Pay periods: 1st–15th and 16th–end of month. offset -1 = previous period, etc.
function payPeriod(t, offset = 0) {
  const d = new Date(t);
  let idx = d.getFullYear() * 24 + d.getMonth() * 2 + (d.getDate() <= 15 ? 0 : 1) + offset;
  const y = Math.floor(idx / 24);
  idx -= y * 24;
  const m = Math.floor(idx / 2);
  const second = idx % 2 === 1;
  return {
    offset,
    start: new Date(y, m, second ? 16 : 1).getTime(),
    end: second ? new Date(y, m + 1, 1).getTime() : new Date(y, m, 16).getTime(),
  };
}

class Tracker {
  constructor({ load = () => null, save = () => {}, now = () => Date.now(), deferStartupCheck = false } = {}) {
    this.save_ = save;
    this.now = now;
    this.data = { ros: {}, flags: [], shifts: [], lastSeen: null, lastClosed: null, nextFlagId: 1, ...(load() || {}) };
    if (!Array.isArray(this.data.flags)) this.data.flags = [];
    if (!Array.isArray(this.data.shifts)) this.data.shifts = [];
    delete this.data.current; // per-RO job clock from older versions is no longer used
    this.autoClockOut = null;
    if (!deferStartupCheck) this.startupCheck(); // with sync on, it runs after the first sync instead
  }

  save() { this.save_(this.data); }
  heartbeat() { this.data.lastSeen = this.now(); this.save(); }

  // ---------- shift clock ----------
  openShift() {
    const last = this.data.shifts[this.data.shifts.length - 1];
    return last && last.end == null ? last : null;
  }

  clockIn() {
    if (this.openShift()) return;
    this.data.shifts.push({ start: this.now(), end: null });
    this.autoClockOut = null;
    this.save();
  }

  clockOut() {
    const s = this.openShift();
    if (!s) return;
    s.end = this.now();
    this.save();
  }

  // The app is closing. Stay clocked in (you may just be restarting it); remember when.
  appClosed() {
    this.data.lastClosed = this.now();
    this.data.lastSeen = this.data.lastClosed;
    this.save();
  }

  // On startup: a shift still open from an earlier day means you forgot to clock out,
  // so end it when the app was last closed (or last seen running, if it didn't close cleanly).
  startupCheck() {
    const s = this.openShift();
    if (!s) return;
    // lastClosedAll: the latest any of your computers closed the app (filled in by sync)
    const lastOpen = Math.max(this.data.lastClosed || 0, this.data.lastSeen || 0, this.data.lastClosedAll || 0);
    if (!lastOpen || startOfDay(lastOpen) >= startOfDay(this.now())) return; // same day: keep going
    s.end = Math.max(s.start, lastOpen);
    s.auto = true;
    this.autoClockOut = { at: s.end };
    this.save();
  }

  workedBetween(from, to) {
    const t = this.now();
    let ms = 0;
    for (const s of this.data.shifts) ms += overlap(s.start, s.end ?? t, from, to);
    return ms;
  }

  // ---------- RO lines ----------
  // A line counts if it is approved, primary (primary lines are pre-approved) or already completed
  // (closed tickets often don't mark completed lines approved), and not declined.
  static counts(l) { return (l.approved || l.primary || l.completed) && !l.declined; }
  static paid(l) { return l.override ?? l.hours ?? 0; }
  // Your own name for a line wins over Xtime's description
  static text(l) { return l.name || l.complaint || l.service || ''; }
  static flagLabel(l) { return (l.diag ? 'Diag: ' : '') + Tracker.text(l); }

  // Rename a line (e.g. "Miscellaneous repair or repair not in catalog"). Empty name = back to Xtime's.
  renameLine(ro, key, name) {
    const line = this.data.ros[ro]?.lines[key];
    const clean = String(name ?? '').trim().slice(0, 200) || null;
    if (line) line.name = clean && clean !== line.complaint ? clean : null;
    const f = this.flagFor(ro, key);
    if (f) {
      if (line) f.label = Tracker.flagLabel(line);
      else if (clean) f.label = (f.label.startsWith('Diag: ') ? 'Diag: ' : '') + clean;
    }
    this.save();
  }

  ro(ro) {
    ro = String(ro);
    if (!this.data.ros[ro]) this.data.ros[ro] = { lines: {}, order: [], ymm: '' };
    return this.data.ros[ro];
  }

  updateLines(ro, lines, ymm) {
    const rec = this.ro(ro);
    rec.touched = this.now();
    if (ymm) rec.ymm = ymm;
    const manualKeys = rec.order.filter((k) => rec.lines[k]?.manual);
    rec.order = [...lines.map((l) => l.key), ...manualKeys];
    for (const l of lines) {
      const prev = rec.lines[l.key] || {};
      rec.lines[l.key] = { ...l, override: prev.override ?? null, name: prev.name ?? null };
    }
    for (const k of Object.keys(rec.lines)) if (!rec.order.includes(k)) delete rec.lines[k];
    this.save();
  }

  // A line you type in yourself (e.g. for an RO that has already closed).
  addManualLine(ro, { dms = '', complaint = '', hours = 0 }, ymm = '') {
    const rec = this.ro(ro);
    rec.touched = this.now();
    if (ymm && !rec.ymm) rec.ymm = ymm;
    let n = 1;
    while (rec.lines[`M${n}`]) n++;
    const key = `M${n}`;
    rec.lines[key] = {
      key, dms: String(dms).trim().toUpperCase().slice(0, 4), complaint: String(complaint).trim(), service: '',
      hours: Math.max(0, Number(hours) || 0), override: null,
      approved: true, primary: false, declined: false, completed: true, manual: true,
    };
    rec.order.push(key);
    this.save();
    return key;
  }

  setOverride(ro, key, hours) {
    const line = this.data.ros[ro]?.lines[key];
    if (!line) return;
    const n = Number(hours);
    line.override = hours === null || hours === '' || !Number.isFinite(n) || n < 0 ? null : round2(n);
    // Keep an unconfirmed flag in step with the line's hours
    const f = this.flagFor(ro, key);
    if (f && !f.confirmed) f.hours = Tracker.paid(line);
    this.save();
  }

  // ---------- flags ----------
  flagFor(ro, key) { return this.data.flags.find((f) => f.ro === String(ro) && f.key === key) || null; }

  flagLine(ro, key, when = null) {
    ro = String(ro);
    const line = this.data.ros[ro]?.lines[key];
    if (!line || this.flagFor(ro, key)) return;
    this.data.flags.push({
      id: this.data.nextFlagId++,
      ro, key,
      dms: line.dms || '',
      label: Tracker.flagLabel(line),
      ymm: this.data.ros[ro].ymm || '',
      hours: Tracker.paid(line),
      flaggedAt: when ?? this.now(),
      confirmed: false,
    });
    this.save();
  }

  unflagLine(ro, key) {
    this.data.flags = this.data.flags.filter((f) => !(f.ro === String(ro) && f.key === key));
    this.save();
  }

  updateFlag(id, changes) {
    const f = this.data.flags.find((x) => x.id === id);
    if (!f) return;
    if ('confirmed' in changes) f.confirmed = !!changes.confirmed;
    if ('hours' in changes) {
      const n = Number(changes.hours);
      if (Number.isFinite(n) && n >= 0) {
        f.hours = round2(n);
        const line = this.data.ros[f.ro]?.lines[f.key];
        if (line) line.override = f.hours === line.hours ? null : f.hours;
      }
    }
    if ('flaggedAt' in changes && Number.isFinite(Number(changes.flaggedAt))) f.flaggedAt = Number(changes.flaggedAt);
    if ('label' in changes) { this.renameLine(f.ro, f.key, changes.label); return; }
    this.save();
  }

  deleteFlag(id) {
    this.data.flags = this.data.flags.filter((f) => f.id !== id);
    this.save();
  }

  // ---------- RO status: active → review → completed ----------
  // After each good Xtime refresh: ROs in Pending Review (mode 7), or that were on this tech's list
  // and have dropped off it (closed), go to Review. An RO sent back to work leaves Review again.
  syncStatus(techId, mine) {
    const t = this.now();
    const tech = String(techId);
    const onList = new Set(mine.map((r) => String(r.ro)));
    for (const r of mine) {
      const rec = this.ro(r.ro);
      rec.onList = tech;
      rec.touched = t;
      if (r.vin) rec.vin = String(r.vin).toUpperCase();
      const ymm = [r.year, r.make, r.model].filter(Boolean).join(' ');
      if (ymm) rec.ymm = ymm;
      if (r.tag) rec.tag = String(r.tag);
      if (r.mode === 7) {
        if (!rec.review || rec.review.reason !== 'Pending review') rec.review = { reason: 'Pending review', since: t };
      } else if (rec.review) {
        rec.review = null; // sent back to work
      }
    }
    for (const [ro, rec] of Object.entries(this.data.ros)) {
      if (rec.onList === tech && !onList.has(ro)) {
        rec.onList = null;
        rec.review = { reason: 'Closed', since: t };
      }
    }
    this.save();
  }

  markCompleted(ro) {
    const rec = this.ro(ro);
    rec.completedAt = this.now();
    this.save();
  }

  moveBack(ro) {
    const rec = this.data.ros[ro];
    if (!rec) return;
    rec.completedAt = null;
    rec.review = null;
    rec.touched = this.now();
    this.save();
  }

  // 'completed' | 'review' | 'active'
  roStatus(ro) {
    const rec = this.data.ros[ro];
    if (rec?.completedAt) return 'completed';
    if (rec?.review) return 'review';
    return 'active';
  }

  // Lists for the Review and Completed tabs (completed: the last `days` days)
  statusLists(days = 60) {
    const cutoff = this.now() - days * DAY;
    const review = [];
    const completed = [];
    for (const [ro, rec] of Object.entries(this.data.ros)) {
      const base = { ro, ymm: rec.ymm || '', vin: rec.vin || '', tag: rec.tag || '' };
      if (rec.completedAt) {
        if (rec.completedAt >= cutoff) completed.push({ ...base, completedAt: rec.completedAt });
      } else if (rec.review) {
        review.push({ ...base, reason: rec.review.reason, since: rec.review.since });
      }
    }
    review.sort((a, b) => a.since - b.since);
    completed.sort((a, b) => b.completedAt - a.completedAt);
    return { review, completed };
  }

  // ---------- reporting ----------
  roSummary(ro) {
    const rec = this.data.ros[ro];
    if (!rec) return null;
    const lines = rec.order.map((k) => rec.lines[k]).filter((l) => l && Tracker.counts(l)).map((l) => {
      const flag = this.flagFor(ro, l.key);
      return {
        key: l.key, dms: l.dms, complaint: Tracker.text(l), service: l.service, manual: !!l.manual, diag: !!l.diag,
        renamed: !!l.name, original: l.name ? (l.complaint || l.service || '') : '',
        hours: l.hours, override: l.override, paid: flag ? flag.hours : Tracker.paid(l), primary: !!l.primary,
        status: flag ? 'flagged' : l.completed ? 'done' : 'todo',
        flag: flag ? { id: flag.id, flaggedAt: flag.flaggedAt, confirmed: flag.confirmed } : null,
      };
    });
    const sum = (f) => round2(lines.filter(f).reduce((a, l) => a + l.paid, 0));
    return {
      ymm: rec.ymm,
      vin: rec.vin || '',
      status: rec.completedAt ? 'completed' : rec.review ? 'review' : 'active',
      reason: rec.review?.reason || '',
      completedAt: rec.completedAt || null,
      lines,
      total: sum(() => true),
      flagged: sum((l) => l.status === 'flagged'),
      unflagged: sum((l) => l.status !== 'flagged'),
      flaggedCount: lines.filter((l) => l.status === 'flagged').length,
    };
  }

  // Every line of every RO whose number contains `query`, across all pay periods.
  searchRO(query, limit = 25) {
    const q = String(query || '').trim();
    if (!q) return [];
    const flagsByRO = new Map();
    for (const f of this.data.flags) {
      if (!flagsByRO.has(f.ro)) flagsByRO.set(f.ro, []);
      flagsByRO.get(f.ro).push(f);
    }
    const ros = [...new Set([...Object.keys(this.data.ros), ...flagsByRO.keys()])].filter((ro) => ro.includes(q));
    // Exact match first, then most recently flagged
    const lastFlag = (ro) => Math.max(0, ...(flagsByRO.get(ro) || []).map((f) => f.flaggedAt));
    ros.sort((a, b) => (b === q) - (a === q) || lastFlag(b) - lastFlag(a) || a.localeCompare(b));

    return ros.slice(0, limit).map((ro) => {
      const rec = this.data.ros[ro];
      const flags = flagsByRO.get(ro) || [];
      const rows = [];
      const seen = new Set();
      const fromFlag = (f) => {
        const p = payPeriod(f.flaggedAt);
        return { flagId: f.id, flaggedAt: f.flaggedAt, confirmed: f.confirmed, periodStart: p.start, periodEnd: p.end };
      };
      if (rec) {
        for (const k of rec.order) {
          const l = rec.lines[k];
          if (!l || !Tracker.counts(l)) continue;
          const f = flags.find((x) => x.key === k);
          seen.add(k);
          rows.push({
            key: k, dms: l.dms || '', label: Tracker.flagLabel(l), original: l.name ? (l.complaint || l.service || '') : '',
            hours: f ? f.hours : Tracker.paid(l), ...(f ? fromFlag(f) : { flagId: null }),
          });
        }
      }
      for (const f of flags) {
        if (seen.has(f.key)) continue; // a flagged line Xtime no longer lists
        rows.push({ key: f.key, dms: f.dms, label: f.label, hours: f.hours, ...fromFlag(f) });
      }
      const round = (n) => round2(n);
      return {
        ro,
        ymm: rec?.ymm || flags.find((f) => f.ymm)?.ymm || '',
        canFlag: !!rec,
        lines: rows,
        flagged: round(rows.filter((r) => r.flagId).reduce((a, r) => a + r.hours, 0)),
        total: round(rows.reduce((a, r) => a + r.hours, 0)),
      };
    });
  }

  period(offset = 0) {
    const t = this.now();
    const p = payPeriod(t, offset);
    const flags = this.data.flags
      .filter((f) => f.flaggedAt >= p.start && f.flaggedAt < p.end)
      .sort((a, b) => a.flaggedAt - b.flaggedAt || a.ro.localeCompare(b.ro) || String(a.dms).localeCompare(String(b.dms)));
    const total = round2(flags.reduce((a, f) => a + f.hours, 0));
    const confirmed = round2(flags.filter((f) => f.confirmed).reduce((a, f) => a + f.hours, 0));
    return {
      ...p,
      flags: flags.map((f) => ({ ...f })),
      total, confirmed, unconfirmed: round2(total - confirmed),
      worked: this.workedBetween(p.start, Math.min(p.end, t + 1)),
      isCurrent: offset === 0,
    };
  }

  summary() {
    const t = this.now();
    const s = this.openShift();
    return {
      now: t,
      clockedIn: !!s,
      shiftStart: s ? s.start : null,
      todayWorked: this.workedBetween(startOfDay(t), t + 1),
      autoClockOut: this.autoClockOut,
    };
  }

  // Keep the file small: ROs with no flags and no recent use are dropped; flags and shifts kept 13 months.
  prune(keepDays = 60, keep = []) {
    const t = this.now();
    this.data.flags = this.data.flags.filter((f) => f.flaggedAt >= t - 400 * DAY);
    this.data.shifts = this.data.shifts.filter((s) => s.end == null || s.end >= t - 400 * DAY);
    const flaggedROs = new Set(this.data.flags.map((f) => f.ro));
    const cutoff = t - keepDays * DAY;
    for (const [ro, rec] of Object.entries(this.data.ros)) {
      if (keep.includes(ro) || flaggedROs.has(ro)) continue;
      if (rec.review && !rec.completedAt) continue; // still waiting to be reviewed
      if (Math.max(rec.touched || 0, rec.completedAt || 0) < cutoff) delete this.data.ros[ro];
    }
    this.save();
  }
}

module.exports = { Tracker, payPeriod, startOfDay };