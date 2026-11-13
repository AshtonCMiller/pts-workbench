// Labor Times, reading SLTS's report: one long table, read into operations (code, title, description, notes, overlaps)
// with their options (one per engine, location…), and the combinations and supplements that add on to them.
'use strict';

// ---------- reading SLTS's report ----------
// SLTS's report is one long table. Each operation starts with a row holding <a name="CODE"> and its
// title; then come its description, notes, overlaps and (when it has them) one row per option
// ("2.0L GTDI", "...FROM CONDENSER"). "Combinations" and "Supplements" rows start add-on operations.
const isBase = (code) => /^[0-9A-Z]*\d[A-Z]$/.test(code); // 19700A, 19700D: base operations
function codeOf(td) { return clean(td?.textContent).replace(/\s+/g, '').toUpperCase(); }
function cartOf(td) {
  const a = td?.querySelector('a[href*="discardPartList"]');
  const m = a && a.getAttribute('href').match(/discardPartList\(([\s\S]*)\)/);
  if (!m) return null;
  const args = [...m[1].matchAll(/'((?:[^'\\]|\\.)*)'/g)].map((x) => x[1].replace(/&quot;/g, '"').replace(/&amp;/g, '&'));
  const [opNumBase, opNum, partNo, qty, desc, qual] = args;
  return { opNumBase: opNumBase || '', opNum: opNum || '', partNo: partNo || '', qty: qty || '', desc: desc || '', qual: qual || '' };
}
function parseOverlaps(row) {
  return [...row.querySelectorAll('token')].map((t) => clean(t.textContent)).filter(Boolean);
}
// "When claiming 19700A4: Do not use with; 6007A1, 6007A34*, …" → rules
function overlapRules(lines) {
  const rules = [];
  for (const line of lines) {
    const m = line.match(/When claiming\s+([^:]+):\s*Do not use with;?\s*(.+)$/i);
    if (!m) continue;
    let list = m[2], cond = '';
    const i = list.search(/\s+For vehicles/i);
    if (i >= 0) { cond = list.slice(i).trim(); list = list.slice(0, i); }
    const not = list.split(',').map((x) => x.trim().replace(/\.+$/, '').toUpperCase()).filter((x) => /^[0-9A-Z*]+$/.test(x));
    rules.push({ subject: m[1].trim().toUpperCase(), not, cond });
  }
  return rules;
}
const matches = (pattern, code) => (pattern.endsWith('*') ? code.startsWith(pattern.slice(0, -1)) : code === pattern);

