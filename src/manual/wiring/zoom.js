// Wiring Diagrams, moving around a page: zoom (buttons, wheel, double-click) and drag, fitting the page, the zoom-in
// on a searched item that eases back out (stopped by anything you do), and marking items on the page.
'use strict';

// ---------- zoom and drag ----------
const view = { s: 1, x: 0, y: 0 };
let zoomed = false;
function fitScale() {
  const st = $('w-stage');
  return Math.min(st.clientWidth / size.w, st.clientHeight / size.h) * 0.98 || 1;
}
function apply() {
  const p = $('w-paper');
  p.style.width = `${size.w * view.s}px`;
  p.style.height = `${size.h * view.s}px`;
  p.style.transform = `translate(${view.x}px, ${view.y}px)`;
}
function fit() {
  stopAnim();
  const st = $('w-stage');
  view.s = fitScale();
  view.x = (st.clientWidth - size.w * view.s) / 2;
  view.y = (st.clientHeight - size.h * view.s) / 2;
  zoomed = false;
  apply();
}
function zoomAt(factor, mx, my) {
  stopAnim();
  const st = $('w-stage');
  if (mx == null) { mx = st.clientWidth / 2; my = st.clientHeight / 2; }
  const min = fitScale() * 0.5, max = fitScale() * 14;
  const ns = Math.min(max, Math.max(min, view.s * factor));
  view.x = mx - (mx - view.x) * (ns / view.s);
  view.y = my - (my - view.y) * (ns / view.s);
  view.s = ns;
  zoomed = true;
  apply();
}
// Centre on part of the drawing (drawing units), zoomed in
function zoomTo(box) {
  stopAnim();
  const st = $('w-stage');
  view.s = Math.min(fitScale() * 3, Math.max(fitScale(), Math.min(st.clientWidth / (box.width + 80), st.clientHeight / (box.height + 80))));
  const cx = (box.x + box.width / 2 - size.x) * view.s, cy = (box.y + box.height / 2 - size.y) * view.s;
  view.x = st.clientWidth / 2 - cx;
  view.y = st.clientHeight / 2 - cy;
  zoomed = true;
  apply();
}

// Opened for one part: it starts zoomed in on it, then eases back out to the whole page. Anything you do
// (drag, scroll, zoom, a key, another page) stops it where it is. While it runs, the page is scaled as a picture
// (smooth); at the end (or when stopped) it's drawn sharp at that size.
let anim = null;
function stopAnim() {
  if (!anim) return;
  const a = anim;
  anim = null;
  clearTimeout(a.wait); cancelAnimationFrame(a.frame);
  if (a.started) { Object.assign(view, a.now); $('w-paper').style.transformOrigin = ''; apply(); }
}
function zoomOutLater(delay = 900, ms = 1100) {
  const st = $('w-stage');
  const from = { ...view };
  const s = fitScale();
  const to = { s, x: (st.clientWidth - size.w * s) / 2, y: (st.clientHeight - size.h * s) / 2 };
  const a = { now: { ...from }, started: false };
  anim = a;
  const ease = (t) => (t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2);
  a.wait = setTimeout(() => {
    if (anim !== a) return;
    a.started = true;
    const t0 = performance.now();
    const p = $('w-paper');
    p.style.transformOrigin = '0 0';
    const step = (now) => {
      if (anim !== a) return;
      const t = Math.min(1, (now - t0) / ms), k = ease(t);
      a.now = { s: from.s * Math.pow(to.s / from.s, k), x: from.x + (to.x - from.x) * k, y: from.y + (to.y - from.y) * k };
      // the page stays drawn at its starting size and is scaled as a picture while moving
      p.style.transform = `translate(${a.now.x}px, ${a.now.y}px) scale(${a.now.s / from.s})`;
      if (t < 1) { a.frame = requestAnimationFrame(step); return; }
      anim = null;
      p.style.transformOrigin = '';
      Object.assign(view, to);
      zoomed = false;
      apply();
    };
    a.frame = requestAnimationFrame(step);
  }, delay);
}

// A connector / splice / ground / component the page was opened for: marked, zoomed to, then the whole page shown
// The box of part of the drawing, in the drawing's units (it may sit inside moved / scaled groups)
function boxOf(svg, h) {
  let b;
  try { b = h.getBBox(); } catch { return null; }
  if (!b || (!b.width && !b.height)) return null;
  let m = null;
  try { m = svg.getScreenCTM().inverse().multiply(h.getScreenCTM()); } catch { /* use the box as it is */ }
  const pt = (x, y) => { if (!m) return { x, y }; const p = new DOMPoint(x, y).matrixTransform(m); return { x: p.x, y: p.y }; };
  const a = pt(b.x, b.y), c = pt(b.x + b.width, b.y + b.height);
  return { x: Math.min(a.x, c.x), y: Math.min(a.y, c.y), width: Math.abs(c.x - a.x), height: Math.abs(c.y - a.y) };
}
function addMark(svg, box, cls = 'w-mark') {
  const r = document.createElementNS('http://www.w3.org/2000/svg', 'rect');
  r.setAttribute('x', box.x - 4); r.setAttribute('y', box.y - 4);
  r.setAttribute('width', box.width + 8); r.setAttribute('height', box.height + 8);
  r.setAttribute('rx', 3); r.setAttribute('class', cls);
  svg.append(r); // on top, see-through (Ford draws wires as white lines over black ones)
  return r;
}

