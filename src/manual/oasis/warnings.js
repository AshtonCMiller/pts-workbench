// OASIS, warning messages: the warnings Ford's OASIS lists for this VIN, shown as a list.
'use strict';

// ---------- warning messages ----------
function readWarnings(doc) {
  const body = doc.querySelector('#warnmsg-panel .panel-body');
  const title = headerText(doc, 'warnmsg-panel');
  const lines = [];
  if (body) {
    const items = body.querySelectorAll('li, p, label, .row > div');
    const seen = new Set();
    (items.length ? [...items] : [body]).forEach((n) => {
      if (n.querySelector('li, p, label') && items.length) return; // only the innermost pieces
      const t = clean(n.textContent);
      if (t && !seen.has(t)) { seen.add(t); lines.push(t); }
    });
  }
  // OASIS hides the "imported by HUSM" notice unless it applies
  const husm = doc.getElementById('husmDiv');
  if (husm && !husm.hasAttribute('data-hidden')) {
    const t = clean(doc.getElementById('husmMessage')?.textContent);
    if (t) lines.push([t, clean(doc.getElementById('husmInformation')?.textContent)].filter(Boolean).join(' — '));
  }
  return { found: !!body, title, lines };
}
function renderWarnings(warn) {
  const box = el('section', { class: 'o-section', id: 'warnings' }, el('h2', { text: 'Warning messages' }));
  if (!warn.lines.length) {
    box.append(el('div', { class: 'o-card o-msg ok' }, el('b', { text: '✓ ' }),
      warn.found ? 'No warning messages for this VIN.' : "OASIS didn't include warning messages for this VIN."));
    return box;
  }
  box.append(el('div', { class: 'o-card o-warns' }, ...warn.lines.map((t) => el('div', { class: 'o-warn', text: t }))));
  return box;
}
