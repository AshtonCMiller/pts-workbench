// Workshop Manual, "Only this vehicle": Ford marks the contents' sections and procedures with the options they're for;
// this works out which apply to this vehicle (its option codes from PTS) the same way Ford's own manual page does,
// and builds the contents as shown (this vehicle's only, or everything with the others grayed).
'use strict';

// ---------- only what applies to this vehicle ----------
// Ford marks the contents' sections and procedures with the options they're for (data-qual="#FC~ene3|enr9#SD~20220101":
// every "#" group must match; in an FC group any one option code will do; SD / ED: built on / after, on / before).
// The book lists the option codes it uses (data-feature-pfcs / -mfcs); the vehicle's own come from PTS's VIN decode.
// The same rules as Ford's own manual page (book.js applyFilter).
function vehicleFit(html, features) {
  const why = (t) => { api.log(`contents: not filtered — ${t}`); return null; };
  if (!features || !(features.primary?.length || features.minor?.length)) return why("the vehicle's options aren't known");
  // read from the page itself (Ford's quoting of these attributes varies)
  const doc = new DOMParser().parseFromString(stripStyles(html), 'text/html');
  const holder = doc.querySelector('[data-feature-vl], [data-feature-pfcs]');
  const marked = [...doc.querySelectorAll('[data-qual]')];
  if (!marked.length) return why(`Ford's contents don't mark options for this book (${holder ? 'has' : 'no'} vehicle line)`);
  const list = (v) => String(v || '').toUpperCase().split(/[|,\s]+/).filter(Boolean);
  // the options the book uses: as Ford's page lists them, or else every code its marks name
  let pfcs = list(holder?.getAttribute('data-feature-pfcs')), mfcs = list(holder?.getAttribute('data-feature-mfcs'));
  if (!pfcs.length && !mfcs.length) {
    const used = new Set();
    for (const n of marked) for (const g of String(n.getAttribute('data-qual')).split('#')) if (/^FC~/i.test(g)) list(g.slice(3).replace(/\|/g, ' ')).forEach((c) => used.add(c));
    pfcs = [...used].filter((c) => c.length <= 4); mfcs = [...used].filter((c) => c.length > 4);
    api.log(`contents: book's option list missing; using the ${used.size} codes its ${marked.length} marks name, e.g. ${marked.slice(0, 3).map((n) => n.getAttribute('data-qual')).join(' ')}`);
  }
  const primary = new Set(features.primary || []);
  const minor = new Set((features.minor || []).map((x) => x.slice(0, 5)));
  const matches = new Set([...pfcs.filter((c) => primary.has(c)), ...mfcs.filter((c) => minor.has(c))]);
  // none of the book's main options (engine, transmission…) found in the vehicle's: the codes don't line up, so nothing is hidden
  if (!pfcs.some((c) => matches.has(c))) {
    api.log(`contents: not filtered — none of the book's options (${pfcs.slice(0, 12).join(',')}) are in the vehicle's (${[...primary].slice(0, 20).join(',')})`);
    return null;
  }
  // the vehicle's minor options unknown: rules about them are let through rather than hiding everything
  return { matches: [...matches], set: matches, build: parseInt(features.buildDate, 10) || 0, codes: pfcs.length + mfcs.length,
    mfcs: new Set(mfcs), minorKnown: minor.size > 0 };
}
function qualFits(qual, fit) {
  if (!fit || !qual) return true;
  const groups = String(qual).split('#').slice(1).filter(Boolean);
  if (!groups.length) return true;
  return groups.every((g) => {
    const [kind, rest = ''] = g.toUpperCase().split('~');
    const quals = rest.split('|');
    if (kind === 'FC') return quals.some((q) => fit.set.has(q) || (!fit.minorKnown && fit.mfcs.has(q)));
    if (kind === 'SD') return !fit.build || parseInt(quals[0], 10) <= fit.build;
    if (kind === 'ED') return !fit.build || parseInt(quals[0], 10) >= fit.build;
    return true; // a kind of rule Ford's page doesn't know either
  });
}
function countLeaves(nodes, test) {
  let n = 0;
  for (const x of nodes || []) n += x.type === 'folder' ? countLeaves(x.children, test) : (test(x) ? 1 : 0) + countLeaves(x.children, test);
  return n;
}
// The contents as shown: this vehicle's only (unless "Show all" is on), with paths, counts and the search list
function buildShown() {
  const showAll = !filterable || state.allOptions;
  const copy = (nodes) => {
    const out = [];
    for (const n of nodes) {
      if (n.type === 'folder') {
        const children = copy(n.children);
        if (!children.length) continue;
        out.push({ ...n, children, na: n.na || children.every((c) => c.na) });
      } else {
        // a procedure with parts listed under it (PC/ED "Quick Test Description" › Quick Test, KOEO…)
        const children = n.children ? copy(n.children) : [];
        if (showAll || !n.na || children.length) out.push({ ...n, children: children.length ? children : undefined });
      }
    }
    return out;
  };
  tree = copy(fullTree || []);
  leaves = [];
  const walk = (nodes, pathTitles) => {
    for (const n of nodes) {
      n.path = [...pathTitles, n.title];
      if (n.type === 'folder') walk(n.children, n.path);
      else { leaves.push(n); if (n.children) walk(n.children, n.path); }
    }
  };
  walk(tree, []);
  const count = (n) => (n.type === 'folder' ? (n.count = n.children.reduce((a, c) => a + count(c), 0))
    : 1 + (n.children ? (n.count = n.children.reduce((a, c) => a + count(c), 0)) : 0));
  tree.forEach(count);
}
function drawFilter() {
  const total = countLeaves(fullTree, () => true), na = filterable ? countLeaves(fullTree, (n) => n.na) : 0;
  const c = $('book-count');
  if (c) c.textContent = ` · ${leaves.length.toLocaleString()} entries`;
  const box = $('vin-filter');
  box.hidden = !filterable || !na;
  if (box.hidden) return;
  $('vin-only').checked = !state.allOptions;
  $('vin-note').textContent = state.allOptions ? `gray ones (${na.toLocaleString()}) don't apply` : `${na.toLocaleString()} of ${total.toLocaleString()} hidden`;
}
