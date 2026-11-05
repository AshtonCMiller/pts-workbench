// PTS Workbench — app UI (top bar, RO lists, lines, flags, clock, tab strip, dialogs)
const api = window.workbench;

// Xtime CurrentMode values. 5 is a best guess — fix the label here once confirmed.
const MODES = {
  3: { label: 'In Inspection', color: '#7a5af5' },
  4: { label: 'Pending Approval', color: '#d97706' },
  5: { label: 'Pending Approval (parts?)', color: '#d97706' },
  6: { label: 'In Repair', color: '#15803d' },
  7: { label: 'Pending Review', color: '#0e7490' },
};
const FLAGS = [
  ['waitingParts', 'Waiting on Parts', '#dc2626'],
  ['waitingCustomer', 'Waiting on Customer', '#b45309'],
  ['waitingWarranty', 'Waiting on Warranty', '#7c3aed'],
];
const LINE_STATUS = {
  flagged: { label: 'Flagged', color: '#15803d' },
  done: { label: 'Done in Xtime', color: '#0e7490' },
  todo: { label: 'To do', color: '#1a5fb4' },
};

let state = null;
let editingTab = false;   // don't redraw the tab strip while a tab is being renamed
let editing = false;      // don't redraw lists while hours are being typed
let resizing = false;
const $ = (id) => document.getElementById(id);

function el(tag, props = {}, ...children) {
  const n = document.createElement(tag);
  for (const [k, v] of Object.entries(props)) {
    if (k === 'class') n.className = v;
    else if (k === 'text') n.textContent = v;
    else if (k === 'checked') n.checked = !!v;
    else if (k.startsWith('on')) n.addEventListener(k.slice(2), v);
    else n.setAttribute(k, v);
  }
  for (const c of children) if (c != null && c !== false && c !== '') n.append(c);
  return n;
}
const kids = (...xs) => xs.filter((x) => x != null && x !== false && x !== '');

const ymmOf = (r) => [r.year, r.make, r.model].filter(Boolean).join(' ');
const fmtH = (h) => `${(Math.round((h || 0) * 10) / 10).toFixed(1)} h`;
const hm = (ms) => {
  const m = Math.max(0, Math.floor(ms / 60000));
  return `${Math.floor(m / 60)}:${String(m % 60).padStart(2, '0')}`;
};
const hms = (ms) => {
  const s = Math.max(0, Math.floor(ms / 1000));
  return `${Math.floor(s / 3600)}:${String(Math.floor(s / 60) % 60).padStart(2, '0')}:${String(s % 60).padStart(2, '0')}`;
};
const todayISO = () => new Date(Date.now() - new Date().getTimezoneOffset() * 60000).toISOString().slice(0, 10);
const shortDate = (t) => new Date(t).toLocaleDateString([], { month: 'short', day: 'numeric' });
function periodLabel(p, withYear = false) {
  const last = new Date(p.end - 1);
  const s = `${shortDate(p.start)}–${last.getDate()}`;
  return withYear ? `${s}, ${last.getFullYear()}` : s;
}
// Clocked time not yet included in the last state push (still clocked in)
const liveExtra = () => (state?.tracking?.clockedIn ? Date.now() - state.tracking.now : 0);
const clockTime = (t) => new Date(t).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });

function badge(text, color) {
  const b = el('span', { class: 'badge', text });
  b.style.color = color;
  return b;
}

function badgesFor(r) {
  const wrap = el('div', { class: 'badges' });
  if (r.mode != null) {
    const m = MODES[r.mode] || { label: `Mode ${r.mode}`, color: '#6b7280' };
    wrap.append(badge(m.label, m.color));
  }
  for (const [key, label, color] of FLAGS) if (r[key]) wrap.append(badge(label, color));
  return wrap.childElementCount ? wrap : null;
}

function sortROs(list) {
  return [...list].sort((a, b) => ((a.mode ?? 99) - (b.mode ?? 99)) || a.ro.localeCompare(b.ro));
}

