// Labor Times, the tray (kept per vehicle): picked operations adding up warranty and customer pay time, with warnings
// when they overlap, and the Add buttons that put them in.
'use strict';

// ---------- the tray (kept per vehicle) ----------
function toggleTray(o, opt) {
  const i = saved.tray.findIndex((t) => t.code === opt.code && t.qual === opt.qual);
  if (i >= 0) saved.tray.splice(i, 1);
  else {
    const base = o.parentCode ? lastOps.find((x) => x.code === o.parentCode) : null;
    saved.tray.push({ code: opt.code, qual: opt.qual, time: opt.time, name: o.name, kind: o.kind, base: base?.code || '',
      baseName: base?.name || '', cart: opt.cart, rules: o.rules || [] });
  }
  save();
  renderTray();
}

function trayWarnings() {
  const out = [];
  const codes = saved.tray.map((t) => t.code);
  saved.tray.forEach((t) => {
    for (const r of t.rules || []) {
      if (!matches(r.subject, t.code)) continue;
      for (const other of codes) {
        if (other === t.code) continue;
        if (r.not.some((p) => matches(p, other))) {
          const pair = [t.code, other].sort();
          out.push(`${pair[0]} and ${pair[1]} overlap: Ford says not to claim them together${r.cond ? ` (${r.cond})` : ''}.`);
        }
      }
    }
    if (t.kind === 'combo' && t.base && !saved.tray.some((x) => x.kind === 'base' && x.code.startsWith(t.base))) out.push(`${t.code} is a combination: also claim its base operation ${t.base}${t.baseName ? ` (${t.baseName})` : ''}.`);
  });
  return [...new Set(out)];
}

function renderTray() {
  const tray = saved.tray;
  $('tray-count').textContent = String(tray.length);
  let w = 0, actual = 0;
  for (const t of tray) { const n = num(t.time); if (n == null) actual++; else w += n; }
  const box = $('l-tray');
  if (!tray.length) {
    box.replaceChildren(el('h2', { text: 'Selected operations' }), el('div', { class: 'l-empty', text: 'Press + Add on an option to add it up here.' }));
    return;
  }
  const warns = trayWarnings();
  fill(box,
    el('h2', {}, 'Selected operations', el('span', { class: 'l-grow' }), el('button', { class: 'ghost small', text: 'Clear', onclick: () => { saved.tray = []; save(); renderTray(); refreshAdds(); } })),
    el('div', { class: 'l-tray-list' }, ...tray.map((t, i) => el('div', { class: 'l-ti' },
      el('div', { class: 'l-ti-top' }, el('span', { class: 'l-code', text: t.code, title: 'Click to copy', onclick: () => copy(t.code) }),
        el('span', { class: 'l-grow' }), el('span', { class: 'n', text: num(t.time) == null ? (t.time || '—') : hrs(num(t.time)) }),
        el('button', { class: 'l-x', title: 'Remove', text: '✕', onclick: () => { saved.tray.splice(i, 1); save(); renderTray(); refreshAdds(); } })),
      el('div', { class: 'l-ti-name', text: [t.name, t.qual].filter(Boolean).join(' · ') })))),
    el('div', { class: 'l-totals' },
      el('div', {}, el('span', { text: 'Warranty' }), el('b', { text: `${hrs(w)} hrs` })),
      el('div', { class: 'cp' }, el('span', { text: `Customer pay (×${CUSTOMER_PAY})` }), el('b', { text: `${hrs(w * CUSTOMER_PAY)} hrs` })),
      actual ? el('div', { class: 'l-actual', text: `${actual} operation${actual === 1 ? '' : 's'} without a set time (actual time) not included.` }) : null),
    warns.length ? el('div', { class: 'l-warns' }, ...warns.map((x) => el('div', { text: x }))) : null,
    el('div', { class: 'l-tray-actions' },
      el('button', { class: 'ghost small', text: 'Copy op codes', onclick: () => copy(tray.map((t) => t.code).join(', '), 'op codes') }),
      el('button', { class: 'ghost small', text: 'One-time use parts', onclick: () => showParts(tray.map((t) => ({ op: { name: t.name, code: t.code }, opt: { code: t.code, qual: t.qual, cart: t.cart } }))) }),
      el('button', { class: 'primary small', text: 'Print', onclick: printTray })));
}
function paintAdd(btn) {
  const on = saved.tray.some((t) => `${t.code}|${t.qual}` === btn.dataset.add);
  btn.className = on ? 'ghost small added' : 'primary small';
  btn.textContent = on ? '✓ Added' : '+ Add';
  btn.title = on ? 'Take it out of your list' : 'Add to your list';
}
function refreshAdds() { document.querySelectorAll('button[data-add]').forEach(paintAdd); }
