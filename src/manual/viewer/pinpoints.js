// Workshop Manual, pinpoint tests: Ford's single-step view (one step at a time, Yes / No moves on, Back goes back) or
// every step at once; newer tests' steps fetched when opened; opening a test and every answer reported to Vehicle
// History the same way Ford's manual does, so warranty can see the test was run and the results.
'use strict';

// ---------- pinpoint tests: single-step view, and reporting to Vehicle History ----------
// Ford's single-step view: one step at a time, answering Yes/No moves to the next step, Back goes to
// the step before. Opening a test and every answer is reported to Vehicle History the same way Ford's
// manual does it, so warranty can see the test was run and what the results were.
const pptMode = () => (state.pptMode === 'all' ? 'all' : 'single');
const pptState = (d, name) => ((d.ppt ||= {})[name] ||= { open: false, step: '', history: [], answers: {} });
const stepLabel = (row) => {
  const t = row.getAttribute('data-ppttypeandstep') || '';
  const m = t.match(/^([A-Z]+[0-9]*)STEP(\d+)/i);
  if (m) return `${m[1]}${m[2]}`;
  const head = (row.querySelector('td.gray b')?.textContent || '').trim();
  return head.split(/\s+/)[0] || '';
};

// Answer rows (Yes / No …) become buttons; one that doesn't go on to another step ends the test with an action
function markAnswers(row) {
  row.querySelectorAll('b.stepresult').forEach((b) => {
    const tr = b.closest('tr');
    if (!tr) return;
    tr.classList.add('ppt-answer');
    if (!tr.querySelector('a[data-jump]')) tr.classList.add('ppt-outcome');
  });
}

function setupPinpoints(docEl, d) {
  const ppts = new Map();
  docEl.querySelectorAll('h3.ppt[data-ppt]').forEach((h) => {
    const name = h.getAttribute('data-ppt');
    const body = docEl.querySelector(`#PPT${CSS.escape(name)}`);
    if (!body) return;
    const rows = [...body.querySelectorAll('tr[data-content-type="step"]')].filter((r) => r.getAttribute('data-pptid') === name);
    const spa = h.getAttribute('data-spa') || '';
    if (!rows.length && !spa) return;
    const p = { name, title: h.textContent.trim(), h, body, rows, spa, legacy: h.hasAttribute('data-legacy'), intro: body.querySelector('tr[data-content-type="wrapper"]') };
    p.bar = el('div', { class: 'ppt-bar' });
    body.before(p.bar);
    h.classList.add('toggle');
    // every answer row becomes one big button
    for (const row of rows) markAnswers(row);
    ppts.set(name, p);
  });
  docEl._ppts = ppts;
  docEl.classList.toggle('has-ppt', ppts.size > 0);
  for (const p of ppts.values()) {
    renderPpt(docEl, d, p);
    if (p.spa && !p.rows.length && (pptState(d, p.name).open || pptMode() === 'all')) loadSpaSteps(docEl, d, p);
  }
  // A procedure that mentions pinpoint tests the viewer didn't recognise: note its layout for working it out
  if (!ppts.size && /PINPOINT TEST/i.test(docEl.textContent || '')) {
    const counts = ['span.isipppt', 'span.spappt', 'span[id^="Span"]', 'tr[data-content-type="step"]', '[data-spa]', 'h3.ppt']
      .map((q) => `${q}=${docEl.querySelectorAll(q).length}`).join(' ');
    api.log(`pinpoint tests not recognised in ${d.searchNumber}/${d.procUid}: ${counts}`);
  }
}

