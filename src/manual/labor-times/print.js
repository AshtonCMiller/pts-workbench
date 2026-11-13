// Labor Times, printing: the tray (operations and their times, with their one-time use parts) or a parts list, on a
// page of its own.
'use strict';

// ---------- printing ----------
let vehicleLine = '';
function printBlocks(title, blocks, extra = []) {
  fill($('l-print'), el('h1', { text: title }), vehicleLine ? el('div', { class: 'l-print-sub', text: vehicleLine }) : null, ...extra, ...blocks);
  document.body.classList.add('printing');
  setTimeout(() => { window.print(); document.body.classList.remove('printing'); }, 50);
}
async function printTray() {
  const tray = saved.tray;
  let w = 0;
  for (const t of tray) { const n = num(t.time); if (n != null) w += n; }
  const table = el('table', { class: 'l-ptable' },
    el('thead', {}, el('tr', {}, el('th', { text: 'Op code' }), el('th', { text: 'Operation' }), el('th', { text: 'Warranty' }), el('th', { text: 'Customer pay' }))),
    el('tbody', {}, ...tray.map((t) => el('tr', {}, el('td', { text: t.code }), el('td', { text: [t.name, t.qual].filter(Boolean).join(' · ') }),
      el('td', { text: num(t.time) == null ? (t.time || '—') : hrs(num(t.time)) }), el('td', { text: num(t.time) == null ? '—' : hrs(num(t.time) * CUSTOMER_PAY) })))),
    el('tfoot', {}, el('tr', {}, el('td', { text: 'Total' }), el('td'), el('td', { text: hrs(w) }), el('td', { text: hrs(w * CUSTOMER_PAY) }))));
  toast('Getting one-time use parts…');
  const parts = await partsBlock(tray.map((t) => ({ op: { name: t.name }, opt: { code: t.code, qual: t.qual, cart: t.cart } })));
  const warns = trayWarnings();
  printBlocks('Labor operations', [el('h2', { text: 'One-time use parts' }), ...parts],
    [table, ...(warns.length ? [el('div', { class: 'l-warns' }, ...warns.map((x) => el('div', { text: x })))] : [])]);
}
