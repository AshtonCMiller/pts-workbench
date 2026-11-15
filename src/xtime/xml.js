// A small XML reader for Xtime's chat messages (XMPP stanzas), in place of the browser's DOMParser.
// parseXml(text) → the top element, or null when the text isn't XML. Each element has what the chat code uses from the
// browser's version: localName, getAttribute(name), children (elements), textContent, getElementsByTagName(name)
// (every element below with that local name) and firstElementChild.
// Namespace prefixes are dropped from names ("stream:features" → "features"), as the chat code only looks at local names.
'use strict';

const ENTITIES = { amp: '&', lt: '<', gt: '>', quot: '"', apos: "'" };
const decode = (s) => s.replace(/&(#x[0-9a-f]+|#\d+|\w+);/gi, (m, e) => {
  if (e[0] === '#') {
    const n = e[1] === 'x' || e[1] === 'X' ? parseInt(e.slice(2), 16) : parseInt(e.slice(1), 10);
    return Number.isFinite(n) ? String.fromCodePoint(n) : m;
  }
  return ENTITIES[e] ?? m;
});
const local = (name) => name.slice(name.indexOf(':') + 1);

class XmlElement {
  constructor(name, attrs) {
    this.localName = local(name);
    this.attrs = attrs;
    this.nodes = []; // elements and text, in order
  }
  getAttribute(name) { return Object.prototype.hasOwnProperty.call(this.attrs, name) ? this.attrs[name] : null; }
  get children() { return this.nodes.filter((n) => n instanceof XmlElement); }
  get firstElementChild() { return this.children[0] || null; }
  get textContent() { return this.nodes.map((n) => (n instanceof XmlElement ? n.textContent : n)).join(''); }
  getElementsByTagName(name) {
    const want = local(name);
    const out = [];
    const walk = (el) => { for (const c of el.children) { if (want === '*' || c.localName === want) out.push(c); walk(c); } };
    walk(this);
    return out;
  }
}

function parseXml(text) {
  const src = String(text || '');
  const stack = [];
  let root = null;
  let i = 0;
  const addText = (t) => { if (t && stack.length) stack[stack.length - 1].nodes.push(t); };
  while (i < src.length) {
    const lt = src.indexOf('<', i);
    if (lt < 0) { addText(decode(src.slice(i))); break; }
    if (lt > i) addText(decode(src.slice(i, lt)));
    if (src.startsWith('<!--', lt)) { const end = src.indexOf('-->', lt); if (end < 0) return null; i = end + 3; continue; }
    if (src.startsWith('<![CDATA[', lt)) { const end = src.indexOf(']]>', lt); if (end < 0) return null; addText(src.slice(lt + 9, end)); i = end + 3; continue; }
    if (src.startsWith('<?', lt) || src.startsWith('<!', lt)) { const end = src.indexOf('>', lt); if (end < 0) return null; i = end + 1; continue; }
    // a tag: find its end, skipping ">" inside quoted attribute values
    let j = lt + 1, quote = null;
    for (; j < src.length; j++) {
      const c = src[j];
      if (quote) { if (c === quote) quote = null; } else if (c === '"' || c === "'") quote = c; else if (c === '>') break;
    }
    if (j >= src.length) return null; // cut off
    const inner = src.slice(lt + 1, j);
    i = j + 1;
    if (inner[0] === '/') { // closing tag
      const name = inner.slice(1).trim();
      const el = stack.pop();
      if (!el || el.localName !== local(name)) return null; // badly nested: not XML
      if (!stack.length) root = el;
      continue;
    }
    const selfClose = inner.endsWith('/');
    const body = selfClose ? inner.slice(0, -1) : inner;
    const name = (body.match(/^[^\s/>]+/) || [''])[0];
    if (!name) return null;
    const attrs = {};
    const re = /([^\s=]+)\s*=\s*("([^"]*)"|'([^']*)')/g;
    let m;
    while ((m = re.exec(body.slice(name.length)))) attrs[m[1]] = decode(m[3] ?? m[4] ?? '');
    const el = new XmlElement(name, attrs);
    if (stack.length) stack[stack.length - 1].nodes.push(el);
    if (selfClose) { if (!stack.length) root = el; } else stack.push(el);
    if (root && !stack.length) break;
  }
  // XMPP over WebSocket: "<open …/>" comes whole; an unclosed top element (a stream start) still counts
  return root || stack[0] || null;
}

module.exports = { parseXml, XmlElement };