// Inline number editor that replaces a button until Enter / Escape / click away
// Local date <-> yyyy-mm-dd for date inputs. A picked date is stored at noon so it can't slip
// into the neighbouring day (or pay period) across time zones / daylight saving.
const isoOf = (t) => { const d = new Date(t); return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`; };
const noonOf = (iso) => { const [y, m, d] = iso.split('-').map(Number); return new Date(y, m - 1, d, 12).getTime(); };

// Inline date editor that replaces a button; saves when a date is picked, Enter, or click away
function editDate(button, ts, onSave) {
  editing = true;
  const input = el('input', { type: 'date', value: isoOf(ts), max: todayISO(), class: 'date-input' });
  let finished = false;
  const done = (save) => {
    if (finished) return;
    finished = true; editing = false;
    if (save && input.value && input.value !== isoOf(ts)) onSave(noonOf(input.value)); else render();
  };
  input.addEventListener('keydown', (ev) => { if (ev.key === 'Enter') done(true); if (ev.key === 'Escape') done(false); });
  input.addEventListener('change', () => { if (input.value) done(true); });
  input.addEventListener('blur', () => done(true));
  input.addEventListener('click', (ev) => ev.stopPropagation());
  button.replaceWith(input);
  input.focus();
  try { input.showPicker(); } catch { /* not supported: user types the date */ }
}

// Inline text editor for renaming a line. Enter/click away saves, Escape cancels; empty = Xtime's name.
function editText(target, startValue, onSave) {
  editing = true;
  const input = el('input', { type: 'text', value: startValue, class: 'name-input', placeholder: "Leave empty to use Xtime's description" });
  let finished = false;
  const done = (save) => {
    if (finished) return;
    finished = true; editing = false;
    if (save && input.value.trim() !== startValue.trim()) onSave(input.value.trim()); else render();
  };
  input.addEventListener('keydown', (ev) => { ev.stopPropagation(); if (ev.key === 'Enter') done(true); if (ev.key === 'Escape') done(false); });
  input.addEventListener('blur', () => done(true));
  input.addEventListener('click', (ev) => ev.stopPropagation());
  target.replaceWith(input);
  input.focus(); input.select();
}
const stripDiag = (label) => String(label || '').replace(/^Diag: /, '');

// Clickable description that renames the line; shows Xtime's original wording underneath when renamed
function renamable(text, original, onRename, cls = 'c-text') {
  const t = el('div', { class: `${cls} renamable`, title: 'Click to rename this line', text: text || '(no description)' });
  t.onclick = (e) => { e.stopPropagation(); editText(t, text, onRename); };
  const wrap = el('div', {}, t);
  if (original) wrap.append(el('div', { class: 'orig', title: "Xtime's description", text: `Xtime: ${original}` }));
  return wrap;
}

function editHours(button, startValue, onSave) {
  editing = true;
  const input = el('input', { type: 'number', step: '0.1', min: '0', value: String(startValue), class: 'hours-input' });
  let finished = false;
  const done = (save) => {
    if (finished) return;
    finished = true; editing = false;
    if (save) onSave(input.value); else render();
  };
  input.addEventListener('keydown', (ev) => { if (ev.key === 'Enter') done(true); if (ev.key === 'Escape') done(false); });
  input.addEventListener('blur', () => done(true));
  input.addEventListener('click', (ev) => ev.stopPropagation());
  button.replaceWith(input);
  input.focus(); input.select();
}

// ---------- lines (approved + primary only) ----------
// "Nov 2 ✎" under a flagged line: click to move the flag to the date it was really flagged
function flagDateBtn(flag) {
  const b = el('button', { class: 'flag-date', title: 'Flagged on this date. Click to change it (e.g. flagged in an earlier pay period).', text: `${shortDate(flag.flaggedAt)} ✎` });
  b.onclick = (e) => { e.stopPropagation(); editDate(b, flag.flaggedAt, (ts) => api.send('update-flag', { id: flag.id, changes: { flaggedAt: ts } })); };
  return b;
}

function lineRow(ro, l, showService = false) {
  const st = LINE_STATUS[l.status];
  const hoursBtn = el('button', {
    class: 'hours' + (l.override != null ? ' over' : ''),
    title: l.override != null
      ? `You set this line to pay ${l.override} h (Xtime says ${l.hours} h). Click to change; clear it to go back.`
      : 'Click to set what this line really pays (e.g. after warranty)',
    text: fmtH(l.paid) + (l.override != null ? '*' : ''),
  });
  hoursBtn.onclick = (e) => {
    e.stopPropagation();
    editHours(hoursBtn, l.paid, (v) => api.send('set-line-hours', { ro, key: l.key, hours: v === '' ? null : v }));
  };
  const flagBtn = l.flag
    ? el('button', {
        class: 'flag on', text: l.flag.confirmed ? '✓ Confirmed' : '✓ Flagged',
        title: `Flagged ${shortDate(l.flag.flaggedAt)}. Click to un-flag.`,
        onclick: (e) => {
          e.stopPropagation();
          if (l.flag.confirmed && !confirm('This line is already confirmed on your time sheet. Un-flag it anyway?')) return;
          api.send('unflag-line', { ro, key: l.key });
        },
      })
    : el('button', {
        class: 'flag', text: 'Flag', title: 'Mark this line done and log its hours for this pay period',
        onclick: (e) => { e.stopPropagation(); api.send('flag-line', { ro, key: l.key }); },
      });
  return el('div', { class: `line ${l.status}` },
    el('span', { class: 'dms', text: l.dms || '·' }),
    el('div', { class: 'concern', title: [l.complaint, l.service].filter(Boolean).join('\n') },
      renamable(l.complaint || l.service, l.original, (name) => api.send('rename-line', { ro, key: l.key, name })),
      showService && l.service ? el('div', { class: 'svc', text: l.service }) : null,
      el('div', { class: 'badges' }, badge(st.label, st.color),
        l.diag ? badge('Diag', '#7a5af5') : l.primary ? badge('Primary', '#6b7280') : null)),
    el('div', { class: 'line-actions' }, hoursBtn, flagBtn, l.flag ? flagDateBtn(l.flag) : null),
  );
}

function linesPanel(ro, info) {
  const wrap = el('div', { class: 'lines' });
  if (!info || !info.lines.length) {
    wrap.append(el('div', { class: 'muted small pad', text: state.lastPoll ? 'No approved lines on this RO yet.' : 'Lines load once Xtime is logged in.' }));
    return wrap;
  }
  // When the diag and repair share a letter and a concern, show Xtime's service name to tell them apart
  const sameConcern = (l) => info.lines.some((o) => o !== l && o.dms === l.dms && o.complaint === l.complaint);
  for (const l of info.lines) wrap.append(lineRow(ro, l, sameConcern(l)));
  wrap.append(el('div', { class: 'lines-foot' },
    el('span', { text: `Job pays ${fmtH(info.total)} · flagged ${fmtH(info.flagged)}` }),
    info.lines.some((l) => l.override != null) ? el('span', { class: 'muted', text: ' · * your hours' }) : null));
  return wrap;
}

function linesSummary(info) {
  if (!info || !info.lines.length) return null;
  return el('div', { class: 'sum' }, `${info.flaggedCount}/${info.lines.length} lines flagged`);
}

// ---------- compact rows (sidebar) ----------
// Two short lines: RO / tag / pays, then vehicle · stage · waiting · flagged count.
function compactRow(r, { ws = null } = {}) {
  const info = state.roInfo?.[r.ro];
  const m = r.mode != null ? (MODES[r.mode] || { label: `Mode ${r.mode}`, color: '#6b7280' }) : null;
  const waiting = FLAGS.filter(([k]) => r[k]).map(([, label, color]) => [label.replace('Waiting on ', 'Wait: '), color]);
  const row = el('div', { class: 'ro compact' },
    el('div', { class: 't' },
      `RO ${r.ro}`, r.tag ? el('span', { class: 'tag', text: `  · Tag ${r.tag}` }) : null,
      info && info.lines.length ? el('span', { class: 'pays', text: fmtH(info.total) }) : null),
    el('div', { class: 'c2' },
      el('span', { class: 'cap', text: ymmOf(r) || ws?.ymm || info?.ymm || '' }),
      m ? badge(m.label, m.color) : null,
      ...waiting.map(([label, color]) => badge(label, color)),
      info && info.lines.length ? el('span', { class: 'muted', text: `${info.flaggedCount}/${info.lines.length} flagged` }) : null),
  );
  return row;
}

// ---------- RO cards ----------
function roCard(r, { inDrawer = false, ws = null } = {}) {
  const info = state.roInfo?.[r.ro];
  const active = ws && state.activeRO === ws.ro && state.mode === 'manual';
  const card = el('div', { class: 'ro' + (active ? ' sel' : '') },
    el('div', { class: 't' },
      `RO ${r.ro}`, r.tag ? el('span', { class: 'tag', text: `  · Tag ${r.tag}` }) : null,
      info && info.lines.length ? el('span', { class: 'pays', title: 'What the approved lines on this job pay', text: `Pays ${fmtH(info.total)}` }) : null),
    badgesFor(r),
    el('div', { class: 'v', text: ymmOf(r) || ws?.ymm || info?.ymm || '' }),
    r.vin ? el('div', { class: 'vin', text: r.vin }) : null,
  );
  const sm = linesSummary(info);
  if (inDrawer) {
    if (sm) card.append(sm);
    card.append(el('div', { class: 'card-actions' },
      el('button', {
        class: 'primary open-btn', text: 'Open in Workbench',
        onclick: () => api.send('open-ro', { ro: r.ro, vin: r.vin, ymm: ymmOf(r) }),
      }),
      el('button', {
        class: 'ghost small', text: 'Hide', title: 'Hide from your lists. To bring it back, open this RO in Xtime and press Unhide.',
        onclick: () => api.send('hide-ro', r.ro),
      })));
  } else if (ws && !active) {
    // Not the vehicle you're on: compact, still draggable and closable
    const row = compactRow(r, { ws });
    row.dataset.ro = ws.ro;
    makeDraggable(row);
    row.addEventListener('click', () => api.send('select-ro', ws.ro));
    row.append(el('button', {
      class: 'x', title: 'Close this vehicle', text: '×',
      onclick: (e) => { e.stopPropagation(); api.send('close-ro', ws.ro); },
    }));
    return row;
  } else if (ws) {
    card.dataset.ro = ws.ro;
    makeDraggable(card);
    card.addEventListener('click', () => api.send('select-ro', ws.ro));
    card.append(el('button', {
      class: 'x', title: 'Close this vehicle', text: '×',
      onclick: (e) => { e.stopPropagation(); api.send('close-ro', ws.ro); },
    }));
    if (active) {
      card.append(linesPanel(r.ro, info));
      card.append(el('div', { class: 'card-actions end' }, el('button', {
        class: 'ghost small', text: 'Mark completed', title: 'Move this RO to Completed (and close its tabs)',
        onclick: (e) => { e.stopPropagation(); markCompleted(r.ro, info); },
      })));
    }
    else if (sm) card.append(sm);
  } else {
    // My ROs (not open yet): compact, click to open
    const row = compactRow(r);
    row.title = 'Click to open this RO';
    row.append(el('button', {
      class: 'hide-btn', text: 'Hide', title: 'Hide from your lists. To bring it back, open this RO in the Xtime tab and press Unhide.',
      onclick: (e) => { e.stopPropagation(); api.send('hide-ro', r.ro); },
    }));
    row.addEventListener('click', () => api.send('open-ro', { ro: r.ro, vin: r.vin, ymm: ymmOf(r) }));
    return row;
  }
  return card;
}

// ---------- drag to reorder open vehicles ----------
// Drag by the ⋮⋮ grip. While dragging, state updates don't redraw the list.
let dragRO = null;
function makeDraggable(card) {
  const grip = el('span', { class: 'grip', title: 'Drag to reorder', text: '⋮⋮' });
  grip.addEventListener('pointerdown', () => { card.draggable = true; });
  grip.addEventListener('click', (e) => e.stopPropagation());
  card.querySelector('.t').prepend(grip);

  card.addEventListener('dragstart', (e) => {
    dragRO = card.dataset.ro;
    editing = true;
    e.dataTransfer.effectAllowed = 'move';
    e.dataTransfer.setData('text/plain', dragRO);
    requestAnimationFrame(() => card.classList.add('dragging'));
  });
  card.addEventListener('dragend', () => {
    card.draggable = false;
    card.classList.remove('dragging');
    document.querySelectorAll('.drop-before, .drop-after').forEach((n) => n.classList.remove('drop-before', 'drop-after'));
    dragRO = null;
    editing = false;
    render();
  });
  card.addEventListener('dragover', (e) => {
    if (!dragRO || card.dataset.ro === dragRO) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = 'move';
    const r = card.getBoundingClientRect();
    const after = e.clientY > r.top + r.height / 2;
    card.classList.toggle('drop-after', after);
    card.classList.toggle('drop-before', !after);
  });
  card.addEventListener('dragleave', () => card.classList.remove('drop-before', 'drop-after'));
  card.addEventListener('drop', (e) => {
    e.preventDefault();
    if (!dragRO || card.dataset.ro === dragRO) return;
    const after = card.classList.contains('drop-after');
    const order = state.workspaces.map((w) => w.ro).filter((ro) => ro !== dragRO);
    let i = order.indexOf(card.dataset.ro);
    if (after) i++;
    order.splice(i, 0, dragRO);
    // Reorder locally right away, then tell the app (which saves it)
    const byRO = new Map(state.workspaces.map((w) => [w.ro, w]));
    state.workspaces = order.map((ro) => byRO.get(ro));
    api.send('reorder-ws', order);
  });
}

// ---------- Active / Review / Completed ----------
let sideView = 'active';
try { sideView = localStorage.getItem('sideView') || 'active'; } catch { /* storage unavailable */ }
const expanded = new Set(); // Review/Completed cards showing their lines

function markCompleted(ro, info) {
  const left = info ? info.lines.filter((l) => l.status !== 'flagged') : [];
  if (left.length && !confirm(`${left.length} line${left.length === 1 ? '' : 's'} on RO ${ro} ${left.length === 1 ? "isn't" : "aren't"} flagged (${fmtH(info.unflagged)}). Mark it completed anyway?`)) return;
  expanded.delete(ro);
  api.send('mark-completed', ro);
}

function flagCheck(info) {
  if (!info || !info.lines.length) return el('div', { class: 'check-line muted', text: 'No lines saved for this RO' });
  const n = info.lines.length;
  const left = info.lines.filter((l) => l.status !== 'flagged').length;
  return left
    ? el('div', { class: 'check-line warn', text: `${left} of ${n} line${n === 1 ? '' : 's'} not flagged · ${fmtH(info.unflagged)}` })
    : el('div', { class: 'check-line ok', text: `All ${n} line${n === 1 ? '' : 's'} flagged ✓ · ${fmtH(info.flagged)}` });
}

function statusCard(item, kind) {
  const info = state.roInfo?.[item.ro];
  const open = expanded.has(item.ro);
  const sel = state.mode === 'manual' && state.activeRO === item.ro;
  const card = el('div', { class: 'ro status-card' + (sel ? ' sel' : '') + (open ? ' open' : '') },
    el('div', { class: 't' },
      el('span', { class: 'chev', text: open ? '▾' : '▸' }),
      `RO ${item.ro}`, item.tag ? el('span', { class: 'tag', text: `  · Tag ${item.tag}` }) : null,
      info && info.lines.length ? el('span', { class: 'pays', text: fmtH(info.total) }) : null),
    el('div', { class: 'c2' },
      el('span', { class: 'cap', text: item.ymm || info?.ymm || '' }),
      kind === 'review'
        ? badge(item.reason === 'Closed' ? 'Closed' : 'Pending Review', item.reason === 'Closed' ? '#6b7280' : '#0e7490')
        : el('span', { class: 'muted', text: `Completed ${shortDate(item.completedAt)}` })),
    flagCheck(info),
  );
  card.addEventListener('click', () => { if (open) expanded.delete(item.ro); else expanded.add(item.ro); render(); });
  if (open) {
    const lp = linesPanel(item.ro, info);
    lp.addEventListener('click', (e) => e.stopPropagation()); // working in the lines doesn't collapse the card
    card.append(lp);
  }
  const actions = el('div', { class: 'card-actions' },
    el('button', { class: 'ghost small', text: 'Open', title: 'Open its tabs (Xtime, PTS…)', onclick: (e) => { e.stopPropagation(); api.send('open-any', item.ro); } }));
  if (kind === 'review') {
    const done = info && info.lines.length && info.lines.every((l) => l.status === 'flagged');
    actions.append(el('button', {
      class: done ? 'primary small-btn' : 'ghost small', text: 'Mark completed',
      onclick: (e) => { e.stopPropagation(); markCompleted(item.ro, info); },
    }));
  } else {
    actions.append(el('button', { class: 'ghost small', text: 'Move back to Active', onclick: (e) => { e.stopPropagation(); api.send('move-back', item.ro); } }));
  }
  card.append(actions);
  return card;
}

function renderStatusView() {
  const lists = state?.statusLists || { review: [], completed: [] };
  $('review-count').textContent = lists.review.length ? String(lists.review.length) : '';
  document.querySelectorAll('#side-tabs button').forEach((b) => b.classList.toggle('on', b.dataset.view === sideView));
  $('sidebar').dataset.view = sideView;
  if (sideView === 'active') return;
  const items = sideView === 'review' ? lists.review : lists.completed;
  $('status-help').textContent = sideView === 'review'
    ? 'ROs in Pending Review or closed. Check every line is flagged, then mark the RO completed.'
    : 'ROs you marked completed in the last 60 days.';
  $('status-list').replaceChildren(...(items.length
    ? items.map((it) => statusCard(it, sideView))
    : [el('div', { class: 'empty-note', text: sideView === 'review' ? 'Nothing to review.' : 'Nothing completed yet.' })]));
}

document.querySelectorAll('#side-tabs button').forEach((b) => {
  b.onclick = () => {
    sideView = b.dataset.view;
    try { localStorage.setItem('sideView', sideView); } catch { /* ignore */ }
    render();
  };
});

// ---------- Xtime not updating ----------
// After at least one good refresh: if the list hasn't updated for 5 minutes (or 3 missed checks),
// show a warning with Reload Xtime / Restart app.
function renderXtimeAlert() {
  const s = state;
  const box = $('xtime-alert');
  if (!s || !s.lastPoll) { box.hidden = true; return; }
  const limit = Math.max(5 * 60 * 1000, 3 * (Number(s.settings?.pollSeconds) || 60) * 1000);
  const age = Date.now() - s.lastPoll;
  if (age < limit) { box.hidden = true; return; }
  const mins = Math.round(age / 60000);
  const since = new Date(s.lastPoll).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });
  const howLong = mins >= 60 ? `${Math.floor(mins / 60)} h ${mins % 60} min` : `${mins} min`;
  $('xtime-alert-text').textContent = `Xtime not updating since ${since}`;
  box.title = `Your RO list hasn't updated for ${howLong}.${s.xtimeStatus ? ` Last message: ${s.xtimeStatus}` : ''}`;
  box.hidden = false;
}
$('btn-alert-reload').onclick = () => api.send('reload-xtime');
$('btn-alert-restart').onclick = () => {
  if (confirm('Restart PTS Workbench? Your windows and open vehicles come back, and the clock keeps running.')) api.send('restart-app');
};

