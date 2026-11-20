// Reading a photo of the pay sheet, on the phone (Tesseract, from jsDelivr): each page read twice, at two sizes, and
// the two compared (sheet.js combine). The photo never leaves the phone.

import { Sheet } from './sheet.js';

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

/** The photo, upright, gray and with its contrast stretched, `long` pixels on its long side */
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
  const hist = new Uint32Array(256);
  for (let i = 0; i < d.length; i += 4) {
    const g = ((d[i] * 299 + d[i + 1] * 587 + d[i + 2] * 114) / 1000) | 0;
    d[i] = g;
    hist[g]++;
  }
  const total = d.length / 4;
  let lo = 0;
  let hi = 255;
  let acc = 0;
  for (; lo < 255 && (acc += hist[lo]) < total * 0.01; lo++);
  acc = 0;
  for (; hi > 0 && (acc += hist[hi]) < total * 0.01; hi--);
  const span = Math.max(1, hi - lo);
  for (let i = 0; i < d.length; i += 4) {
    const v = Math.max(0, Math.min(255, ((d[i] - lo) * 255) / span));
    d[i] = d[i + 1] = d[i + 2] = v;
  }
  ctx.putImageData(img, 0, 0);
  return c;
}

/** A photo of a page → its rows (Sheet.combine of the two readings). `step(text, fraction)` tells how it's going. */
export async function readPage(file, step) {
  step('Getting the text reader ready…', 0.03);
  if (!window.Tesseract) await loadScript(TESSERACT, 'The text reader');
  let pass = 0;
  if (!reader) {
    reader = await window.Tesseract.createWorker('eng', 1, {
      logger: (m) => {
        if (m.status === 'recognizing text') step(`Reading the page (${pass} of 2)…`, 0.1 + (pass - 1 + m.progress) * 0.42);
        else if (/load|initializ/i.test(m.status || '')) step('Getting the text reader ready…', 0.05 + (m.progress || 0) * 0.05);
      },
    });
    await reader.setParameters({ tessedit_pageseg_mode: '6', preserve_interword_spaces: '1' });
  }
  const bitmap = await createImageBitmap(file);
  const long = Math.max(bitmap.width, bitmap.height);
  const texts = [];
  for (const size of [Math.min(long, 2016), Math.min(Math.round(long * 1.5), 3200)]) {
    pass++;
    step(`Reading the page (${pass} of 2)…`, 0.1 + (pass - 1) * 0.42);
    const { data } = await reader.recognize(prepared(bitmap, size));
    texts.push(data.text);
  }
  return Sheet.combine(Sheet.parse(texts[0]), Sheet.parse(texts[1]));
}
