// The page's rows from where each word is on it, not from the lines the text reader made of them. A photo of a
// page held in a hand is tilted, bent, or creased, and then its rows slope or jump: the reader joins the left of
// one row to the right of the row above. Here:
//
// - each row's left part is followed from its RO number (or, that unreadable, its booked date) across to the pay
//   type, word to word, each next word taken at the height the row has got to (so it follows the row's slope);
// - each row's right part the same way, from its pay type (ROP) on: the hours, the op code, the description;
// - and the two put together in order: the nth left part with the nth right part, as near as they line up, the
//   way they line up following the page down (a crease shifts the right side a little more each row; it's the
//   rows above that say how much).
//
// `words`: [{ text, x0, y0, x1, y1 }] (Tesseract's, with their boxes) → the rows' text, one line each, top to bottom.

const median = (xs) => {
  const s = [...xs].sort((a, b) => a - b);
  return s.length ? s[Math.floor(s.length / 2)] : 0;
};
const digitish = (t) => (String(t).match(/[0-9OoSsBIl|]/g) || []).length;
const isRo = (t) => /^\W{0,2}[0-9A-Za-z|]{5,7}\W{0,2}$/.test(t) && (String(t).match(/\d/g) || []).length >= 4 && digitish(t) >= 5;
const isDate = (t) => /^[^A-Za-z0-9]*[0-9A-Za-z|!]{1,3}(JAN|FEB|MAR|APR|MAY|JUN|JUL|AUG|SEP|OCT|NOV|DEC|N0V|0CT)/i.test(t);
// the pay type, ROP, as a photo reads it (sheet.js PAY)
const isPay = (t) => /^[^A-Za-z0-9]*[A-Za-z]{1,2}[Oo0][PpRrEe][^A-Za-z0-9]*$/.test(t);