function renderPpt(docEl, d, p) {
  const st = pptState(d, p.name);
  const single = pptMode() === 'single';
  if (!p.rows.length) { // a newer test whose steps haven't been fetched yet
    const open = st.open || !single;
    p.h.classList.toggle('open', open);
    p.body.classList.toggle('hide', !open);
    p.bar.classList.toggle('hide', !open);
    if (open) p.bar.replaceChildren(el('span', { class: 'where', text: p.loading ? 'Loading the test steps…' : (p.loadError || 'No steps for this test') }),
      el('span', { class: `rec ${p.rec?.state || ''}`, text: p.rec?.text || '' }));
    return;
  }
  const current = p.rows.find((r) => r.getAttribute('data-pptstepid') === st.step) || p.rows[0];
  const open = !single || st.open;
  p.h.classList.toggle('open', open);
  p.body.classList.toggle('hide', !open);
  p.bar.classList.toggle('hide', !single || !open);
  const first = current === p.rows[0];
  if (p.intro) p.intro.classList.toggle('hide', single && !(first && !st.history.length));
  for (const r of p.rows) {
    r.classList.toggle('hide', single && r !== current);
    const answer = st.answers[r.getAttribute('data-pptstepid')];
    r.querySelectorAll('tr.ppt-answer').forEach((a) => {
      const b = a.querySelector('b.stepresult');
      a.classList.toggle('chosen', !!answer && b?.id === answer);
    });
  }
  if (!single || !open) return;
  const n = p.rows.indexOf(current) + 1;
  p.bar.replaceChildren(
    el('button', { class: 'ghost small', text: '‹ Back', disabled: st.history.length ? null : 'disabled', title: 'Go back to the step before',
      'data-ppt-action': 'back' }),
    el('span', { class: 'where', text: `Step ${stepLabel(current)} · ${n} of ${p.rows.length}` }),
    el('span', { class: `rec ${p.rec?.state || ''}`, text: p.rec?.text || (p.legacy ? "Ford only records the start of this kind of test, not each step" : 'Answers are recorded to Vehicle History') }),
    el('button', { class: 'ghost small', text: 'Start over', title: 'Back to the first step', 'data-ppt-action': 'restart' }));
}

async function report(p, d, payload, okText) {
  p.rec = { state: 'busy', text: 'Recording…' };
  renderPptBar(p, d);
  const res = await api.report({ procUid: d.procUid, searchNumber: d.searchNumber, pptName: p.name, title: p.title, ...payload });
  p.rec = res?.ok ? { state: 'ok', text: `✓ ${okText}` }
    : { state: 'bad', text: res?.needsLogin ? '⚠ Not recorded: sign in to PTS' : `⚠ Not recorded (${res?.message || 'error'})` };
  renderPptBar(p, d);
}
function renderPptBar(p, d) {
  const docEl = p.h.closest('.doc');
  if (docEl) renderPpt(docEl, d, p);
}

async function loadSpaSteps(docEl, d, p) {
  if (p.loading || p.rows.length) return;
  p.loading = true; p.loadError = '';
  renderPpt(docEl, d, p);
  const res = await api.spa({ file: p.spa, procUid: d.procUid });
  p.loading = false;
  if (res?.error) { p.loadError = res.needsLogin ? 'Sign in to PTS to load the steps' : `Couldn't load the steps (${res.message})`; renderPpt(docEl, d, p); return; }
  const html = String(res.html || '');
  const parsed = new DOMParser().parseFromString(`<table><tbody id="spa-rows">${stripStyles(html)}</tbody></table>`, 'text/html');
  const holder = parsed.getElementById('spa-rows') || parsed.body;
  if (!holder.querySelector('tr')) { // not table rows: put whatever came back in a wrapper
    const div = parsed.createElement('div'); div.innerHTML = stripStyles(html);
    cleanContent(div, res.base, d);
    p.body.prepend(...div.childNodes);
  } else {
    cleanContent(holder, res.base, d);
    let tbody = p.body.querySelector(':scope > table > tbody');
    if (!tbody) { const t = el('table', { class: 'grid' }); tbody = el('tbody'); t.append(tbody); p.body.append(t); }
    tbody.querySelectorAll(':scope > tr[data-content-type="step"]').forEach((r) => r.remove());
    tbody.append(...holder.childNodes);
  }
  p.rows = [...p.body.querySelectorAll('tr[data-content-type="step"]')].filter((r) => !r.getAttribute('data-pptid') || r.getAttribute('data-pptid') === p.name);
  p.rows.forEach(markAnswers);
  api.log(`test ${p.name} (${p.spa}): ${html.length} characters, ${p.rows.length} steps`);
  if (!p.rows.length) p.loadError = 'This test came back without steps';
  const st = pptState(d, p.name);
  if (p.rows.length && !p.rows.some((r) => r.getAttribute('data-pptstepid') === st.step)) st.step = p.rows[0].getAttribute('data-pptstepid');
  renderPpt(docEl, d, p);
}

function openPpt(docEl, d, p, { step, scroll = true } = {}) {
  const st = pptState(d, p.name);
  const wasOpen = st.open;
  st.open = true;
  if (step) st.step = step;
  else if (!st.step && p.rows.length) st.step = p.rows[0].getAttribute('data-pptstepid');
  save();
  renderPpt(docEl, d, p);
  if (p.spa && !p.rows.length) loadSpaSteps(docEl, d, p);
  if (!wasOpen) report(p, d, {}, 'Test start recorded to Vehicle History');
  if (scroll) p.h.scrollIntoView({ block: 'start' });
}