// ---------- employee number & first-launch setup ----------
function employeeText() {
  const st = state?.settings || {};
  if (!st.employeeNumber) return 'Enter your employee number and the technician ID is filled in for you.';
  if (state.employeeStatus) return state.employeeStatus;
  if (st.techId && st.techIdFor === st.employeeNumber) return `Xtime technician ID ${st.techId}`;
  return 'Looked up once you are logged in to Xtime.';
}

let setupShown = false; // ask once per session until it's answered
function maybeShowSetup() {
  if (setupShown || !state?.settings || state.settings.setupDone) return;
  if (document.querySelector('.modal:not([hidden])')) return; // don't stack on another dialog
  setupShown = true;
  const f = $('form-setup');
  f.elements.profileName.value = state.profile?.name && state.profile.name !== 'Main' ? state.profile.name : (state.profile?.xtimeName || '');
  f.elements.employeeNumber.value = state.settings.employeeNumber || '';
  f.elements.storeId.value = state.settings.storeId || '';
  openModal('modal-setup');
  setTimeout(() => (f.elements.profileName.value ? f.elements.employeeNumber : f.elements.profileName).focus(), 50);
}
$('form-setup').onsubmit = (e) => {
  e.preventDefault();
  const d = Object.fromEntries(new FormData(e.target));
  api.send('save-settings', {
    profileName: d.profileName.trim(), employeeNumber: d.employeeNumber.trim(), storeId: d.storeId.trim(), setupDone: true,
  });
  closeModal('modal-setup');
};
$('btn-setup-later').onclick = () => closeModal('modal-setup');