// A connector / splice / ground (or a component, by name) the page was opened for: marked and zoomed to
function markItem(item) {
  fit();
  const svg = $('w-paper').querySelector('svg');
  // compared without spaces / punctuation ("C1689", "c1689-2", "Battery Monitoring Sensor (BMS)")
  const norm = (t) => String(t || '').toUpperCase().replace(/[^A-Z0-9]/g, '');
  const want = norm(item);
  const short = norm(String(item).replace(/\(.*?\)/g, '')); // without "(BMS)"
  const code = (String(item).match(/\(([^)]+)\)/) || [])[1]; // "BMS"
  const same = (t) => { const n = norm(t); return n && (n === want || n === short || (code && n === norm(code))); };
  // connectors / splices / grounds: Ford's clickable names (name="C1689"); a pin number on the end doesn't matter
  let hits = [...svg.querySelectorAll('a[name]')].filter((a) => same(a.getAttribute('name')) || (want.length > 3 && want.startsWith(norm(a.getAttribute('name'))) && /^[CSG]\d/i.test(a.getAttribute('name'))));
  // components: Ford names their groups "COMP_<code>~INDEX~<NAME>"
  if (!hits.length) hits = [...svg.querySelectorAll('g[id^="COMP_"], g[id^="comp_"]')].filter((g) => g.id.split('~').some((x) => same(x.replace(/^COMP_/i, ''))));
  // anything else: its label
  if (!hits.length) hits = [...svg.querySelectorAll('text')].filter((t) => same(t.textContent));
  if (!hits.length && short.length >= 4) hits = [...svg.querySelectorAll('g[id^="COMP_"], g[id^="comp_"]')].filter((g) => norm(g.id).includes(short));
  if (!hits.length) {
    const comps = [...svg.querySelectorAll('g[id^="COMP_"], g[id^="comp_"]')].slice(0, 8).map((g) => g.id).join(' | ');
    const names = [...svg.querySelectorAll('a[name]')].slice(0, 12).map((a) => a.getAttribute('name')).join(',');
    api.log(`wiring: "${item}" not found on ${state.at?.cell}-${state.at?.page}; components: ${comps}; names: ${names}`);
    toast(`${item} isn't labeled on this page`);
    return;
  }
  let first = null;
  for (const h of hits) {
    const box = boxOf(svg, h);
    if (!box) continue;
    addMark(svg, box);
    first ||= box;
  }
  if (first) { zoomTo(first); zoomOutLater(); }
}

let drag = null;
function linkTarget(a) {
  const href = a?.getAttribute('data-href');
  if (!href || !/\/wiring\/page/i.test(href)) return null;
  try { return new URL(href.replace(/&amp;/g, '&'), 'https://www.fordtechservice.dealerconnection.com/').searchParams; } catch { return null; }
}
function stageEvents() {
  const st = $('w-stage');
  st.addEventListener('pointerover', (e) => {
    if (drag) return;
    const sp = linkTarget(e.target.closest?.('a.w-link'));
    if (sp) warm(sp.get('cell'), sp.get('page'));
  });
  st.addEventListener('wheel', (e) => {
    stopAnim();
    e.preventDefault();
    const r = st.getBoundingClientRect();
    zoomAt(Math.exp(-e.deltaY * (e.ctrlKey ? 0.01 : 0.0015)), e.clientX - r.left, e.clientY - r.top);
  }, { passive: false });
  st.addEventListener('pointerdown', (e) => {
    stopAnim();
    if (e.button !== 0) return;
    drag = { x: e.clientX, y: e.clientY, vx: view.x, vy: view.y, moved: false, id: e.pointerId };
  });
  st.addEventListener('pointermove', (e) => {
    if (!drag || drag.id !== e.pointerId) return;
    const dx = e.clientX - drag.x, dy = e.clientY - drag.y;
    if (!drag.moved && Math.hypot(dx, dy) < 4) return;
    if (!drag.moved) { drag.moved = true; st.setPointerCapture(e.pointerId); st.classList.add('dragging'); hidePop(); }
    view.x = drag.vx + dx; view.y = drag.vy + dy; zoomed = true;
    apply();
  });
  const end = (e) => {
    if (!drag || drag.id !== e.pointerId) return;
    const moved = drag.moved;
    drag = null;
    st.classList.remove('dragging');
    if (!moved && e.type === 'pointerup') clickAt(e);
  };
  st.addEventListener('pointerup', end);
  st.addEventListener('pointercancel', end);
  st.addEventListener('dblclick', (e) => {
    if (e.target.closest?.('a.w-link')) return;
    const r = st.getBoundingClientRect();
    zoomAt(2, e.clientX - r.left, e.clientY - r.top);
  });
  new ResizeObserver(() => { if (!$('w-page-view').hidden && !zoomed) fit(); }).observe(st);
}