// Open a pinpoint test by Ford's id for it (a letter like "A", or an id like "G2198756p1"), e.g. from a
// Diagnostics search result or a DTC chart
function findPpt(docEl, id) {
  const ppts = docEl._ppts;
  if (!ppts || !id) return null;
  const want = String(id).replace(/^PPT/i, '');
  if (ppts.has(want)) return ppts.get(want);
  for (const p of ppts.values()) {
    if (p.spa === want || p.rows.some((r) => r.getAttribute('data-pptid') === want)) return p;
    if (p.name.toLowerCase() === want.toLowerCase()) return p;
  }
  // Ford writes the same test's id in a few ways (PC/ED especially): "2015_GASOLINE_s5p001_DH", "PPT_DH", "DH"…
  const lc = want.toLowerCase();
  const tail = lc.split(/[_:#|]/).filter(Boolean).pop() || lc;
  for (const p of ppts.values()) {
    const ids = [p.name, p.spa, p.h.id, p.body.id.replace(/^PPT/i, '')].filter(Boolean).map((x) => String(x).toLowerCase());
    if (ids.some((x) => x === tail || lc.endsWith(`_${x}`) || x.endsWith(`_${lc}`) || x.endsWith(`_${tail}`))) return p;
  }
  // or only by its name in the heading ("PINPOINT TEST DH: …")
  for (const p of ppts.values()) {
    const m = p.title.match(/PINPOINT TEST\s+([A-Z0-9]+)\s*:/i);
    if (m && (m[1].toLowerCase() === lc || m[1].toLowerCase() === tail)) return p;
  }
  return null;
}
function goToPpt(docEl, d, id) {
  const p = findPpt(docEl, id);
  if (p) { openPpt(docEl, d, p); p.h.classList.remove('flash'); void p.h.offsetWidth; p.h.classList.add('flash'); return true; }
  const target = docEl.querySelector(`#PPT${CSS.escape(String(id))}`) || docEl.querySelector(`#n-${CSS.escape(String(id))}`) || docEl.querySelector(`[name="${CSS.escape(String(id))}"]`);
  if (target) { target.scrollIntoView({ block: 'start' }); return true; }
  api.log(`pinpoint test ${id} not found in ${d.searchNumber}/${d.procUid}; tests here: ${[...(docEl._ppts?.values() || [])].slice(0, 12)
    .map((p) => `${p.name}${p.spa ? `(${p.spa})` : ''}`).join(', ') || 'none recognised'}`);
  return false;
}

// A jump to a pinpoint test or one of its steps (from the symptom chart, a link, or a reopened tab)
function pinpointJump(docEl, target) {
  if (pptMode() !== 'single' || !docEl._ppts) return false;
  const d = activeDoc();
  if (!d) return false;
  const next = target.nextElementSibling;
  const h = target.closest('h3.ppt[data-ppt]') || (next?.matches('h3.ppt[data-ppt]') ? next : null);
  if (h && docEl._ppts.has(h.getAttribute('data-ppt'))) { openPpt(docEl, d, docEl._ppts.get(h.getAttribute('data-ppt'))); return true; }
  const row = target.closest('tr[data-content-type="step"]');
  const p = row && docEl._ppts.get(row.getAttribute('data-pptid'));
  if (!p) return false;
  openPpt(docEl, d, p, { step: row.getAttribute('data-pptstepid'), scroll: false });
  p.bar.scrollIntoView({ block: 'start' });
  return true;
}

function stepInfo(row, answerRow, answer, mode) {
  const id = stepLabel(row);
  let title = (row.querySelector(`a[name="${CSS.escape(row.getAttribute('data-pptstepid') || '')}"]`)?.nextElementSibling?.textContent
    || row.querySelector('td.gray b')?.textContent || '').replace(/\s+/g, ' ').trim();
  if (id && title.startsWith(id)) title = title.slice(id.length).trim();
  const question = (row.querySelector('b[data-content-type="stepquestion"]')?.textContent || '').trim();
  let outcome = '';
  if (answerRow?.classList.contains('ppt-outcome')) {
    const cells = [...answerRow.children];
    outcome = (cells[cells.length - 1]?.textContent || '').replace(/\s+/g, ' ').trim();
  }
  return { id, title, question, answer, outcome, mode };
}

function pinpointClick(docEl, d, e) {
  const ppts = docEl._ppts;
  if (!ppts || !ppts.size) return false;
  const single = pptMode() === 'single';
  // the test's title opens/closes it
  const h = e.target.closest('h3.ppt.toggle');
  if (h && single) {
    const p = ppts.get(h.getAttribute('data-ppt'));
    const st = pptState(d, p.name);
    if (st.open) { st.open = false; save(); renderPpt(docEl, d, p); } else openPpt(docEl, d, p, { scroll: false });
    return true;
  }
  // Back / Start over
  const btn = e.target.closest('[data-ppt-action]');
  if (btn) {
    const p = [...ppts.values()].find((x) => x.bar.contains(btn));
    const st = p && pptState(d, p.name);
    if (!p) return true;
    if (btn.getAttribute('data-ppt-action') === 'back' && st.history.length) st.step = st.history.pop().step;
    if (btn.getAttribute('data-ppt-action') === 'restart') { st.history = []; st.answers = {}; st.step = p.rows[0].getAttribute('data-pptstepid'); }
    save(); renderPpt(docEl, d, p);
    p.bar.scrollIntoView({ block: 'nearest' });
    return true;
  }
  // an answer (Yes / No …)
  const answerRow = e.target.closest('tr.ppt-answer');
  const row = answerRow?.closest('tr[data-content-type="step"]');
  const p = row && ppts.get(row.getAttribute('data-pptid'));
  if (!p) return false;
  const answer = answerRow.querySelector('b.stepresult')?.id || '';
  const isOutcome = answerRow.classList.contains('ppt-outcome');
  // "GO to A2" answers move on by themselves; for an action ("INSTALL… REFER to: …") only a link you click opens
  const clicked = e.target.closest('a[data-jump], a[data-open], a[data-url]');
  const jumps = answerRow.querySelectorAll('a[data-jump]');
  // one "GO to" goes there; with a choice ("…built before May 12, GO to A3 / after, GO to A4") you click the one that applies
  const link = isOutcome ? clicked : (clicked || (jumps.length === 1 ? jumps[0] : null) || answerRow.querySelector('a[data-open]'));
  const choose = !isOutcome && !link && jumps.length > 1;
  if (!single) {
    // all-steps view: Ford still records the step when you follow its link
    if (link && !p.legacy) report(p, d, { step: stepInfo(row, answerRow, answer, 'MULTI') }, `${stepLabel(row)} ${answer} recorded`);
    return false; // the normal link handling carries on
  }
  e.preventDefault();
  const st = pptState(d, p.name);
  const stepId = row.getAttribute('data-pptstepid');
  const again = st.answers[stepId] === answer && (isOutcome || st.choosing === stepId);
  st.answers[stepId] = answer;
  st.choosing = choose ? stepId : '';
  if (p.legacy) {
    p.rec = choose ? { state: '', text: 'Click the step that applies to this vehicle' } : null;
  } else if (!again) report(p, d, { step: stepInfo(row, answerRow, answer, 'SINGLE') },
    choose ? `${stepLabel(row)}: ${answer} recorded. Now click the step that applies to this vehicle` : `${stepLabel(row)}: ${answer} recorded to Vehicle History`);
  const hash = link?.dataset.jump;
  const next = hash && p.rows.find((r) => r.getAttribute('data-pptstepid') === hash);
  if (next) {
    st.history.push({ step: stepId, answer });
    st.step = hash;
    save(); renderPpt(docEl, d, p);
    p.bar.scrollIntoView({ block: 'start' });
  } else if (hash) {
    save(); renderPpt(docEl, d, p);
    jumpTo(docEl, hash, true); // another pinpoint test on this page
  } else if (link?.dataset.open) {
    save(); renderPpt(docEl, d, p);
    const target = JSON.parse(link.dataset.open);
    openDoc({ ...target, section: '' }, { newTab: true, hash: target.hash });
  } else if (link?.dataset.url) {
    save(); renderPpt(docEl, d, p);
    api.openUrl(link.dataset.url);
  } else if (choose) {
    save(); renderPpt(docEl, d, p);
  } else {
    save(); renderPpt(docEl, d, p); // an action to do (INSTALL…, REPAIR…): the test ends here
  }
  return true;
}

$('btn-expand').onclick = () => {
  state.pptMode = pptMode() === 'single' ? 'all' : 'single';
  save();
  const node = $('content').querySelector('.doc');
  const d = activeDoc();
  if (node?._ppts && d) for (const p of node._ppts.values()) {
    renderPpt(node, d, p);
    if (p.spa && !p.rows.length && pptMode() === 'all') loadSpaSteps(node, d, p);
  }
  updateModeButton(node);
};
function updateModeButton(node) {
  const has = !!node?._ppts?.size;
  $('btn-expand').hidden = !has;
  $('btn-expand').textContent = pptMode() === 'single' ? 'Show all steps' : 'Single-step view';
  $('btn-expand').title = pptMode() === 'single'
    ? 'Show every step at once (Ford only treats single-step runs as a recorded test)'
    : "Back to Ford's single-step view, where each answer is recorded to Vehicle History";
}
