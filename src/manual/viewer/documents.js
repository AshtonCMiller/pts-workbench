// Workshop Manual, open procedures: each one a tab (a plain click replaces the open one, Ctrl+click adds a tab),
// showing one (fetched from Ford once, kept while the tab is open), and the welcome and problem screens.
'use strict';

// ---------- documents ----------
function openDoc(link, { newTab = false, hash = link.hash } = {}) {
  const key = docKey(link);
  let d = state.docs.find((x) => x.key === key);
  if (!d) {
    d = { key, title: link.title || 'Procedure', section: link.section || '', searchNumber: link.searchNumber,
          procUid: link.procUid || '', linkType: link.linkType || 'Procedure', scroll: 0, hash: '' };
    const cur = state.docs.findIndex((x) => x.key === state.active);
    // A plain click replaces the open procedure (like a browser); Ctrl+click or links inside a procedure add a tab
    if (!newTab && cur >= 0 && !state.docs[cur].pinned) state.docs.splice(cur, 1, d);
    else state.docs.splice(cur + 1, 0, d);
  }
  if (hash) { d.hash = hash; d.scroll = 0; }
  if (link.ppt) { d.goPpt = String(link.ppt); d.scroll = 0; }
  rememberScroll();
  state.active = key;
  save();
  renderTabs();
  showDoc();
  if (!state.query) renderTree();
}

function closeDoc(key) {
  const i = state.docs.findIndex((d) => d.key === key);
  if (i < 0) return;
  state.docs.splice(i, 1);
  docCache.delete(key);
  backStacks.delete(key);
  if (state.active === key) state.active = (state.docs[i] || state.docs[i - 1])?.key || null;
  save();
  renderTabs();
  showDoc();
  if (!state.query) renderTree();
}

function renderTabs() {
  $('tabs').replaceChildren(...state.docs.map((d) => el('div', {
    class: `dtab${d.key === state.active ? ' on' : ''}`, title: [d.section, d.title].filter(Boolean).join(' › '),
    onclick: () => { if (state.active !== d.key) { rememberScroll(); state.active = d.key; save(); renderTabs(); showDoc(); if (!state.query) renderTree(); } },
    onauxclick: (e) => { if (e.button === 1) closeDoc(d.key); },
  }, el('span', { class: 't', text: d.title }), el('button', { class: 'x', title: 'Close', text: '×', onclick: (e) => { e.stopPropagation(); closeDoc(d.key); } }))));
  const on = $('tabs').querySelector('.dtab.on');
  if (on) on.scrollIntoView({ block: 'nearest', inline: 'nearest' });
}

function rememberScroll() {
  const d = activeDoc();
  if (d && docCache.has(d.key) && $('content').firstChild === docCache.get(d.key)) d.scroll = $('content').scrollTop;
}

let showToken = 0;
async function showDoc() {
  const d = activeDoc();
  const token = ++showToken;
  $('doc-bar').hidden = !d;
  if (!d) { if (tree) showWelcome(); else $('content').replaceChildren(); return; }
  $('doc-where').textContent = [d.section, d.title].filter(Boolean).join(' › ');
  updateBack();
  let node = docCache.get(d.key);
  if (!node) {
    $('content').replaceChildren(el('div', { class: 'state' }, el('div', { class: 'spinner' }), `Loading ${d.title}…`));
    const res = await api.proc({ searchNumber: d.searchNumber, procUid: d.procUid, linkType: d.linkType });
    if (token !== showToken) return;
    if (res?.error) return showProblem(res, showDoc);
    try {
      node = renderProcedure(res.html, res.base, d);
    } catch (e) {
      api.log(`procedure ${d.searchNumber}/${d.procUid}: ${e && e.stack || e}`);
      return showProblem({ message: `Couldn't show this procedure (${e?.message || e})` }, showDoc);
    }
    if (!(node.textContent || '').replace(d.title, '').trim()) {
      api.log(`procedure ${d.searchNumber}/${d.procUid} came back empty: ${String(res.html).slice(0, 300)}`);
    }
    docCache.set(d.key, node);
    if (d.title === 'Procedure' && node.dataset.title) { d.title = node.dataset.title; renderTabs(); save(); }
  }
  $('content').replaceChildren(node);
  if (node._ppts) for (const p of node._ppts.values()) renderPpt(node, d, p); // the view may have changed in another procedure
  updateModeButton(node);
  if (d.goPpt) {
    const id = d.goPpt; d.goPpt = ''; save();
    requestAnimationFrame(() => goToPpt(node, d, id));
  } else if (d.hash) {
    const h = d.hash; d.hash = ''; save();
    requestAnimationFrame(() => jumpTo(node, h, false));
  } else {
    $('content').scrollTop = d.scroll || 0;
  }
}

function showWelcome() {
  $('doc-bar').hidden = true;
  $('content').replaceChildren(el('div', { class: 'state' },
    el('h2', { text: 'Pick a procedure' }),
    el('p', { text: 'Open a section on the left, or search the contents. Ctrl+click opens a procedure in a new tab; links inside a procedure always do.' })));
}

function showProblem(res, retry) {
  $('doc-bar').hidden = true;
  const actions = el('div', { class: 'actions' });
  if (res.needsLogin) {
    actions.append(el('button', { class: 'primary', text: 'Sign in to PTS', onclick: async (e) => {
      e.target.disabled = true; e.target.textContent = 'Waiting for sign-in…';
      await api.signIn();
      retry();
    } }));
  } else {
    actions.append(el('button', { class: 'primary', text: 'Try again', onclick: () => retry() }));
  }
  actions.append(el('button', { class: 'ghost', text: 'Open in PTS instead', onclick: () => api.openPts() }));
  if (!res.needsLogin) actions.append(el('button', { class: 'ghost', text: 'Open log', title: 'What the app asked Ford for, and what came back', onclick: () => api.openLog() }));
  $('content').replaceChildren(el('div', { class: 'state' },
    el('h2', { text: res.needsLogin ? 'Sign in to PTS' : "Couldn't load this" }),
    el('p', { text: res.needsLogin ? 'The manual needs your PTS login. Sign in once and it opens straight away from then on.' : (res.message || 'Something went wrong talking to Ford.') }),
    res.detail ? el('p', { class: 'detail', text: res.detail }) : null,
    actions));
}
