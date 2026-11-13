// TSBs, loading the list for this VIN, the buttons and keys, and starting the page (the saved filter comes back; an
// article sent from Diagnostics opens once the list is in). Loaded last, after the other files (see common.js).
'use strict';

// ---------- loading ----------
function showState(...kids) { $('t-body').replaceChildren(el('div', { class: 'state' }, ...kids)); }
async function load() {
  showState(el('div', { class: 'spinner' }), 'Loading TSBs for this VIN…');
  let res;
  try { res = await api.tsbList(); } catch (e) { res = { error: true, message: String(e?.message || e) }; }
  if (res?.needsDealer) {
    const input = el('input', { class: 'o-odo', placeholder: 'USA12345', maxlength: '12' });
    const go = async () => { if (input.value.trim() && (await api.setDealerCode(input.value.trim()))) load(); else input.focus(); };
    input.addEventListener('keydown', (e) => { if (e.key === 'Enter') go(); });
    showState(el('h2', { text: 'Your Warranty Dealer Code' }),
      el('p', { text: "Ford's TSB list needs the dealership's code. It's also in ⚙ Settings." }),
      el('div', { class: 'actions' }, input, el('button', { class: 'primary', text: 'Save', onclick: go })));
    setTimeout(() => input.focus(), 0);
    return false;
  }
  if (res?.error) {
    showState(el('h2', { text: res.needsLogin ? 'Sign in to PTS' : "Couldn't load TSBs" }), el('p', { text: res.message || '' }),
      el('div', { class: 'actions' }, res.needsLogin
        ? el('button', { class: 'primary', text: 'Sign in to PTS', onclick: async () => { await api.signIn(); load(); } })
        : el('button', { class: 'primary', text: 'Try again', onclick: load })));
    return false;
  }
  readList(res.html);
  renderList();
  return true;
}

$('t-q').addEventListener('input', () => { state.q = $('t-q').value; save(); renderList(); });
$('t-form').addEventListener('submit', (e) => e.preventDefault());
$('t-cat').addEventListener('change', () => { state.cat = $('t-cat').value; save(); renderList(); });
$('btn-back').onclick = showList;
$('btn-print').onclick = () => window.print();
$('btn-ford').onclick = fordPage;
document.addEventListener('keydown', (e) => { if (e.key === 'Escape' && current) showList(); });
let pendingDiag = null; // sent from Diagnostics before the list loaded
api.onOpen?.((o) => { if (o?.tsb) { if (articles.length) openFromDiag(o.tsb); else pendingDiag = o.tsb; } });

(async () => {
  info = (await api.init().catch(() => null)) || {};
  if (info.state && typeof info.state === 'object') state = { ...state, ...info.state };
  $('t-q').value = state.q || '';
  $('t-sub').textContent = [info.ymm, info.vin].filter(Boolean).join(' · ') || 'Bulletins and messages for this VIN';
  if (info.open?.tsb) pendingDiag = info.open.tsb;
  await load();
  if (pendingDiag) { const t = pendingDiag; pendingDiag = null; openFromDiag(t); }
})();