// 5212BF → 5212B, 18125AT → 18125A, 5649ARB → 5649A: the operation an option belongs to
const opIdOf = (code) => (String(code || '').match(/^[0-9A-Z]*?\d[A-Z]/) || [code || ''])[0];
// Row kinds in SLTS's report
const isOptionRow = (r) => r.cells[1]?.getAttribute('align') === 'right' && !!codeOf(r.cells[3]);
const opAnchor = (r) => [...r.querySelectorAll('a[name]')].find((a) => !/^(top|bottom)$/i.test(a.getAttribute('name')));
// An operation's title: bold name in a small table with SLTS's ▲▼ (top / bottom) links, or its own <a name>
const isTitleRow = (r) => !isOptionRow(r) && !!(opAnchor(r) || (r.cells[1]?.querySelector('table b') && r.cells[1].querySelector('a[href$="#top"], a[href$="#bottom"]')));
// The first option row after an operation's title row (before the next title)
function nextOptionRow(row) {
  for (let r = row.nextElementSibling; r; r = r.nextElementSibling) {
    if (isTitleRow(r)) return null;
    if (isOptionRow(r)) return r;
  }
  return null;
}
function parseReport(doc) {
  const xml = doc.getElementById('TableRow_XmlContent');
  if (!xml) return null;
  // A section can come back as several report tables (one per group of operations): read them all, in order
  // The report table: rows of six cells (gap, description, gap, operation number, gap, time). Keep only
  // the innermost such tables, so a wrapper table's rows aren't read twice.
  const opRow = (r) => r.cells.length === 6 && r.cells[3].getAttribute('width') === '100';
  let tables = [...xml.querySelectorAll('table')].filter((t) => [...t.rows].some(opRow));
  tables = tables.filter((t) => !tables.some((o) => o !== t && t.contains(o)));
  if (!tables.length) return null;
  const title = clean(xml.querySelector('td.copysubtitle')?.textContent);
  const ops = [];
  let cur = null, base = null, mode = 'base', suppParent = null, group = '';
  for (const row of tables.flatMap((t) => [...t.rows])) {
    if (!opRow(row)) continue;
    const tds = row.cells;
    if (tds.length < 2) continue;
    const text = clean(row.textContent);
    if (!text || /^Operation Description\s+Operation Number\s+Time$/i.test(text)) continue;
    // A section list puts a coloured group title ("DIAGNOSTIC OPERATIONS", "ALL OPERATIONS - …") between operations
    if (tds[1] && !tds[1].querySelector('table') && tds[1].querySelector('b font') && !codeOf(tds[3])) {
      group = clean(tds[1].textContent); cur = null; base = null; mode = 'base'; suppParent = null;
      continue;
    }
    // An operation's title row: its name in bold (with part numbers) in a small table of its own. It carries
    // <a name="CODE"> only when the operation has a single time; otherwise the code comes from its options.
    if (codeOf(tds[3]) === 'OPERATIONNUMBER') continue; // the column headings
    if (isTitleRow(row)) {
      const anchor = opAnchor(row);
      let code = anchor ? anchor.getAttribute('name').toUpperCase() : '';
      if (!code) code = opIdOf(codeOf(nextOptionRow(row)?.cells[3]));
      const b = tds[1]?.querySelector('b');
      const parts = [...(b?.querySelectorAll('a[href*="partlink"]') || [])].map((a) => clean(a.textContent));
      const name = clean(b?.textContent).replace(/\(\s*[^)]*\)\s*/g, (m) => (parts.some((p) => m.includes(p)) ? '' : m)).replace(/\s+-\s+/, ' - ').trim();
      let kind = 'base', parent = null;
      if (!code || isBase(code)) { mode = 'base'; suppParent = null; }
      else if (mode === 'supp' && suppParent && code.startsWith(suppParent.code)) { kind = 'supp'; parent = suppParent; }
      else if (mode === 'supp') { mode = 'combo'; }
      if (kind !== 'supp' && mode === 'combo' && base) { kind = 'combo'; parent = base; }
      cur = { code, name: name || code, parts, kind, parentCode: parent?.code || '', group, desc: [], notes: [], overlaps: [], options: [] };
      if (kind === 'base') base = cur;
      ops.push(cur);
      const optCode = codeOf(tds[3]);
      if (optCode) cur.options.push({ code: optCode, qual: '', time: clean(tds[5]?.textContent), cart: cartOf(tds[3]) });
      continue;
    }
    if (/^Combinations/i.test(text) && row.querySelector('b u, u b')) { mode = 'combo'; continue; }
    if (/^Supplements\b/i.test(text) && text.length < 40) { mode = 'supp'; suppParent = cur; continue; }
    if (row.querySelector('token')) { if (cur) cur.overlaps.push(...parseOverlaps(row)); continue; }
    if (tds[1]?.getAttribute('align') === 'right' && codeOf(tds[3])) {
      if (cur) cur.options.push({ code: codeOf(tds[3]), qual: clean(tds[1].textContent), time: clean(tds[5]?.textContent), cart: cartOf(tds[3]) });
      continue;
    }
    const blue = row.querySelector('font[color] a, a.hyperlink');
    if (blue && cur) {
      const href = blue.getAttribute('href') || '';
      const url = (href.match(/window\.open\('([^']+)'/) || [])[1] || '';
      cur.notes.push({ text: clean(blue.textContent), url });
      continue;
    }
    const descTd = tds[1];
    const t = clean(descTd?.textContent);
    if (!t) continue;
    if (cur) cur.desc.push(t);
    else group = t;
  }
  for (const op of ops) op.rules = overlapRules(op.overlaps);
  return { title, ops };
}
