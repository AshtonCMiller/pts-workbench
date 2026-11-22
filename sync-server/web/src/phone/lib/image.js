// Getting a photo of the pay sheet ready to be read (ocr.js): on gray pixels (a Uint8ClampedArray, one byte each,
// `w` × `h`), so it's the same on any phone and can be tried out away from one.

/**
 * The page's own light divided out, then the contrast stretched: shadows (a hand, the phone over it) and glare go,
 * and the print is black on white all over. The light at each place: the brightest of the paper around it (in
 * blocks, so the print doesn't count as shadow), smoothed.
 */
export function flatten(gray, w, h) {
  const B = Math.max(8, Math.round(Math.max(w, h) / 84)); // a block: about two rows of print
  const bw = Math.ceil(w / B);
  const bh = Math.ceil(h / B);
  // the brightest in each block (well, the 95th percentile: a speck of glare isn't the paper)
  let light = new Float32Array(bw * bh);
  const hist = new Uint32Array(256);
  for (let by = 0; by < bh; by++) {
    for (let bx = 0; bx < bw; bx++) {
      hist.fill(0);
      let n = 0;
      for (let y = by * B; y < Math.min(h, (by + 1) * B); y += 2) {
        for (let x = bx * B; x < Math.min(w, (bx + 1) * B); x += 2) {
          hist[gray[y * w + x]]++;
          n++;
        }
      }
      let acc = 0;
      let v = 255;
      for (; v > 0 && (acc += hist[v]) < n * 0.05; v--);
      light[by * bw + bx] = v;
    }
  }
  // the brightest of each block and its neighbours (a block that's all print), then smoothed
  const grow = new Float32Array(bw * bh);
  for (let y = 0; y < bh; y++) {
    for (let x = 0; x < bw; x++) {
      let m = 0;
      for (let dy = -1; dy <= 1; dy++) for (let dx = -1; dx <= 1; dx++) {
        const yy = y + dy;
        const xx = x + dx;
        if (yy >= 0 && yy < bh && xx >= 0 && xx < bw) m = Math.max(m, light[yy * bw + xx]);
      }
      grow[y * bw + x] = m;
    }
  }
  light = new Float32Array(bw * bh);
  for (let y = 0; y < bh; y++) {
    for (let x = 0; x < bw; x++) {
      let s = 0;
      let n = 0;
      for (let dy = -1; dy <= 1; dy++) for (let dx = -1; dx <= 1; dx++) {
        const yy = y + dy;
        const xx = x + dx;
        if (yy >= 0 && yy < bh && xx >= 0 && xx < bw) {
          s += grow[yy * bw + xx];
          n++;
        }
      }
      light[y * bw + x] = s / n;
    }
  }
  // each pixel against the light where it is (between the blocks' middles)
  const out = new Float32Array(w * h);
  const at = (x, y) => light[Math.min(bh - 1, Math.max(0, y)) * bw + Math.min(bw - 1, Math.max(0, x))];
  const hist2 = new Uint32Array(256);
  for (let y = 0; y < h; y++) {
    const fy = y / B - 0.5;
    const y0 = Math.floor(fy);
    const ty = fy - y0;
    for (let x = 0; x < w; x++) {
      const fx = x / B - 0.5;
      const x0 = Math.floor(fx);
      const tx = fx - x0;
      const l = (at(x0, y0) * (1 - tx) + at(x0 + 1, y0) * tx) * (1 - ty) + (at(x0, y0 + 1) * (1 - tx) + at(x0 + 1, y0 + 1) * tx) * ty;
      const v = Math.min(255, (gray[y * w + x] / Math.max(l, 1)) * 255);
      out[y * w + x] = v;
      hist2[v | 0]++;
    }
  }
  // the contrast stretched (the darkest and lightest 1% go to black and white)
  const total = w * h;
  let lo = 0;
  let hi = 255;
  let acc = 0;
  for (; lo < 255 && (acc += hist2[lo]) < total * 0.01; lo++);
  acc = 0;
  for (; hi > 0 && (acc += hist2[hi]) < total * 0.01; hi--);
  const span = Math.max(1, hi - lo);
  const res = new Uint8ClampedArray(w * h);
  for (let i = 0; i < total; i++) res[i] = ((out[i] - lo) * 255) / span;
  return res;
}

/**
 * How far the rows slope (degrees; + down to the right), from where the print is: the slope that lines the dark
 * pixels up into the sharpest rows (−6° to 6°).
 */
export function skewOf(gray, w, h) {
  // the dark pixels, from at most about 900 across
  const step = Math.max(1, Math.round(w / 900));
  const pts = [];
  for (let y = 0; y < h; y += step) for (let x = 0; x < w; x += step) if (gray[y * w + x] < 110) pts.push(x / step, y / step);
  const H = Math.ceil(h / step) + Math.ceil(w / step) * 0.11 + 4;
  const bins = new Float64Array(Math.ceil(H * 2));
  const sharp = (deg) => {
    const t = Math.tan((deg * Math.PI) / 180);
    bins.fill(0);
    const off = Math.ceil(w / step) * 0.11 + 2;
    for (let i = 0; i < pts.length; i += 2) bins[Math.round(pts[i + 1] - pts[i] * t + off)]++;
    let s = 0;
    for (let i = 1; i < bins.length; i++) s += (bins[i] - bins[i - 1]) ** 2;
    return s;
  };
  let best = 0;
  let bestS = -1;
  for (let d = -6; d <= 6.001; d += 0.25) {
    const s = sharp(d);
    if (s > bestS) {
      bestS = s;
      best = d;
    }
  }
  const around = best;
  for (let d = around - 0.25; d <= around + 0.251; d += 0.05) {
    const s = sharp(d);
    if (s > bestS) {
      bestS = s;
      best = d;
    }
  }
  return Math.round(best * 100) / 100;
}

/** Tesseract's TSV → its words, with their boxes */
export function wordsOf(tsv) {
  return String(tsv || '')
    .split('\n')
    .map((l) => l.split('\t'))
    .filter((c) => c[0] === '5' && c[11] && c[11].trim())
    .map((c) => ({ text: c[11].trim(), x0: +c[6], y0: +c[7], x1: +c[6] + +c[8], y1: +c[7] + +c[9] }));
}
