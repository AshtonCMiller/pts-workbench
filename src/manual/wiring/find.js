// Wiring Diagrams, finding words on the page showing (Ctrl+F): marks every match and steps through them.
'use strict';

// ---------- find on this page (Ctrl+F) ----------
let finds = [], findAt = -1;
function findOnPage(q) {
  const svg = $('w-paper').querySelector('svg');
  svg?.querySelectorAll('.w-findmark').forEach((n) => n.remove());
  finds = []; findAt = -1;
  q = clean(q).toUpperCase();
  if (!svg || !q) { $('w-find-n').textContent = ''; return; }
  for (const t of svg.querySelectorAll('text')) {
    if (!clean(t.textContent).toUpperCase().includes(q)) continue;
    const box = boxOf(svg, t);
    if (box) finds.push({ box, mark: addMark(svg, box, 'w-mark w-findmark') });
  }
  finds.sort((a, b) => (a.box.y - b.box.y) || (a.box.x - b.box.x));
  $('w-find-n').textContent = finds.length ? `${finds.length} found` : 'not on this page';
  if (finds.length) findNext();
}
function findNext(back = false) {
  if (!finds.length) return;
  finds[findAt]?.mark.classList.remove('cur');
  findAt = (findAt + (back ? -1 : 1) + finds.length) % finds.length;
  finds[findAt].mark.classList.add('cur');
  $('w-find-n').textContent = `${findAt + 1} of ${finds.length}`;
  zoomTo(finds[findAt].box);
}