// ---------- version & updates ----------
function updateText(u) {
  if (!u) return '';
  const at = u.checkedAt ? ` (checked ${new Date(u.checkedAt).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' })})` : '';
  switch (u.state) {
    case 'checking': return 'Checking for updates…';
    case 'current': return `Up to date${at}`;
    case 'downloading': return `Downloading v${u.version}… ${u.percent || 0}%`;
    case 'ready': return `v${u.version} is ready to install`;
    case 'error': return `Update check failed: ${u.message}${at}`;
    case 'off': return u.message;
    default: return '';
  }
}
function renderVersion() {
  const a = state?.app;
  if (!a) return;
  $('app-version').textContent = `v${a.version}`;
  $('about-version').textContent = `Version ${a.version}`;
  $('about-update').textContent = updateText(a.update);
  $('btn-check-update').disabled = a.update.state === 'off' || a.update.state === 'checking' || a.update.state === 'downloading';
  const btn = $('btn-update');
  const ready = a.update.state === 'ready';
  btn.hidden = !ready && a.update.state !== 'downloading';
  btn.disabled = !ready;
  btn.textContent = ready ? `Restart to update to v${a.update.version}` : `Downloading v${a.update.version}… ${a.update.percent || 0}%`;
  btn.title = ready ? 'Installs the update and reopens your windows. It will also install the next time you close the app.' : '';
}
$('btn-update').onclick = () => {
  if (confirm(`Restart now to install v${state?.app?.update?.version}? Your windows and open vehicles come back, and the clock keeps running.`)) {
    api.send('install-update');
  }
};
$('btn-check-update').onclick = () => api.send('check-update');

