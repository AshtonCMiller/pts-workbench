// Reading a photo of the pay sheet, on the phone (Tesseract, from jsDelivr). The photo never leaves the phone.
//
// A pay sheet in a hand is shadowed, tilted, bent and often creased down the middle, so before it's read:
// - the page's own light is divided out (image.js flatten): print black on white all over;
// - it's turned level (image.js skewOf);
// and it's read twice: once whole (for the dates at the top, the total at the bottom, and where the pay type
// column is), and once in two strips cut at the pay type column, bigger, so a crease or a bend between the two
// halves of a row can't join one row's left to another's right. Each reading's rows are put together from where
// their words are (rows.js), then the two readings compared (sheet.js combine).

import { Sheet } from './sheet.js';
import { flatten, skewOf, wordsOf } from './image.js';
import { linesFromWords } from './rows.js';

const TESSERACT = 'https://cdn.jsdelivr.net/npm/tesseract.js@5.1.1/dist/tesseract.min.js';
let reader = null;

export function loadScript(src, what) {
  return new Promise((ok, fail) => {
    const s = document.createElement('script');
    s.src = src;
    s.onload = ok;
    s.onerror = () => fail(new Error(`${what} didn’t load (no internet?)`));
    document.head.append(s);
  });
}

/** The photo, gray and flattened (its light divided out), `long` pixels on its long side: a canvas */
function prepared(bitmap, long) {
  const scale = long / Math.max(bitmap.width, bitmap.height);
  const c = document.createElement('canvas');
  c.width = Math.round(bitmap.width * scale);
  c.height = Math.round(bitmap.height * scale);
  const ctx = c.getContext('2d', { willReadFrequently: true });
  ctx.imageSmoothingQuality = 'high';
  ctx.drawImage(bitmap, 0, 0, c.width, c.height);
  const img = ctx.getImageData(0, 0, c.width, c.height);
  const d = img.data;
  const gray = new Uint8ClampedArray(c.width * c.height);
  for (let i = 0, p = 0; i < d.length; i += 4, p++) gray[p] = (d[i] * 299 + d[i + 1] * 587 + d[i + 2] * 114) / 1000;
  const flat = flatten(gray, c.width, c.height);
  for (let i = 0, p = 0; i < d.length; i += 4, p++) {
    d[i] = d[i + 1] = d[i + 2] = flat[p];
    d[i + 3] = 255;
  }
  ctx.putImageData(img, 0, 0);
  return { canvas: c, gray: flat };
}

/** The canvas turned `deg` the other way (rows that slope `deg` come out level), on white */
function levelled(src, deg) {
  if (!deg) return src;
  const r = (-deg * Math.PI) / 180;
  const w = Math.ceil(Math.abs(src.width * Math.cos(r)) + Math.abs(src.height * Math.sin(r)));
  const h = Math.ceil(Math.abs(src.width * Math.sin(r)) + Math.abs(src.height * Math.cos(r)));
  const c = document.createElement('canvas');
  c.width = w;
  c.height = h;
  const ctx = c.getContext('2d');
  ctx.fillStyle = '#fff';
  ctx.fillRect(0, 0, w, h);
  ctx.translate(w / 2, h / 2);
  ctx.rotate(r);
  ctx.imageSmoothingQuality = 'high';
  ctx.drawImage(src, -src.width / 2, -src.height / 2);
  return c;
}

/** Part of a canvas, from x0 to x1 */
function strip(src, x0, x1) {
  const c = document.createElement('canvas');
  c.width = Math.max(1, Math.round(x1 - x0));
  c.height = src.height;
  c.getContext('2d').drawImage(src, -x0, 0);
  return c;
}

/** A canvas read: its text and its words (with where they are) */
async function read(canvas, dx = 0) {
  const { data } = await reader.recognize(canvas, {}, { text: true, tsv: true });
  return { text: data.text || '', words: wordsOf(data.tsv).map((w) => ({ ...w, x0: w.x0 + dx, x1: w.x1 + dx })) };
}

/** Where the pay type (ROP) column starts, from a reading's words (none: not found) */
function payColumn(words) {
  const xs = words.filter((w) => /^[^A-Za-z0-9]*[A-Za-z]{1,2}[Oo0][PpRrEe][^A-Za-z0-9]*$/.test(w.text)).map((w) => w.x0).sort((a, b) => a - b);
  return xs.length >= 3 ? xs[Math.floor(xs.length / 2)] : null;
}

/** A photo of a page → its rows (the two readings, compared). `step(text, fraction)` tells how it's going. */
export async function readPage(file, step) {
  step('Getting the text reader ready…', 0.03);
  if (!window.Tesseract) await loadScript(TESSERACT, 'The text reader');
  let pass = 0;
  if (!reader) {
    reader = await window.Tesseract.createWorker('eng', 1, {
      logger: (m) => {
        if (m.status === 'recognizing text') step(`Reading the page (${pass} of 3)…`, 0.12 + (pass - 1 + m.progress) * 0.28);
        else if (/load|initializ/i.test(m.status || '')) step('Getting the text reader ready…', 0.05 + (m.progress || 0) * 0.05);
      },
    });
    await reader.setParameters({ tessedit_pageseg_mode: '6', preserve_interword_spaces: '1' });
  }
  const bitmap = await createImageBitmap(file);
  const long = Math.max(bitmap.width, bitmap.height);

  // the whole page
  step('Straightening the page…', 0.1);
  const first = prepared(bitmap, Math.min(long, 2016));
  const deg = skewOf(first.gray, first.canvas.width, first.canvas.height);
  pass = 1;
  const firstLevel = levelled(first.canvas, deg);
  const whole = await read(firstLevel);
  const a = Sheet.parse(whole.text, linesFromWords(whole.words));

  // bigger, in two strips cut at the pay type column (or whole, when the column wasn't found)
  const big = Math.min(Math.round(long * 1.5), 3200);
  const second = levelled(prepared(bitmap, big).canvas, deg);
  const scale = second.width / firstLevel.width;
  const pay = payColumn(whole.words);
  let words;
  let total = null;
  if (pay === null) {
    pass = 2;
    words = (await read(second)).words;
  } else {
    const cut = pay * scale - 12;
    pass = 2;
    const left = await read(strip(second, 0, cut + 10));
    pass = 3;
    const right = await read(strip(second, cut, second.width), cut);
    words = [...left.words, ...right.words];
    total = Sheet.totalOfStrip(right.text);
  }
  const rows = linesFromWords(words);
  // the total (read bigger, in the strip, first): never less than this page's own rows come to (it's all pages')
  const own = Sheet.parse(whole.text, rows).rows.reduce((s, r) => s + (r.sold || 0), 0);
  total = [total, a.total].find((t) => t !== null && t !== undefined && t >= own - 0.05) ?? null;
  const b = { ...Sheet.parse(whole.text, rows), total };
  // (the strips read better: theirs first, where the two differ)
  const strips = b.rows.length >= a.rows.length / 2;
  return { ...Sheet.combine(strips ? b : { ...a, total }, strips ? a : b), total };
}
