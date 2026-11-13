// OASIS, field service actions (recalls): read from Ford's FSA panel, each with its documents; FSAs on Ford's software
// verification form get a "Retrieve SVC" button (the software validation code, fetched with the dealer's WDC).
'use strict';

// ---------- field service actions (recalls) ----------
function readFsa(doc) {
  const panel = doc.getElementById('fsa-panel');
  const out = { found: !!panel, title: headerText(doc, 'fsa-panel'), items: [], links: [] };
  if (!panel) return out;
  panel.querySelectorAll('li').forEach((li) => {
    const a = li.querySelector('a[data-fsanum], a');
    const num = clean(a?.getAttribute('data-fsanum') || a?.textContent);
    const desc = clean([...li.querySelectorAll('label')].map((l) => l.textContent).join(' ')) || clean(li.textContent).replace(num, '').trim();
    if (num || desc) out.items.push({ num, desc, url: a?.getAttribute('href') || '' });
  });
  // Ford's notices above the list (software repair steps, the list of recalls the process supports)
  panel.querySelectorAll(':scope .row a[href]').forEach((a) => {
    if (a.closest('li')) return;
    const text = clean(a.closest('label')?.textContent || a.textContent);
    if (text) out.links.push({ text: text.replace(/^NOTICE\s*[–-]\s*/i, ''), url: a.getAttribute('href'), label: clean(a.textContent) });
  });
  return out;
}
function renderFsa(fsa) {
  const box = el('section', { class: 'o-section', id: 'fsa' }, el('h2', { text: 'Field service actions (recalls)' }));
  if (!fsa.found) return box.appendChild(el('div', { class: 'o-card o-msg', text: "OASIS didn't include field service actions for this VIN." })), box;
  if (!fsa.items.length) {
    box.append(el('div', { class: 'o-card o-msg ok' }, el('b', { text: '✓ ' }), /no\b/i.test(fsa.title) ? nice(fsa.title) : 'No open field service actions for this VIN.'));
    return box;
  }
  const rows = fsa.items.map((f) => el('div', { class: 'o-fsa' },
    f.url ? el('button', { class: 'o-fsa-num', text: f.num, title: `Open ${f.num}`, onclick: () => api.openArticle(f.url, `FSA ${f.num}`) })
      : el('span', { class: 'o-fsa-num', text: f.num }),
    el('span', { class: 'o-fsa-desc', text: f.desc }),
    el('span', { class: 'o-svc', 'data-fsa': f.num }),
    el('button', { class: 'ghost small', text: 'Copy FSA', title: `Copy ${f.num}`, onclick: () => copy(f.num) })));
  const notes = fsa.links.map((l) => el('div', { class: 'o-note' }, el('button', { class: 'link', text: l.text, onclick: () => api.openArticle(l.url, l.label || 'Notice') })));
  box.append(el('div', { class: 'o-card o-fsas' }, ...rows), ...(notes.length ? [el('div', { class: 'o-notes' }, ...notes)] : []));
  return box;
}

// ---------- software validation codes (SVC) ----------
// Only FSAs on Ford's software verification form get a "Retrieve SVC" button
let svcInfo = null; // { fsas, jv, saved, dealerCode }
const svcState = new Map(); // FSA → { status, code, message, … } for this page
const svcSlot = (num) => document.querySelector(`.o-svc[data-fsa="${CSS.escape(num)}"]`);

async function loadSvc() {
  if (!document.querySelector('.o-svc')) return;
  let res;
  try { res = await api.svcList(); } catch (e) { res = { error: true, message: String(e?.message || e) }; }
  if (res?.error) { api.log(`oasis: SVC list: ${res.message || 'failed'}`); return; }
  svcInfo = res;
  for (const [num, v] of Object.entries(res.saved || {})) if (v?.code && !svcState.has(num)) svcState.set(num, { status: 'ok', code: v.code, saved: v.at });
  for (const f of res.fsas || []) renderSvc(f.num);
}

function renderSvc(num) {
  const slot = svcSlot(num);
  const f = svcInfo?.fsas?.find((x) => x.num === num);
  if (!slot || !f) return;
  const st = svcState.get(num) || {};
  const retrieve = (label = 'Retrieve SVC', cls = 'primary small') =>
    el('button', { class: cls, text: label, title: 'Ask Ford for the software validation code', onclick: () => getSvc(num) });
  const some = f.some ? el('span', { class: 'o-svc-note', title: 'Some vehicles in this FSA don’t need a software update or SVC. Check the FSA instructions.', text: '**' }) : null;
  let kids;
  if (st.status === 'loading') kids = [el('span', { class: 'o-svc-wait' }, el('span', { class: 'spinner' }), 'Checking…')];
  else if (st.status === 'ok') {
    kids = [el('span', { class: 'o-svc-code', title: 'Software validation code. Click to copy', text: st.code, onclick: () => copy(st.code) }),
      el('button', { class: 'primary small', text: 'Copy SVC', onclick: () => copy(st.code) }),
      el('button', { class: 'ghost small', text: '↻', title: 'Ask Ford again', onclick: () => getSvc(num) })];
  } else if (st.status === 'incomplete') {
    kids = [el('span', { class: 'o-svc-bad', title: [st.modules && `Modules: ${st.modules}`, st.detail && `Status: ${st.detail}`].filter(Boolean).join('\n') || 'The software updates for this FSA aren’t finished yet',
      text: 'Conditions not met' }), retrieve('Try again', 'ghost small')];
  } else if (st.status === 'needs-dealer') {
    const input = el('input', { class: 'o-odo upper', placeholder: 'USA12345', maxlength: '12', title: 'Warranty Dealer Code (WDC)' });
    const save = async () => {
      const code = input.value.trim();
      if (!code) { input.focus(); return; }
      if (await api.setDealerCode(code)) { svcState.delete(num); getSvc(num); } else toast('That dealer code doesn’t look right');
    };
    input.addEventListener('keydown', (e) => { if (e.key === 'Enter') save(); });
    kids = [el('span', { class: 'o-svc-ask', text: 'Your Warranty Dealer Code:' }), input, el('button', { class: 'primary small', text: 'Save', onclick: save })];
    setTimeout(() => input.focus(), 0);
  } else if (st.status === 'error') {
    kids = [el('span', { class: 'o-svc-err', title: st.message || '', text: st.needsLogin ? 'Sign in to PTS' : (st.usePts ? 'Use PTS for this one' : 'Couldn’t get SVC') }),
      st.needsLogin ? el('button', { class: 'ghost small', text: 'Sign in', onclick: async () => { await api.signIn(); getSvc(num); } })
        : st.usePts ? el('button', { class: 'ghost small', text: 'Open form', onclick: () => (st.url ? api.openUrl(st.url) : api.openPts()) })
          : retrieve('Try again', 'ghost small')];
  } else kids = [retrieve()];
  slot.replaceChildren(...[...kids, some].filter(Boolean));
}

async function getSvc(num) {
  svcState.set(num, { status: 'loading' });
  renderSvc(num);
  let res;
  try { res = await api.svc(num); } catch (e) { res = { error: true, message: String(e?.message || e) }; }
  if (res?.error) res = { status: 'error', message: res.message, needsLogin: !!res.needsLogin };
  svcState.set(num, res || { status: 'error' });
  if (res?.status === 'ok') copy(res.code);
  renderSvc(num);
}