// ---------- top bar ----------
function renderTimer() {
  const t = state?.tracking;
  const box = $('timer');
  if (!t) { box.replaceChildren(); return; }
  const today = t.todayWorked + liveExtra();
  if (t.clockedIn) {
    box.className = 'timer running';
    box.replaceChildren(
      el('span', { class: 'dot on' }),
      el('span', { class: 'ro-n', text: `Clocked in ${clockTime(t.shiftStart)}` }),
      el('span', { class: 'mono', title: 'Clocked time today', text: hms(today) }),
      el('button', { class: 'ghost small', text: 'Clock out', onclick: () => api.send('clock-out') }));
  } else {
    box.className = 'timer stopped';
    const auto = t.autoClockOut
      ? el('span', { class: 'auto-note', title: 'You were still clocked in when the app was last closed, so you were clocked out at that time.',
          text: `Auto-out ${new Date(t.autoClockOut.at).toLocaleDateString([], { weekday: 'short' })} ${clockTime(t.autoClockOut.at)}` })
      : null;
    box.replaceChildren(...kids(
      el('span', { class: 'dot off' }),
      el('span', { text: today ? `Clocked out · ${hm(today)} today` : 'Clocked out' }),
      auto,
      el('button', { class: 'primary small-btn', text: 'Clock in', onclick: () => api.send('clock-in') })));
  }
}

function renderPayChip() {
  const btn = $('btn-hours');
  if (!state?.period) { btn.textContent = ''; return; }
  // The chip always shows the current pay period, whatever the dialog is showing
  const cur = state.period.isCurrent ? state.period : null;
  const total = cur ? cur.total : state.currentPeriodTotal;
  btn.replaceChildren(
    el('span', { class: 'muted', text: 'This pay period ' }),
    el('strong', { text: `${fmtH(total)} flagged` }));
}

// ---------- main render ----------
function render() {
  if (!state) return;
  const s = state;
  const body = document.body;
  body.classList.toggle('xtime', s.mode === 'xtime');
  body.classList.toggle('manual', s.mode === 'manual');
  body.classList.toggle('drawer-closed', !s.drawerOpen);
  if (!resizing) document.documentElement.style.setProperty('--side', `${s.settings.sidebarWidth || 380}px`);

  if (s.profile) { $('brand').textContent = `PTS Workbench · ${s.profile.displayName}`; document.title = `PTS Workbench - ${s.profile.displayName}`; }
  $('mode-xtime').classList.toggle('on', s.mode === 'xtime');
  $('mode-manual').classList.toggle('on', s.mode === 'manual');
  $('ws-count').textContent = s.workspaces.length ? String(s.workspaces.length) : '';
  $('btn-drawer').classList.toggle('on', s.drawerOpen);
  const ago = s.lastPoll ? ` · ${new Date(s.lastPoll).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' })}` : '';
  $('xtime-status').textContent = s.xtimeStatus + ago;
  renderTimer();
  renderPayChip();
  renderVersion();
  renderXtimeAlert();

  const hidden = new Set(s.hidden || []);
  const statusOf = (ro) => s.roInfo?.[ro]?.status || 'active';
  const allMine = sortROs(s.myROs).filter((r) => statusOf(r.ro) === 'active');
  const mine = allMine.filter((r) => !hidden.has(r.ro));
  const activeWorkspaces = s.workspaces.filter((w) => statusOf(w.ro) === 'active');
  const hiddenHere = allMine.length - mine.length;

  // Viewing a hidden RO's page in Xtime: offer to unhide it
  const unhide = $('btn-unhide');
  const canUnhide = s.mode === 'xtime' && s.xtimeViewingRO && hidden.has(s.xtimeViewingRO);
  unhide.hidden = !canUnhide;
  if (canUnhide) {
    unhide.textContent = `Unhide RO ${s.xtimeViewingRO}`;
    unhide.onclick = () => api.send('unhide-ro', s.xtimeViewingRO);
  }
  $('hidden-count').textContent = hidden.size ? `${hidden.size} RO${hidden.size === 1 ? '' : 's'} hidden` : 'No hidden ROs';
  $('btn-unhide-all').disabled = !hidden.size;
  const byRO = new Map(mine.map((r) => [r.ro, r]));

  if (!editing) {
    $('drawer-list').replaceChildren(...(mine.length
      ? mine.map((r) => roCard(r, { inDrawer: true }))
      : [el('div', { class: 'empty-note', text: 'No ROs yet. Log in to Xtime and they will appear here.' })]));

    $('open-list').replaceChildren(...(activeWorkspaces.length
      ? activeWorkspaces.map((ws) => {
          const live = byRO.get(ws.ro);
          const card = roCard(live || { ro: ws.ro, vin: ws.vin, mode: null }, { ws });
          if (!live && s.lastPoll) card.append(el('div', { class: 'gone', text: 'Not on your Xtime list' }));
          return card;
        })
      : [el('div', { class: 'empty-note', text: 'Nothing open yet.' })]));

    $('open-count').textContent = activeWorkspaces.length ? String(activeWorkspaces.length) : '';
    const openSet = new Set(s.workspaces.map((w) => w.ro));
    const notOpen = mine.filter((r) => !openSet.has(r.ro));
    $('mine-count').textContent = notOpen.length ? String(notOpen.length) : '';
    $('mine-count').title = hiddenHere ? `${hiddenHere} hidden` : '';
    $('mine-hidden').textContent = hiddenHere ? `${hiddenHere} hidden` : '';
    $('mine-list').replaceChildren(...(notOpen.length
      ? notOpen.map((r) => roCard(r))
      : [el('div', { class: 'empty-note', text: mine.length ? 'All of your ROs are open.' : 'Waiting for Xtime…' })]));

    renderStatusView();
    if (!$('modal-hours').hidden) renderPeriod();
    if (!$('modal-settings').hidden) { renderProfiles(); renderLogins(); $('employee-status').textContent = employeeText(); }
  }

  const ws = s.workspaces.find((w) => w.ro === s.activeRO);
  body.classList.toggle('no-ws', !ws);
  if (ws && !editingTab) renderTabs(ws);
}

function renderTabs(ws) {
  $('tabs').replaceChildren(...ws.tabs.map((t) => {
    const title = el('span', { text: t.title });
    const tab = el('div', { class: `tab kind-${t.kind}` + (t.id === ws.activeTab ? ' on' : ''), title: 'Double-click to rename' },
      el('span', { class: 'tab-dot' }),
      title,
      el('button', {
        class: 'close', text: '×', title: 'Close tab',
        onclick: (e) => { e.stopPropagation(); api.send('close-tab', { ro: ws.ro, id: t.id }); },
      }),
    );
    tab.addEventListener('click', () => { if (t.id !== ws.activeTab) api.send('select-tab', { ro: ws.ro, id: t.id }); });
    tab.addEventListener('dblclick', () => {
      const input = el('input', { value: t.title });
      let finished = false;
      editingTab = true;
      const done = (save) => {
        if (finished) return;
        finished = true; editingTab = false;
        if (save) api.send('rename-tab', { ro: ws.ro, id: t.id, title: input.value }); else render();
      };
      input.addEventListener('keydown', (e) => { if (e.key === 'Enter') done(true); if (e.key === 'Escape') done(false); });
      input.addEventListener('blur', () => done(true));
      title.replaceWith(input);
      input.focus(); input.select();
    });
    return tab;
  }));
  const active = ws.tabs.find((t) => t.id === ws.activeTab);
  $('tab-status').textContent = active?.status || '';
  $('btn-reenter').style.display = ['pts', 'ota', 'cvd', 'prodemand'].includes(active?.kind) ? '' : 'none';
  $('btn-reenter').textContent = active?.kind === 'prodemand' ? 'Re-enter VIN' : 'Reopen vehicle';
}