export function linesFromWords(input) {
  const words = input
    .filter((w) => String(w.text).trim())
    .map((w) => ({ ...w, cx: (w.x0 + w.x1) / 2, cy: (w.y0 + w.y1) / 2, h: w.y1 - w.y0, used: false }));
  if (!words.length) return [];
  const h = median(words.map((w) => w.h)) || 10;

  // a column: where the leftmost of these words line up (the tech number further along looks like an RO too)
  const col = (list) => {
    const xs = list.map((w) => w.x0).sort((p, q) => p - q);
    if (!xs.length) return null;
    const lo = xs[Math.floor(xs.length * 0.1)];
    return median(xs.filter((x) => x < lo + 3 * h));
  };

  // the left parts' starts: RO numbers, and a booked date where the RO couldn't be read
  const roX = col(words.filter((w) => isRo(w.text)));
  if (roX === null) return [];
  const anchors = words.filter((w) => isRo(w.text) && Math.abs(w.x0 - roX) < 3 * h);
  const dateX = col(words.filter((w) => isDate(w.text) && w.x0 > roX + h));
  if (dateX !== null) {
    for (const d of words) {
      if (!isDate(d.text) || Math.abs(d.x0 - dateX) > 2 * h) continue;
      if (!anchors.some((a) => Math.abs(a.cy - d.cy) < 0.6 * h)) anchors.push(d);
    }
  }
  anchors.sort((a, b) => a.cy - b.cy);
  const top = anchors.length ? anchors[0].cy - 2 * h : 0;

  // the right parts' starts: the pay type (or whatever's in its column, unreadable)
  const payX = col(words.filter((w) => isPay(w.text) && w.cy > top && w.x0 > roX + 10 * h));
  const pays = payX === null ? [] : words
    .filter((w) => w.cy > top && Math.abs(w.x0 - payX) < 1.5 * h && (isPay(w.text) || !/\d[.,]\d/.test(w.text)))
    .sort((a, b) => a.cy - b.cy);

  /** A row's words from `start`, rightwards (up to `until`, an x), following its slope */
  const follow = (start, until, slopeHint) => {
    start.used = true;
    const chain = [start];
    let slope = slopeHint;
    let cur = start;
    for (;;) {
      let best = null;
      let bestScore = Infinity;
      for (const w of words) {
        if (w.used || w.x0 < cur.x1 - 0.3 * h || w.x0 > cur.x1 + 14 * h || w.x0 >= until) continue;
        const dy = Math.abs(w.cy - (cur.cy + slope * (w.cx - cur.cx)));
        if (dy > 0.5 * h) continue;
        const score = w.x0 - cur.x1 + dy * 6;
        if (score < bestScore) {
          bestScore = score;
          best = w;
        }
      }
      if (!best) break;
      best.used = true;
      chain.push(best);
      cur = best;
      if (cur.cx - start.cx > 8 * h) slope = (cur.cy - start.cy) / (cur.cx - start.cx);
    }
    return { y: start.cy, end: cur.cy, slope, text: chain.map((w) => w.text).join(' ') };
  };

  // the right parts first (so a left part stops where they start)
  const slopes = [];
  const rights = pays.map((p) => {
    if (p.used) return null;
    const r = follow(p, Infinity, slopes.length ? median(slopes.slice(-5)) : 0);
    slopes.push(r.slope);
    return r;
  }).filter(Boolean);
  slopes.length = 0;
  const lefts = anchors.map((a) => {
    if (a.used) return null;
    const l = follow(a, payX === null ? Infinity : payX - 0.3 * h, slopes.length ? median(slopes.slice(-5)) : 0);
    slopes.push(l.slope);
    return l;
  }).filter(Boolean);

  // put together, in order: each left part with the right part that keeps how far down the right side sits
  // closest to the rows' around it (it changes slowly down the page, never a row at once), leaving out a part
  // whose other half couldn't be read
  const pitch = median(lefts.slice(1).map((l, i) => l.y - lefts[i].y).filter((d) => d > 0.5 * h)) || 1.4 * h;
  const SKIP = 0.35; // (a part left out costs about a third of a row's jump)
  const BACK = 3;
  const n = lefts.length;
  const m = rights.length;
  const shiftOf = (i, k) => rights[k].y - lefts[i].end;
  const cost = Array.from({ length: n }, () => new Array(m).fill(Infinity));
  const from = Array.from({ length: n }, () => new Array(m).fill(null));
  for (let i = 0; i < n; i++) {
    for (let k = 0; k < m; k++) {
      if (Math.abs(shiftOf(i, k)) > 1.5 * pitch) continue;
      // (the first pair: everything before it left out)
      let best = SKIP * (Math.min(i, BACK) + Math.min(k, BACK)) + Math.abs(shiftOf(i, k)) / pitch;
      let prev = null;
      for (let a = Math.max(0, i - BACK); a < i; a++) {
        for (let b = Math.max(0, k - BACK); b < k; b++) {
          if (cost[a][b] === Infinity) continue;
          const c = cost[a][b] + Math.abs(shiftOf(i, k) - shiftOf(a, b)) / pitch + SKIP * (i - a - 1 + k - b - 1);
          if (c < best) {
            best = c;
            prev = [a, b];
          }
        }
      }
      cost[i][k] = best;
      from[i][k] = prev;
    }
  }
  let end = null;
  let endCost = Infinity;
  for (let i = 0; i < n; i++) {
    for (let k = 0; k < m; k++) {
      const c = cost[i][k] + SKIP * (Math.min(n - 1 - i, BACK) + Math.min(m - 1 - k, BACK));
      if (c < endCost) {
        endCost = c;
        end = [i, k];
      }
    }
  }
  const pairOf = new Map();
  for (let at = end; at; at = from[at[0]][at[1]]) pairOf.set(at[0], at[1]);
  const out = lefts.map((l, i) => ({ y: l.y, text: pairOf.has(i) ? `${l.text} ${rights[pairOf.get(i)].text}` : l.text }));
  return out.sort((p, q) => p.y - q.y).map((l) => l.text);
}
