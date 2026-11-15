// Connected Vehicle › Software: the vehicle's software subscriptions (connectivity package, BlueCruise, and so on):
// what's active and until when, and what the customer can still start (a trial, or one that's run out).
'use strict';

let subsRes = null;
const PLAN_TYPES = { AUTO: 'Free trial', FREE: 'Free', PAID: 'Paid' };

function renderSubs(res) {
  const notes = [];
  const sec = res.security || {};
  if (sec.modemAvailable === false) notes.push('Ford doesn’t see a modem in this vehicle.');
  else if (sec.modemAuthorized === false) notes.push('The vehicle’s modem isn’t authorized yet (the customer turns on connected services in the vehicle or the Ford app).');
  if (res.message) notes.push(res.message);
  const list = res.products || [];
  const statusCls = (p) => (p.active ? (p.expiringSoon ? 'pending' : 'ok') : p.trial ? 'pending' : 'bad');
  const table = list.length
    ? el('table', { class: 'cv-table' },
      el('thead', {}, el('tr', {}, ...['Subscription', 'Current status', 'Type', 'Expiration date'].map((h) => el('th', { text: h })))),
      el('tbody', {}, ...list.map((p) => el('tr', {},
        el('td', { class: 'name', text: p.name }),
        el('td', { class: statusCls(p) }, p.status, p.expiringSoon ? el('span', { class: 'note', text: ' · expiring soon' }) : null, p.renewAnytime ? el('span', { class: 'note', text: ' · renew anytime' }) : null),
        el('td', { text: PLAN_TYPES[p.type] || words(p.type) || '' }),
        el('td', { text: p.expires ? fmtDate(p.expires) : '—' })))))
    : el('div', { class: 'cv-empty', text: res.none ? 'Ford has no subscription information for this vehicle.' : 'No software subscriptions for this vehicle.' });
  return el('section', { class: 'o-section' },
    el('h2', {}, 'Vehicle software subscriptions', res.asked ? el('span', { class: 'cv-when', text: `As of ${fmtTime(res.asked)}` }) : null),
    ...notes.map((t) => msg(t)),
    el('div', { class: 'o-card cv-subs' }, table),
    el('p', { class: 'cv-foot', text: 'Customers start or renew subscriptions in the Ford app or the vehicle’s screen; dealers can activate them through the Dealer Member Portal.' }));
}

async function showSubs(fresh) {
  const body = $('cv-body');
  if (fresh || !subsRes) {
    body.replaceChildren(el('div', { class: 'state' }, el('div', { class: 'spinner' }), 'Asking Ford for the vehicle’s software subscriptions…'));
    subsRes = await ask(() => api.cvSubscriptions());
  }
  if (state.view !== 'subs') return;
  if (subsRes?.error) { body.replaceChildren(failed(subsRes, 'software subscriptions', () => showSubs(true))); return; }
  try { body.replaceChildren(renderSubs(subsRes)); } catch (e) {
    api.log(`connected vehicle subscriptions: ${e && e.stack || e}`);
    body.replaceChildren(failed({ message: String(e?.message || e) }, 'software subscriptions', () => showSubs(true)));
  }
}