// ---------- pay period dialog ----------
function renderPeriod() {
  const p = state?.period;
  if (!p) return;
  renderLookup();
  const searching = !!(state.search && $('flag-search').value.trim());
  $('modal-hours').classList.toggle('searching', searching);
  if (searching) { renderSearch(state.search); return; }
  $('period-title').textContent = `Pay period ${periodLabel(p, true)}`;
  $('period-next').disabled = p.isCurrent;

  const worked = p.worked + (p.isCurrent ? liveExtra() : 0);
  const eff = worked > 0 ? `${Math.round((p.total / (worked / 3600000)) * 100)}%` : '—';
  const stat = (label, value, sub) => el('div', { class: 'stat' },
    el('div', { class: 'stat-l', text: label }), el('div', { class: 'stat-v', text: value }),
    sub ? el('div', { class: 'muted small', text: sub }) : null);
  $('hours-stats').replaceChildren(
    stat('Flagged', fmtH(p.total), `${p.flags.length} line${p.flags.length === 1 ? '' : 's'}`),
    stat('Confirmed', fmtH(p.confirmed), 'on your time sheet'),
    stat('Not confirmed', fmtH(p.unconfirmed), p.unconfirmed ? 'still to check' : 'all checked'),
    stat('Clocked', hm(worked), `efficiency ${eff}`));

  // Grouped by RO: an RO header row, then its lines indented underneath
  const head = el('tr', {}, ...['Line', 'Concern', 'Flagged', 'Hours', 'Confirmed', ''].map((c) => el('th', { text: c })));
  const groups = new Map();
  for (const f of p.flags) {
    if (!groups.has(f.ro)) groups.set(f.ro, []);
    groups.get(f.ro).push(f);
  }
  const lineOrder = (a, b) => String(a.dms).localeCompare(String(b.dms)) || (a.label.startsWith('Diag') ? -1 : 0) - (b.label.startsWith('Diag') ? -1 : 0);
  const rows = [];
  for (const [ro, flags] of groups) {
    flags.sort(lineOrder);
    const sub = flags.reduce((a, f) => a + f.hours, 0);
    const allConfirmed = flags.every((f) => f.confirmed);
    const ymm = flags.find((f) => f.ymm)?.ymm || state.roInfo?.[ro]?.ymm || '';
    rows.push(el('tr', { class: 'ro-head' + (allConfirmed ? ' confirmed' : '') },
      el('td', { colspan: '3' }, el('span', { class: 'strong', text: `RO ${ro}` }), ymm ? el('span', { class: 'muted cap', text: `  · ${ymm}` }) : null),
      el('td', { class: 'strong', text: fmtH(sub) }),
      el('td', { class: 'center' }, el('input', {
        type: 'checkbox', checked: allConfirmed, title: 'Confirm every line on this RO',
        onchange: (e) => flags.forEach((f) => { if (f.confirmed !== e.target.checked) api.send('update-flag', { id: f.id, changes: { confirmed: e.target.checked } }); }),
      })),
      el('td'),
    ));
    for (const f of flags) {
      const hoursBtn = el('button', { class: 'hours', text: fmtH(f.hours), title: 'Click to change' });
      hoursBtn.onclick = () => editHours(hoursBtn, f.hours, (v) => { if (v !== '') api.send('update-flag', { id: f.id, changes: { hours: v } }); else render(); });
      rows.push(el('tr', { class: 'ro-line' + (f.confirmed ? ' confirmed' : '') },
        el('td', { class: 'dmscell' }, el('span', { class: 'arrow', text: '↳' }), f.dms || '—'),
        el('td', { class: 'concern-cell' }, (() => {
          const t = el('span', { class: 'renamable', title: 'Click to rename this line', text: f.label || '(no description)' });
          t.onclick = () => editText(t, stripDiag(f.label), (name) => api.send('update-flag', { id: f.id, changes: { label: name } }));
          return t;
        })()),
        el('td', { class: 'nowrap' }, flagDateBtn(f)),
        el('td', {}, hoursBtn),
        el('td', { class: 'center' }, el('input', {
          type: 'checkbox', checked: f.confirmed, title: 'Tick once it shows on your time sheet',
          onchange: (e) => api.send('update-flag', { id: f.id, changes: { confirmed: e.target.checked } }),
        })),
        el('td', {}, el('button', {
          class: 'x-small', text: '×', title: 'Remove this flag',
          onclick: () => { if (confirm(`Remove the flag for RO ${f.ro} line ${f.dms || ''}?`)) api.send('delete-flag', f.id); },
        })),
      ));
    }
  }
  const foot = p.flags.length
    ? el('tr', { class: 'total' }, el('td', { colspan: '3', text: 'Total' }), el('td', { text: fmtH(p.total) }), el('td', { class: 'center muted', text: `${fmtH(p.confirmed)} confirmed` }), el('td'))
    : el('tr', {}, el('td', { colspan: '6', class: 'muted', text: 'Nothing flagged in this pay period yet.' }));
  $('flag-table').replaceChildren(head, ...rows, foot);

}

// ---------- "Add an RO" ----------
function renderLookup() {
  const lk = state.lookup;
  const box = $('lookup-result');
  const manual = $('form-manual');
  manual.hidden = !lk || lk.status === 'loading';
  if (!manual.hidden && !manual.elements.date.value) manual.elements.date.value = todayISO();
  if (!lk) { box.replaceChildren(); return; }
  const info = state.roInfo?.[lk.ro];
  box.replaceChildren(...kids(
    el('div', { class: 'lookup-head' },
      el('strong', { text: `RO ${lk.ro}` }),
      info?.ymm ? el('span', { class: 'muted cap', text: ` · ${info.ymm}` }) : null,
      el('button', { class: 'ghost small', text: 'Done', onclick: () => api.send('clear-lookup') })),
    lk.message ? el('p', { class: lk.status === 'error' ? 'error' : 'muted small', text: lk.message }) : null,
    info?.lines.length ? linesPanel(lk.ro, info) : null,
    el('p', { class: 'muted small', text: 'Add a line by hand (it is flagged straight away):' }),
  ));
}

