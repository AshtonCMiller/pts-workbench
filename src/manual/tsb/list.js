// TSBs, the list: Ford's list of articles for this VIN read into { type, id, title, date, summary, url, categories },
// Ford's categories (with counts) for the drop-down, and the list as shown with the filter box and category.
'use strict';

// ---------- Ford's list ----------
let articles = [];   // { type, id, title, date, summary, url, cats:Set }
let cats = [];       // { id, label, depth, count }
function readList(html) {
  const doc = parse(html);
  articles = [];
  for (const li of doc.querySelectorAll('#articles li, ul.filterThis li')) {
    const a = li.querySelector('.article a, a.small');
    if (!a) continue;
    const href = a.getAttribute('href') || '';
    const hash = href.includes('#') ? href.slice(href.indexOf('#') + 1) : href;
    const m = /DisplayArticle\.aspx/i.test(hash) ? hash : (a.getAttribute('onclick') || '').match(/DisplayArticle\.aspx[^'"]*/i)?.[0];
    if (!m) continue;
    const url = new URL(m.replace(/&amp;/g, '&'), TSB_BASE).href;
    const sp = new URL(url).searchParams;
    const label = clean(a.textContent);                          // "TSB 26-2414"
    const classes = (li.getAttribute('class') || '').split(/\s+/).filter(Boolean);
    const letter = classes.find((c) => /^[TGSI]$/.test(c)) || (label.match(/^(TSB|GSB|SSM|ISB)/i)?.[1] || 'T').charAt(0).toUpperCase();
    const after = clean(li.querySelector('.article')?.textContent || '').replace(label, '');
    articles.push({
      type: TYPES[letter] || (sp.get('type') || 'TSB').toUpperCase(),
      id: sp.get('id') || label.replace(/^\S+\s+/, ''),
      title: label,
      date: (after.match(/Effective\s+(.+)$/i) || [])[1] || '',
      summary: clean(li.querySelector('.summary')?.textContent) || clean(sp.get('t')),
      url,
      cats: new Set(classes),
    });
  }
  // Ford's category menu: nested lists of <a id="CATEGORY"><div class="desc">…</div><div class="acnt">n</div></a>
  cats = [];
  const root = doc.querySelector('#container > ul, #symptoms ul');
  const walk = (ul, depth) => {
    for (const li of ul.children) {
      if (li.tagName !== 'LI') continue;
      const a = li.querySelector(':scope > a');
      const id = a?.getAttribute('id');
      const label = clean(a?.querySelector('.desc')?.textContent || a?.textContent);
      if (id && id !== 'x' && label) {
        const count = Number(clean(a.querySelector('.acnt')?.textContent)) || articles.filter((x) => x.cats.has(id)).length;
        if (count) cats.push({ id, label, depth, count });
      }
      const sub = li.querySelector(':scope > ul');
      if (sub) walk(sub, depth + 1);
    }
  };
  if (root) walk(root, 0);
  api.log(`tsb: ${articles.length} articles, ${cats.length} categories`);
}

function renderList() {
  // categories
  const sel = $('t-cat');
  sel.replaceChildren(el('option', { value: '', text: 'All categories' }),
    ...cats.map((c) => el('option', { value: c.id, text: `${'  '.repeat(c.depth)}${c.depth ? '› ' : ''}${c.label} (${c.count})` })));
  sel.value = cats.some((c) => c.id === state.cat) ? state.cat : '';
  sel.hidden = !cats.length;
  // type tabs with counts (for what the category and filter leave)
  const words = state.q.toLowerCase().split(/\s+/).filter(Boolean);
  const base = articles.filter((a) => (!sel.value || a.cats.has(sel.value))
    && words.every((w) => `${a.title} ${a.id} ${a.summary} ${[...a.cats].join(' ')}`.toLowerCase().includes(w)));
  const counts = { '': base.length };
  for (const a of base) counts[a.type] = (counts[a.type] || 0) + 1;
  const types = ['', ...['TSB', 'GSB', 'SSM', 'ISB'].filter((t) => articles.some((a) => a.type === t))];
  if (state.type && !types.includes(state.type)) state.type = '';
  $('t-types').replaceChildren(...types.map((t) => el('button', {
    class: `d-tab${state.type === t ? ' on' : ''}${counts[t] ? '' : ' empty'}`, type: 'button',
    onclick: () => { state.type = t; save(); renderList(); },
  }, t ? `${t}s` : 'All', el('span', { class: 'n', text: String(counts[t] || 0) }))));
  const shown = base.filter((a) => !state.type || a.type === state.type);
  $('t-body').replaceChildren(
    shown.length ? el('div', { class: 't-count', text: `${shown.length} of ${articles.length}, newest first` }) : null,
    shown.length
      ? el('div', { class: 'd-list' }, ...shown.map((a) => el('button', { class: 'd-item t-item', type: 'button', onclick: () => openArticle(a) },
        el('div', { class: 't' }, el('span', { class: `kind ${a.type.toLowerCase()}`, text: a.type }), el('span', { class: 'num', text: a.id }),
          el('span', { class: 'date', text: a.date })),
        el('div', { class: 'desc', text: a.summary }))))
      : el('div', { class: 'd-empty', text: articles.length ? 'Nothing matches that.' : 'Ford lists no TSBs, GSBs or SSMs for this VIN.' }));
}
