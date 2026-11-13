// Wiring Diagrams, the drawing itself: Ford's SVG without its scripts, click handlers or inline styles (this page's
// security rules don't allow them), Ford's styles carried over as plain attributes, pictures fixed up, and Ford's
// colors swapped for the app's theme (light / dark).
'use strict';

let size = { w: 1100, h: 850 };
// Ford's own styles (inline style="…" and <style> rules) as plain drawing attributes: this page's security rules
// don't allow inline styles, and the location views draw the vehicle (engine, body…) with them.
// visibility / display aren't carried over: Ford's file starts with its sheet hidden and its page script shows it.
const STYLE_PROPS = new Set(['fill', 'fill-opacity', 'fill-rule', 'stroke', 'stroke-width', 'stroke-opacity', 'stroke-dasharray', 'stroke-dashoffset',
  'stroke-linecap', 'stroke-linejoin', 'stroke-miterlimit', 'opacity', 'font-size', 'font-family', 'font-weight', 'font-style', 'text-anchor',
  'dominant-baseline', 'letter-spacing', 'clip-path', 'clip-rule', 'mask', 'stop-color', 'stop-opacity', 'color']);
function styleToAttrs(n, css, overwrite) {
  for (const part of String(css).split(';')) {
    const i = part.indexOf(':');
    if (i < 0) continue;
    const k = part.slice(0, i).trim().toLowerCase(), v = part.slice(i + 1).replace(/!important/i, '').trim();
    if (!STYLE_PROPS.has(k) || !v || /expression|javascript:|url\(\s*['"]?(?!#)/i.test(v)) continue;
    if (overwrite || !n.hasAttribute(k)) n.setAttribute(k, v);
  }
}
function applyStyleSheets(svg) {
  const rules = [];
  for (const st of svg.querySelectorAll('wstyle, style')) {
    const css = st.textContent.replace(/\/\*[\s\S]*?\*\//g, '').replace(/<!\[CDATA\[|\]\]>/g, '');
    for (const m of css.matchAll(/([^{}]+)\{([^{}]*)\}/g)) rules.push([m[1].trim(), m[2]]);
    st.remove();
  }
  for (const [sel, body] of rules) {
    for (const one of sel.split(',')) {
      let hits = [];
      try { hits = svg.querySelectorAll(one.trim()); } catch { continue; }
      for (const n of hits) styleToAttrs(n, body, true);
    }
  }
  return rules.length;
}

function draw(text, base = 'https://www.fordservicecontent.com/') {
  stopAnim();
  let src = String(text).replace(/<\?xml[\s\S]*?\?>/i, '').replace(/<!DOCTYPE[\s\S]*?>/i, '').replace(/<script\b[\s\S]*?<\/script>/gi, '');
  // Ford's notes about the drawing (<desc>, <metadata>) aren't drawn, and read as a web page they can swallow the
  // whole drawing (an empty <Qualifier/> there isn't closed in a web page), so they're taken out first
  src = src.replace(/<(desc|metadata)\b[\s\S]*?<\/\1\s*>/gi, '').replace(/<(desc|metadata)\b[^>]*\/>/gi, '');
  // styles renamed before reading, so this page's security rules don't complain (they're turned into attributes below)
  src = src.replace(/(<[^<>]*?\s)style(\s*=)/gi, '$1data-wstyle$2').replace(/<(\/?)style\b/gi, '<$1wstyle');
  // read as a drawing (SVG / XML) first; if Ford's file isn't tidy enough for that, as a web page
  let svg = null;
  const xml = new DOMParser().parseFromString(/xmlns\s*=\s*["']http:\/\/www\.w3\.org\/2000\/svg/i.test(src) ? src
    : src.replace(/<svg\b/i, '<svg xmlns="http://www.w3.org/2000/svg"'), 'image/svg+xml');
  if (!xml.querySelector('parsererror') && xml.documentElement?.localName === 'svg') svg = xml.documentElement;
  if (!svg) {
    const doc = new DOMParser().parseFromString(`<!doctype html><body>${src}</body>`, 'text/html');
    svg = doc.querySelector('svg');
  }
  if (!svg) throw new Error('no drawing in the reply');
  svg.querySelectorAll('script, foreignObject, iframe, object, embed').forEach((n) => n.remove());
  const sheetRules = applyStyleSheets(svg);
  const hidden = [], pictures = [];
  for (const n of [svg, ...svg.querySelectorAll('*')]) {
    const tag = n.localName.toLowerCase();
    for (const a of [...n.attributes]) {
      const name = a.name.toLowerCase();
      if (name.startsWith('on')) n.removeAttribute(a.name);
      else if (name === 'style' || name === 'data-wstyle') { styleToAttrs(n, a.value, true); n.removeAttribute(a.name); }
      else if (name === 'visibility' && /hidden/i.test(a.value)) { hidden.push(`${tag}#${n.id || '?'} visibility`); n.removeAttribute(a.name); } // Ford's page script shows these
      else if (name === 'display' && /none/i.test(a.value)) hidden.push(`${tag}#${n.id || '?'} display:none`);
      else if (name === 'href' || name === 'xlink:href') {
        const v = a.value.trim();
        if (tag === 'a') { n.setAttribute('data-href', v); n.removeAttribute(a.name); }
        else if (v.startsWith('#')) { /* a shape drawn from elsewhere in the drawing (<use>): kept */ }
        else if (tag === 'image' && /^data:(image\/[a-z0-9.+-]+)?[;,]/i.test(v)) {
          // a picture inside the file (the location views' vehicle). Ford leaves out its type ("data:;base64,…"),
          // so it's filled in from the picture's first bytes
          if (/^data:[;,]/i.test(v)) {
            const b64 = v.slice(v.indexOf(',') + 1, v.indexOf(',') + 9);
            const type = b64.startsWith('iVBOR') ? 'image/png' : b64.startsWith('/9j/') ? 'image/jpeg' : b64.startsWith('R0lG') ? 'image/gif'
              : b64.startsWith('UklGR') ? 'image/webp' : b64.startsWith('PHN2Zy') || b64.startsWith('PD94bW') ? 'image/svg+xml' : 'image/png';
            n.setAttribute(a.name, `data:${type}${v.slice(5)}`);
          }
          pictures.push(v.slice(0, 40));
        }
        else if (tag === 'image' || tag === 'feimage') { // a picture (the location views' vehicle): fetched with the PTS login
          let abs = '';
          // as on Ford's page (the drawing is put into a PTS page): /ford_content/… is Ford's content server, other
          // /… addresses are PTS, and the rest are next to the drawing's own file
          const from = /^\/ford_content\//i.test(v) ? 'https://www.fordservicecontent.com/' : /^\//.test(v) ? 'https://www.fordtechservice.dealerconnection.com/' : base;
          try { abs = new URL(v, from).href; } catch { /* bad address */ }
          pictures.push(abs || v);
          if (/^https:/i.test(abs)) n.setAttribute(a.name, `ptsasset://f/?u=${encodeURIComponent(abs)}`); else n.removeAttribute(a.name);
        } else n.removeAttribute(a.name);
      }
    }
  }
  if (hidden.length) api.log(`wiring: hidden in Ford's file: ${hidden.slice(0, 20).join(', ')}`);
  if (pictures.length || sheetRules) api.log(`wiring: ${pictures.length} picture(s)${pictures.length ? ` e.g. ${pictures[0].slice(0, 200)}` : ''}, ${sheetRules} style rule(s)`);
  const vb = (svg.getAttribute('viewBox') || '').trim().split(/[\s,]+/).map(Number);
  size = vb.length === 4 && vb[2] > 0 && vb[3] > 0 ? { x: vb[0], y: vb[1], w: vb[2], h: vb[3] }
    : { x: 0, y: 0, w: parseFloat(svg.getAttribute('width')) || 1100, h: parseFloat(svg.getAttribute('height')) || 850 };
  if (!(vb.length === 4)) svg.setAttribute('viewBox', `0 0 ${size.w} ${size.h}`);
  svg.setAttribute('width', '100%'); svg.setAttribute('height', '100%');
  svg.setAttribute('preserveAspectRatio', 'xMidYMid meet');
  svg.removeAttribute('id');
  for (const a of svg.querySelectorAll('a[data-href], a[name]')) a.classList.add('w-link');
  themeColors(svg);
  // the drawing's sheet(s): shown, as Ford's page does
  for (const g of svg.querySelectorAll('g[id^="Sheet"]')) { g.removeAttribute('visibility'); g.removeAttribute('display'); }
  $('w-paper').replaceChildren(document.importNode(svg, true));
  zoomed = false;
  // a record of what was drawn, for working out problems
  const shown = $('w-paper').querySelector('svg');
  let box = null;
  try { const b = shown.getBBox(); box = `${Math.round(b.x)},${Math.round(b.y)} ${Math.round(b.width)}×${Math.round(b.height)}`; } catch { /* not laid out */ }
  const tags = {};
  for (const n of shown.querySelectorAll('*')) tags[n.localName] = (tags[n.localName] || 0) + 1;
  const kids = [...shown.children].slice(0, 12).map((k) => { let b = ''; try { const r = k.getBBox(); b = ` ${Math.round(r.width)}×${Math.round(r.height)}`; } catch {} return `${k.localName}${k.id ? `#${k.id}` : ''}${b}`; });
  api.log(`wiring: elements ${Object.entries(tags).map(([k, v]) => `${k} ${v}`).join(', ')} | top: ${kids.join(', ')} | namespace ${shown.namespaceURI}`);
  api.log(`wiring: drew ${String(text).length} characters, ${shown.querySelectorAll('line, path, polyline, polygon, rect, text').length} shapes, ` +
    `view ${size.w}×${size.h}, drawing at ${box || '?'}; starts: ${String(text).replace(/\s+/g, ' ').slice(0, 300)}`);
}

// The app's colors: Ford's black lines and text, white paper, light gray boxes and blue links are tagged so the
// page's style can draw them in the app's light / dark colors. Wire colors (red, pink, blue wires…) stay as Ford drew them.
function colorKind(v, isText) {
  const c = String(v || '').trim().toLowerCase().replace(/\s+/g, '');
  if (!c || c === 'none' || c.startsWith('url(')) return '';
  let rgb = null;
  const named = { black: [0, 0, 0], white: [255, 255, 255], blue: [0, 0, 255], gray: [128, 128, 128], grey: [128, 128, 128], purple: [128, 0, 128] };
  if (named[c]) rgb = named[c];
  else if (/^#[0-9a-f]{3}$/.test(c)) rgb = [1, 2, 3].map((i) => parseInt(c[i] + c[i], 16));
  else if (/^#[0-9a-f]{6}$/.test(c)) rgb = [1, 3, 5].map((i) => parseInt(c.slice(i, i + 2), 16));
  else { const m = c.match(/^rgb\((\d+),(\d+),(\d+)\)$/); if (m) rgb = [m[1], m[2], m[3]].map(Number); }
  if (!rgb) return '';
  const [r, g, b] = rgb;
  const grayish = Math.max(r, g, b) - Math.min(r, g, b) < 12;
  if (grayish && r < 60) return 'ink';
  if (grayish && r > 245) return 'paper';
  if (grayish && r >= 200) return 'tint';                       // light gray boxes
  if (b > 180 && r < 90 && g < 90) return 'link';              // Ford's blue: page numbers, connector names, component outlines
  return '';
}
function themeColors(svg) {
  for (const n of svg.querySelectorAll('[fill], [stroke]')) {
    const isText = n.localName === 'text' || n.localName === 'tspan';
    const f = colorKind(n.getAttribute('fill'), isText), st = colorKind(n.getAttribute('stroke'), false);
    if (f) n.classList.add(`f-${f}`);
    if (st && !(st === 'link' && n.closest('[id^="CIRC"]'))) n.classList.add(`s-${st}`); // a blue wire stays Ford's blue
  }
  // text with no fill of its own is black
  for (const t of svg.querySelectorAll('text:not([fill])')) if (!t.closest('[fill]')) t.classList.add('f-ink');
}