// ---------- search every pay period by RO ----------
function renderSearch(search) {
  const n = search.results.length;
  $('period-title').textContent = n ? `RO search: “${search.query}”` : `No ROs matching “${search.query}”`;
  const head = el('tr', {}, ...['Line', 'Concern', 'Hours', 'Flagged', 'Paid in pay period', 'Confirmed'].map((c) => el('th', { text: c })));
  const rows = [];
  for (const r of search.results) {
    rows.push(el('tr', { class: 'ro-head' },
      el('td', { colspan: '2' }, el('span', { class: 'strong', text: `RO ${r.ro}` }), r.ymm ? el('span', { class: 'muted cap', text: `  · ${r.ymm}` }) : null),
      el('td', { class: 'strong', text: fmtH(r.flagged) }),
      el('td', { colspan: '3', class: 'muted small', text: r.flagged === r.total ? 'all lines flagged' : `${fmtH(r.total - r.flagged)} not flagged yet` }),
    ));
    if (!r.lines.length) rows.push(el('tr', {}, el('td', { colspan: '6', class: 'muted small', text: 'No approved lines saved for this RO.' })));
    for (const l of r.lines) {
      const flagged = !!l.flagId;
      rows.push(el('tr', { class: 'ro-line' + (l.confirmed ? ' confirmed' : '') },
        el('td', { class: 'dmscell' }, el('span', { class: 'arrow', text: '↳' }), l.dms || '—'),
        el('td', { class: 'concern-cell' }, (() => {
          const t = el('span', { class: 'renamable', title: 'Click to rename this line', text: l.label || '(no description)' });
          t.onclick = () => editText(t, stripDiag(l.label), (name) => l.flagId
            ? api.send('update-flag', { id: l.flagId, changes: { label: name } })
            : api.send('rename-line', { ro: r.ro, key: l.key, name }));
          const cell = el('div', {}, t);
          if (l.original) cell.append(el('div', { class: 'orig', text: `Xtime: ${l.original}` }));
          return cell;
        })()),
        el('td', { class: 'strong', text: fmtH(l.hours) }),
        el('td', { class: 'nowrap' }, flagged
          ? flagDateBtn({ id: l.flagId, flaggedAt: l.flaggedAt })
          : r.canFlag
            ? el('button', { class: 'flag', text: 'Flag', title: 'Flag this line now', onclick: () => api.send('flag-line', { ro: r.ro, key: l.key }) })
            : el('span', { class: 'muted', text: 'Not flagged' })),
        el('td', { class: 'nowrap', text: flagged ? periodLabel({ start: l.periodStart, end: l.periodEnd }, true) : '—' }),
        el('td', { class: 'center' }, flagged ? el('input', {
          type: 'checkbox', checked: l.confirmed, title: 'Tick once it shows on your time sheet',
          onchange: (e) => api.send('update-flag', { id: l.flagId, changes: { confirmed: e.target.checked } }),
        }) : null),
      ));
    }
  }
  if (!n) rows.push(el('tr', {}, el('td', { colspan: '6', class: 'muted', text: 'Nothing saved for that RO number. You can add it below under “Add an RO”.' })));
  $('flag-table').replaceChildren(head, ...rows);
}

let searchTimer = null;
$('flag-search').addEventListener('input', (e) => {
  clearTimeout(searchTimer);
  const q = e.target.value.trim();
  if (!q) { api.send('search-flags', ''); renderPeriod(); return; }
  searchTimer = setTimeout(() => api.send('search-flags', q), 150);
});

$('period-prev').onclick = () => api.send('set-period', (state?.period?.offset ?? 0) - 1);
$('period-next').onclick = () => api.send('set-period', Math.min(0, (state?.period?.offset ?? 0) + 1));
$('form-lookup').onsubmit = (e) => {
  e.preventDefault();
  const ro = e.target.elements.ro.value.trim();
  if (!ro) return;
  api.send('lookup-ro', ro);
  const f = $('form-manual');
  f.reset();
  f.elements.date.value = todayISO();
};
$('form-manual').onsubmit = (e) => {
  e.preventDefault();
  const lk = state?.lookup;
  if (!lk) return;
  const d = Object.fromEntries(new FormData(e.target));
  api.send('add-manual-line', { ro: lk.ro, dms: d.dms, complaint: d.complaint, hours: d.hours, date: d.date });
  const keepDate = e.target.elements.date.value;
  e.target.reset();
  e.target.elements.date.value = keepDate;
};

// "My ROs" can be collapsed to its header; remembered on this computer
let mineCollapsed = false;
try { mineCollapsed = localStorage.getItem('mineCollapsed') === '1'; } catch { /* storage unavailable */ }
function applyMineCollapsed() {
  $('side-mine').classList.toggle('collapsed', mineCollapsed);
  $('btn-mine-toggle').textContent = mineCollapsed ? 'Show' : 'Hide';
}
$('btn-unhide-all').onclick = () => {
  if (confirm('Show every hidden RO in your lists again?')) api.send('unhide-all');
};
$('btn-mine-toggle').onclick = () => {
  mineCollapsed = !mineCollapsed;
  try { localStorage.setItem('mineCollapsed', mineCollapsed ? '1' : '0'); } catch { /* ignore */ }
  applyMineCollapsed();
};
applyMineCollapsed();

// ---------- top bar buttons ----------
$('mode-xtime').onclick = () => api.send('set-mode', 'xtime');
$('mode-manual').onclick = () => api.send('set-mode', 'manual');
$('btn-refresh').onclick = () => api.send('refresh-ros');
$('btn-drawer').onclick = () => api.send('toggle-drawer');
$('btn-reload-xtime').onclick = () => api.send('reload-xtime');
$('btn-hours').onclick = () => {
  $('flag-search').value = '';
  api.send('search-flags', '');
  api.send('set-period', 0);
  renderPeriod();
  openModal('modal-hours');
  $('flag-search').focus();
};

const activeWs = () => state?.workspaces.find((w) => w.ro === state.activeRO);
$('add-tab').onchange = (e) => {
  const kind = e.target.value;
  e.target.value = '';
  const ws = activeWs();
  if (ws && kind) api.send('add-tab', { ro: ws.ro, kind });
};
$('btn-copy-vin').onclick = () => { const ws = activeWs(); if (ws) api.send('copy-vin', ws.ro); };
$('btn-reenter').onclick = () => { const ws = activeWs(); if (ws) api.send('reenter-vin', { ro: ws.ro, id: ws.activeTab }); };
$('btn-reload-tab').onclick = () => { const ws = activeWs(); if (ws) api.send('reload-tab', { ro: ws.ro, id: ws.activeTab }); };

// ---------- resizable sidebar ----------
$('resizer').addEventListener('pointerdown', (e) => {
  e.preventDefault();
  resizing = true;
  api.send('set-overlay', true); // hide PTS while dragging so the mouse isn't swallowed by it
  $('resizer').setPointerCapture(e.pointerId);
  document.body.classList.add('resizing');
});
$('resizer').addEventListener('pointermove', (e) => {
  if (!resizing) return;
  document.documentElement.style.setProperty('--side', `${Math.min(700, Math.max(260, e.clientX))}px`);
});
$('resizer').addEventListener('pointerup', (e) => {
  if (!resizing) return;
  resizing = false;
  document.body.classList.remove('resizing');
  const w = Math.min(700, Math.max(260, e.clientX));
  if (state) state.settings.sidebarWidth = w;
  api.send('set-sidebar', w);
  api.send('set-overlay', false);
});

// ---------- dialogs ----------
function openModal(id) { $(id).hidden = false; api.send('set-overlay', true); }
function closeModal(id) { $(id).hidden = true; api.send('set-overlay', false); }
document.querySelectorAll('[data-close]').forEach((b) => { b.onclick = () => closeModal(b.closest('.modal').id); });
document.addEventListener('keydown', (e) => {
  if (e.key !== 'Escape' || editing) return;
  document.querySelectorAll('.modal:not([hidden])').forEach((m) => closeModal(m.id));
});

// Windows (profiles): each has its own logins and settings; hours are shared
function renderProfiles() {
  const list = $('profile-list');
  if (!state?.profiles) return;
  $('settings-for').textContent = state.profile ? `· ${state.profile.displayName}` : '';
  list.replaceChildren(...state.profiles.map((p) => el('div', { class: 'profile-row' + (p.current ? ' current' : '') },
    el('span', { class: 'dot ' + (p.open ? 'on' : 'off') }),
    el('span', { class: 'strong', text: p.displayName }),
    p.displayName !== p.name ? el('span', { class: 'muted small', text: ` (${p.name})` }) : null,
    p.current
      ? el('span', { class: 'muted small push', text: 'This window' })
      : el('button', {
          type: 'button', class: 'ghost small push', text: p.open ? 'Show' : 'Open',
          onclick: () => { api.send('open-profile', p.id); closeModal('modal-settings'); },
        }),
  )));
}
$('btn-new-profile').onclick = () => {
  const name = $('new-profile-name').value.trim();
  if (!name) { $('new-profile-name').focus(); return; }
  api.send('new-profile', name);
  $('new-profile-name').value = '';
  closeModal('modal-settings');
};
$('new-profile-name').addEventListener('keydown', (e) => {
  if (e.key === 'Enter') { e.preventDefault(); $('btn-new-profile').click(); } // don't submit the settings form
});

function renderLogins() {
  const list = $('logins-list');
  const logins = state?.logins || [];
  $('logins-note').textContent = state?.canSavePasswords === false
    ? "This computer can't encrypt passwords for this app, so logins aren't saved."
    : 'Saved when you sign in to Xtime, PTS or ProDemand in this window, encrypted with your Windows account. Only this window uses them.';
  list.replaceChildren(...(logins.length ? logins.map((l) => el('div', { class: 'profile-row' },
    el('span', { class: 'strong', text: l.host }),
    l.username ? el('span', { class: 'muted small', text: ` · ${l.username}` }) : null,
    el('button', { type: 'button', class: 'ghost small push', text: 'Remove', onclick: () => api.send('remove-login', l.host) }),
  )) : [el('div', { class: 'muted small', text: 'None yet — sign in once and it will be remembered.' })]));
}

$('btn-settings').onclick = () => {
  const f = $('form-settings');
  for (const [k, v] of Object.entries(state?.settings || {})) {
    const input = f.elements[k];
    if (!input) continue;
    if (input.type === 'checkbox') input.checked = !!v; else input.value = v;
  }
  f.elements.profileName.value = state?.profile?.name || '';
  $('employee-status').textContent = employeeText();
  renderProfiles();
  renderLogins();
  openModal('modal-settings');
};
$('form-settings').onsubmit = (e) => {
  e.preventDefault();
  const out = {};
  for (const input of e.target.elements) {
    if (!input.name) continue;
    out[input.name] = input.type === 'checkbox' ? input.checked : input.value;
  }
  api.send('save-settings', out);
  closeModal('modal-settings');
};

// VIN check digit (North American VINs)
const VAL = { A:1,B:2,C:3,D:4,E:5,F:6,G:7,H:8,J:1,K:2,L:3,M:4,N:5,P:7,R:9,S:2,T:3,U:4,V:5,W:6,X:7,Y:8,Z:9 };
const W = [8,7,6,5,4,3,2,10,0,9,8,7,6,5,4,3,2];
function vinProblem(v) {
  if (!/^[A-HJ-NPR-Z0-9]{17}$/.test(v)) return 'A VIN is 17 letters and numbers (no I, O or Q).';
  let sum = 0;
  for (let i = 0; i < 17; i++) sum += (/\d/.test(v[i]) ? +v[i] : VAL[v[i]]) * W[i];
  return v[8] === (sum % 11 === 10 ? 'X' : String(sum % 11)) ? '' : 'That VIN fails its check digit. Double-check it.';
}

// "Add vehicle": the VIN is optional; left blank, the app gets it from Xtime
let addingRO = null; // RO being looked up in Xtime while the dialog waits
$('btn-add-vehicle').onclick = () => {
  $('form-add').reset(); $('add-error').textContent = ''; $('add-error').className = 'error';
  addingRO = null; openModal('modal-add');
};
$('form-add').onsubmit = (e) => {
  e.preventDefault();
  const d = Object.fromEntries(new FormData(e.target));
  const ro = d.ro.trim();
  const vin = d.vin.trim().toUpperCase();
  if (vin) {
    const problem = vinProblem(vin);
    if (problem) { $('add-error').textContent = problem; return; }
  }
  if (!vin) addingRO = ro; // wait for the Xtime lookup before closing
  api.send('add-vehicle', { ro, vin, ymm: d.ymm.trim() });
  if (vin) closeModal('modal-add');
};
document.querySelector('#form-add [data-close]').addEventListener('click', () => { if (addingRO) api.send('cancel-add'); addingRO = null; });
function renderAddLookup() {
  if (!addingRO || $('modal-add').hidden) return;
  const a = state?.addLookup;
  const box = $('add-error');
  if (a && a.ro === addingRO) {
    box.textContent = a.message || '';
    box.className = a.status === 'error' ? 'error' : 'muted small';
    $('form-add').querySelector('button[type=submit]').disabled = a.status === 'loading';
  } else if (state?.workspaces.some((w) => w.ro === addingRO)) {
    addingRO = null; // found and opened
    $('form-add').querySelector('button[type=submit]').disabled = false;
    closeModal('modal-add');
  }
}

// Live clock: update running times every second without a full redraw
setInterval(() => {
  if (!state) return;
  renderTimer();
  renderXtimeAlert();
}, 1000);

let noticeTimer = null;
api.onNotice?.((n) => {
  const box = $('notice');
  box.replaceChildren(el('span', { text: n.text }), n.removeHost
    ? el('button', { class: 'ghost small', text: 'Remove', title: "Don't keep this login",
        onclick: () => { api.send('remove-login', n.removeHost); box.hidden = true; } })
    : null);
  box.hidden = false;
  clearTimeout(noticeTimer);
  noticeTimer = setTimeout(() => { box.hidden = true; }, n.removeHost ? 15000 : 5000);
});

api.onState((s) => { state = s; render(); renderAddLookup(); maybeShowSetup(); });
api.send('ui-ready');