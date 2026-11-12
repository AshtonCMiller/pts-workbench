// PTS Workbench — app UI (top bar, RO lists, lines, flags, clock, tab strip, dialogs)
const api = window.workbench;

// Xtime CurrentMode values. 5 is a best guess — fix the label here once confirmed.
const MODES = {
  3: { label: 'In Inspection', color: '#7a5af5' },
  4: { label: 'Pending Approval', color: '#d97706' },
  5: { label: 'Pending Approval (parts?)', color: '#d97706' },
  6: { label: 'In Repair', color: '#15803d' },
  7: { label: 'Pending Review', color: '#0e7490' },
};
const FLAGS = [
  ['waitingParts', 'Waiting on Parts', '#dc2626'],
  ['waitingCustomer', 'Waiting on Customer', '#b45309'],
  ['waitingWarranty', 'Waiting on Warranty', '#7c3aed'],
];
// The Workbench shows the vehicle picked in the sidebar
const inVehicle = (s) => !!s && s.mode === 'manual';

const LINE_STATUS = {
  flagged: { label: 'Flagged', color: '#15803d' },
  done: { label: 'Done in Xtime', color: '#0e7490' },
  todo: { label: 'To do', color: '#1a5fb4' },
};

let state = null;
let editingTab = false;   // don't redraw the tab strip while a tab is being renamed
let editing = false;      // don't redraw lists while hours are being typed
let resizing = false;
let lastMsgCount = null;
const $ = (id) => document.getElementById(id);

function el(tag, props = {}, ...children) {
  const n = document.createElement(tag);
  for (const [k, v] of Object.entries(props)) {
    if (k === 'class') n.className = v;
    else if (k === 'text') n.textContent = v;
    else if (k === 'checked') n.checked = !!v;
    else if (k.startsWith('on')) n.addEventListener(k.slice(2), v);
    else n.setAttribute(k, v);
  }
  for (const c of children) if (c != null && c !== false && c !== '') n.append(c);
  return n;
}
const kids = (...xs) => xs.filter((x) => x != null && x !== false && x !== '');

const ymmOf = (r) => [r.year, r.make, r.model].filter(Boolean).join(' ');
const fmtH = (h) => `${(Math.round((h || 0) * 10) / 10).toFixed(1)} h`;
const hm = (ms) => {
  const m = Math.max(0, Math.floor(ms / 60000));
  return `${Math.floor(m / 60)}:${String(m % 60).padStart(2, '0')}`;
};
const hms = (ms) => {
  const s = Math.max(0, Math.floor(ms / 1000));
  return `${Math.floor(s / 3600)}:${String(Math.floor(s / 60) % 60).padStart(2, '0')}:${String(s % 60).padStart(2, '0')}`;
};
const todayISO = () => new Date(Date.now() - new Date().getTimezoneOffset() * 60000).toISOString().slice(0, 10);
const shortDate = (t) => new Date(t).toLocaleDateString([], { month: 'short', day: 'numeric' });
function periodLabel(p, withYear = false) {
  const last = new Date(p.end - 1);
  const s = `${shortDate(p.start)}–${last.getDate()}`;
  return withYear ? `${s}, ${last.getFullYear()}` : s;
}
// Clocked time not yet included in the last state push (still clocked in)
const liveExtra = () => (state?.tracking?.clockedIn ? Date.now() - state.tracking.now : 0);
const clockTime = (t) => new Date(t).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });

function badge(text, color) {
  const b = el('span', { class: 'badge', text });
  b.style.color = color;
  return b;
}

function badgesFor(r) {
  const wrap = el('div', { class: 'badges' });
  if (r.mode != null) {
    const m = MODES[r.mode] || { label: `Mode ${r.mode}`, color: '#6b7280' };
    wrap.append(badge(m.label, m.color));
  }
  for (const [key, label, color] of FLAGS) if (r[key]) wrap.append(badge(label, color));
  return wrap.childElementCount ? wrap : null;
}

// ---------- the app's own Xtime dashboard ----------
// Same sections and columns as Xtime's dashboard, from the data the app already reads every poll.
const DASH_SECTIONS = [
  [[2], 'I', 'Awaiting Dispatch'], [[3], 'II', 'In Inspection'], [[4, 5], 'III', 'Pending Approval'],
  [[6], 'IV', 'In Repair'], [[7], 'V', 'Pending Review'],
];
const DASH_SKIP = new Set([9]); // PreRO: Xtime's own dashboard doesn't show these
const dashClosed = new Set();
function since(v) {
  if (v == null || v === '') return '';
  if (typeof v === 'number' && v < 100000) return `${Math.round(v)}d`;
  const m = String(v).match(/\/Date\((\d+)/);
  const d = m ? new Date(Number(m[1])) : new Date(v);
  if (isNaN(d)) return String(v);
  const mins = (Date.now() - d) / 60000;
  if (mins < 60) return `${Math.max(0, Math.round(mins))}m`;
  if (mins < 1440) return `${Math.round(mins / 60)}h`;
  return `${Math.floor(mins / 1440)}d`;
}
function dashDate(v) {
  if (!v) return '';
  const m = String(v).match(/\/Date\((\d+)/);
  const d = m ? new Date(Number(m[1])) : new Date(v);
  if (isNaN(d)) return String(v);
  return d.toLocaleDateString([], { month: '2-digit', day: '2-digit', year: '2-digit' });
}
// ---------- signing in to Xtime from the dashboard ----------
// The app fills in Cox's real sign-in page behind the scenes; this is just the form you type into.
let signinShown = '';
function renderSignin(s) {
  const box = $('xsignin');
  const si = s.xtimeSignin;
  box.hidden = !si;
  $('xdash-body').classList.toggle('dim', !!si);
  if (!si) { signinShown = ''; box.replaceChildren(); return; }
  // Don't redraw while you're typing in this step (keeps what you've typed)
  const key = JSON.stringify([si.step, si.error, si.busy, si.factors, si.message, si.buttons, si.slow]);
  if (key === signinShown && (box.contains(document.activeElement) || si.step === 'loading')) return;
  signinShown = key;
  // Starting up / signed in: stays up until the dashboard has data (or Xtime really asks to sign in)
  if (si.step === 'loading' || si.step === 'starting') {
    const starting = si.step === 'starting';
    box.replaceChildren(el('div', { class: 'si-card si-loading' },
      el('h3', { text: starting ? 'Connecting to Xtime' : 'Signed in' }),
      el('div', { class: 'si-busy big' }, el('span', { class: 'si-spin' }), starting ? 'Loading in the background…' : 'Loading Inspect view…'),
      el('p', { class: 'muted small', text: si.slow ? 'This is taking longer than usual.'
        : starting ? 'Xtime usually signs in by itself. If it needs you, the sign-in box appears here.'
          : 'Getting your ROs from Xtime. This usually takes a few seconds.' }),
      si.slow ? el('div', { class: 'si-row' },
        el('button', { class: 'primary', text: 'Reload Xtime', onclick: () => api.send('reload-xtime') }),
        el('button', { class: 'ghost', text: 'Use Xtime’s own page', onclick: () => api.send('toggle-dashboard') })) : null));
    return;
  }
  const send = (payload) => api.send('xtime-signin', payload);
  const err = si.error ? el('div', { class: 'si-err', text: si.error }) : null;
  const busy = si.busy ? el('div', { class: 'si-busy' }, el('span', { class: 'si-spin' }), 'Signing in…') : null;
  let body;
  if (si.step === 'username' || si.step === 'password') {
    const user = el('input', { id: 'si-user', autocomplete: 'username', value: si.username || '', placeholder: 'Username' });
    // With a saved login the password box can stay empty: the app uses the saved one (it never comes to the screen)
    const pass = el('input', { id: 'si-pass', type: 'password', autocomplete: 'current-password',
      placeholder: si.savedPassword ? '•••••••• saved: just press Sign in' : 'Password' });
    const remember = el('input', { type: 'checkbox', id: 'si-remember' });
    if (si.canRemember) remember.checked = true;
    const form = el('form', { class: 'si-form', onsubmit: (e) => {
      e.preventDefault();
      if (!user.value.trim() || (!pass.value && !si.savedPassword)) return;
      send({ username: user.value.trim(), password: pass.value, remember: remember.checked });
    } },
      el('label', {}, 'Username', user), el('label', {}, 'Password', pass),
      si.canRemember ? el('label', { class: 'check' }, remember, ' Remember on this computer (encrypted with your Windows account)') : null,
      el('button', { class: 'primary', type: 'submit', ...(si.busy ? { disabled: 'disabled' } : {}), text: 'Sign in' }));
    body = form;
    setTimeout(() => { if (!si.busy) (si.username ? (si.savedPassword ? form.querySelector('button.primary') : pass) : user).focus(); }, 0);
  } else if (si.step === 'code') {
    const code = el('input', { id: 'si-code', inputmode: 'numeric', autocomplete: 'one-time-code', placeholder: 'Verification code' });
    body = el('form', { class: 'si-form', onsubmit: (e) => { e.preventDefault(); if (code.value.trim()) send({ code: code.value.trim() }); } },
      si.message ? el('p', { class: 'muted small', text: si.message }) : null,
      el('label', {}, 'Code', code),
      el('button', { class: 'primary', type: 'submit', ...(si.busy ? { disabled: 'disabled' } : {}), text: 'Verify' }));
    setTimeout(() => code.focus(), 0);
  } else if (si.step === 'choose') {
    body = el('div', { class: 'si-form' }, el('p', { class: 'muted small', text: 'Xtime wants to verify it’s you. Where should the code go?' }),
      ...si.factors.map((f, i) => el('button', { class: 'ghost', text: f, ...(si.busy ? { disabled: 'disabled' } : {}), onclick: () => send({ choose: i }) })));
  } else {
    // A step the app doesn't handle itself (user agreement, another session…): show what the page says and its buttons
    body = el('div', { class: 'si-form' }, el('p', { class: 'small', text: si.message || 'Xtime is asking for something the app can’t show here.' }),
      el('div', { class: 'si-row' }, ...(si.buttons || []).map((b) => el('button', { class: 'ghost', text: b, onclick: () => send({ press: b }) }))));
  }
  box.replaceChildren(el('div', { class: 'si-card' },
    el('h3', { text: 'Sign in to Xtime' }),
    el('p', { class: 'muted small', text: 'Your Xtime session has ended. Sign in here, the same as on Xtime’s page.' }),
    err, busy, body,
    el('button', { class: 'link-like small', text: 'Use Xtime’s own page instead', onclick: () => api.send('toggle-dashboard') })));
}

// ---------- an RO's lines, under its row ----------
const dashOpenROs = new Set();
const roLines = new Map(); // ro -> { loading, error, lines }
function toggleLines(ro) {
  if (dashOpenROs.has(ro)) dashOpenROs.delete(ro);
  else { dashOpenROs.add(ro); roLines.set(ro, { ...(roLines.get(ro) || {}), loading: true }); api.send('ro-lines', { ro }); }
  if (state) renderDashboard(state);
}
api.onLines?.((data) => {
  roLines.set(data.ro, data.ok ? { lines: data.lines } : { error: data.error, lines: roLines.get(data.ro)?.lines });
  if (state && dashOpenROs.has(data.ro)) renderDashboard(state);
});

// ---------- Dealership chat (Xtime's Dealership tab; customer chats are left out) ----------
// Messages live only in this screen: never saved, never synced, and nothing is marked read.
// A message is sent only when you press Send, and only to a conversation that already exists.
const chat = { list: null, listError: '', listLoading: false, hist: new Map(), channel: null, ro: null, lastAsk: 0,
  people: null, sending: false, sendError: '', drafts: new Map(), picked: new Map() };
const chatTime = (t) => {
  if (t == null || t === '') return null;
  const d = new Date(typeof t === 'string' && /^\d+$/.test(t) ? Number(t) : t);
  return Number.isNaN(d.getTime()) ? null : d;
};
const chatWhen = (t) => {
  const d = chatTime(t);
  if (!d) return '';
  const today = new Date().toDateString() === d.toDateString();
  return today ? d.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' })
    : d.toLocaleString([], { month: 'short', day: 'numeric', hour: 'numeric', minute: '2-digit' });
};
// One entry per conversation: the newest message of each channel
function chatConversations() {
  const by = new Map();
  for (const m of chat.list || []) {
    const k = m.channelId || `ro:${m.ro}`;
    const cur = by.get(k);
    if (!cur || (chatTime(m.time)?.getTime() || 0) > (chatTime(cur.time)?.getTime() || 0)) by.set(k, { ...m, unread: m.unread || cur?.unread });
    else if (m.unread) cur.unread = true;
  }
  return [...by.values()].sort((a, b) => (chatTime(b.time)?.getTime() || 0) - (chatTime(a.time)?.getTime() || 0));
}
const chatROs = () => new Set(chatConversations().map((c) => c.ro).filter(Boolean));
function chatAskList() { chat.listLoading = true; chat.lastAsk = Date.now(); api.send('chat-list'); }
function chatAskHistory(channelId) {
  chat.hist.set(channelId, { ...(chat.hist.get(channelId) || {}), loading: true });
  api.send('chat-history', { channelId });
}
// Open the panel: everything, or one RO's conversation
function openChat(ro) {
  chat.ro = ro || null;
  chat.channel = null;
  api.send('chat-toggle', { open: true });
  chatAskList();
  if (!chat.people) api.send('chat-roster');
  renderChat();
}
api.onChat?.((d) => {
  if (d.kind === 'list') {
    chat.listLoading = false;
    if (d.ok) { chat.list = d.conversations || []; chat.listError = ''; } else chat.listError = d.error;
    if (chat.channel) chatAskHistory(chat.channel);
  } else if (d.kind === 'history') {
    chat.hist.set(d.channelId, d.ok ? { messages: d.messages || [] } : { error: d.error, messages: chat.hist.get(d.channelId)?.messages });
  } else if (d.kind === 'read') {
    // Read receipts went to Xtime: show those messages as read here too
    const read = new Set(d.ids || []);
    const clear = (m) => (m.unread && read.has(String(m.id)) ? { ...m, unread: false } : m);
    if (chat.list) chat.list = chat.list.map((m) => (m.channelId === d.channelId && !m.mine ? { ...m, unread: false } : clear(m)));
    for (const [k, h] of chat.hist) if (h.messages) chat.hist.set(k, { ...h, messages: h.messages.map(clear) });
  } else if (d.kind === 'roster') {
    if (d.ok) chat.people = d.people || [];
  } else if (d.kind === 'send') {
    chat.sending = false;
    chat.sendError = d.ok ? '' : (d.error || "Couldn't send");
    if (d.ok) { chat.drafts.delete(d.channelId); if (composer.channel === d.channelId) composer.input.value = ''; }
    composer.update();
    return;
  }
  renderChat();
  if (state?.mode === 'xtime' && state.settings.xtimeDashboard) renderDashboard(state);
});
// Keep it current while it's open
setInterval(() => { if (state?.chatOpen) chatAskList(); }, 30000);

// Text with @mentions highlighted, the way Xtime shows them
function withMentions(text, mentions) {
  const names = [...new Set((mentions || []).filter(Boolean))].sort((x, y) => y.length - x.length);
  if (!names.length) return [text];
  const esc = (n) => n.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  return String(text).split(new RegExp(`(@(?:${names.map(esc).join('|')}))`, 'g'))
    .map((part, i) => (i % 2 ? el('span', { class: 'chat-mention', text: part }) : part)).filter((p) => p !== '');
}
function chatMessages(channelId) {
  const h = chat.hist.get(channelId) || {};
  if (h.loading && !h.messages) return el('div', { class: 'chat-note', text: 'Loading the conversation…' });
  if (h.error && !h.messages) return el('div', { class: 'chat-note bad', text: `Couldn't load the conversation: ${h.error}` });
  const msgs = [...(h.messages || [])].filter((m) => m.text).sort((a, b) => (chatTime(a.time)?.getTime() || 0) - (chatTime(b.time)?.getTime() || 0));
  if (!msgs.length) return el('div', { class: 'chat-note', text: 'No messages in this conversation yet.' });
  const out = el('div', { class: 'chat-msgs' },
    h.error ? el('div', { class: 'chat-note bad', text: `Couldn't refresh: ${h.error}` }) : null,
    ...msgs.map((m) => el('div', { class: `chat-msg${m.notification ? ' note' : ''}${m.mine ? ' mine' : ''}` },
      el('div', { class: 'chat-meta' }, el('b', { text: m.sender || (m.notification ? 'Xtime' : '') }), el('span', { text: chatWhen(m.time) })),
      el('div', { class: 'chat-text' }, ...withMentions(m.text, m.mentions)))));
  setTimeout(() => { out.scrollTop = out.scrollHeight; }, 0);
  return out;
}

// The message box stays put while the rest of the panel redraws, so typing isn't interrupted
const composer = (() => {
  const input = el('textarea', { class: 'chat-input', rows: '2', placeholder: 'Message… (@ to mention someone)' });
  const pop = el('div', { class: 'chat-pop', hidden: 'hidden' });
  const status = el('div', { class: 'chat-status small' });
  const sendBtn = el('button', { class: 'primary small', text: 'Send' });
  const box = el('div', { class: 'chat-compose' }, pop, input, el('div', { class: 'chat-compose-row' }, status, sendBtn));
  const c = { box, input, channel: null, ro: null, choices: [], active: 0 };
  const mentionAt = () => {
    const before = input.value.slice(0, input.selectionStart);
    const m = /(^|\s)@([A-Za-z][A-Za-z.'-]*(?: [A-Za-z.'-]*)?)$/.exec(before) || /(^|\s)@$/.exec(before);
    return m ? { start: before.lastIndexOf('@'), q: (m[2] || '').toLowerCase() } : null;
  };
  const showPop = () => {
    const at = mentionAt();
    const people = chat.people || [];
    c.choices = at ? people.filter((p) => !at.q || p.name.toLowerCase().split(/\s+/).some((w, i, ws) => ws.slice(i).join(' ').startsWith(at.q))).slice(0, 6) : [];
    c.active = Math.min(c.active, Math.max(0, c.choices.length - 1));
    pop.hidden = !c.choices.length;
    pop.replaceChildren(...c.choices.map((p, i) => el('button', { class: `chat-pick${i === c.active ? ' on' : ''}`, text: p.name,
      onmousedown: (e) => { e.preventDefault(); pick(p); } })));
  };
  const pick = (p) => {
    const at = mentionAt();
    if (!at) return;
    const caret = input.selectionStart;
    input.value = `${input.value.slice(0, at.start)}@${p.name} ${input.value.slice(caret)}`;
    const pos = at.start + p.name.length + 2;
    input.setSelectionRange(pos, pos);
    chat.picked.set(p.name, p.id);
    pop.hidden = true; c.choices = [];
    input.focus();
  };
  // "@Brenton Skeeters" → Xtime's mention markup, for names picked from the list
  const toMarkup = (text) => {
    let out = text;
    for (const [name, id] of [...chat.picked].sort((x, y) => y[0].length - x[0].length)) out = out.split(`@${name}`).join(`[${name}](${id})`);
    return out;
  };
  const send = () => {
    const text = input.value.trim();
    if (!text || chat.sending || !c.channel) return;
    chat.sending = true; chat.sendError = '';
    api.send('chat-send', { channelId: c.channel, ro: c.ro, text: toMarkup(text) });
    c.update();
  };
  input.addEventListener('input', () => { if (c.channel) chat.drafts.set(c.channel, input.value); showPop(); });
  input.addEventListener('keydown', (e) => {
    if (c.choices.length && ['ArrowDown', 'ArrowUp', 'Enter', 'Tab', 'Escape'].includes(e.key)) {
      e.preventDefault();
      if (e.key === 'ArrowDown') c.active = (c.active + 1) % c.choices.length;
      else if (e.key === 'ArrowUp') c.active = (c.active - 1 + c.choices.length) % c.choices.length;
      else if (e.key === 'Escape') { pop.hidden = true; c.choices = []; return; }
      else { pick(c.choices[c.active]); return; }
      showPop();
      return;
    }
    if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); send(); }
  });
  input.addEventListener('blur', () => setTimeout(() => { pop.hidden = true; }, 150));
  sendBtn.onclick = send;
  c.set = (channel, ro) => {
    if (channel === c.channel) return;
    c.channel = channel; c.ro = ro;
    input.value = (channel && chat.drafts.get(channel)) || '';
    chat.sendError = '';
    c.update();
  };
  c.update = () => {
    input.disabled = chat.sending || !c.channel;
    sendBtn.disabled = chat.sending || !c.channel;
    sendBtn.textContent = chat.sending ? 'Sending…' : 'Send';
    status.className = `chat-status small${chat.sendError ? ' bad' : ''}`;
    status.textContent = chat.sendError || (chat.sending ? 'Sending to Xtime…' : 'Enter to send · Shift+Enter for a new line');
  };
  c.update();
  return c;
})();

function renderChat() {
  const box = $('chat-panel');
  if (!box || !state) return;
  document.body.classList.toggle('chat', !!state.chatOpen);
  if (!state.chatOpen) return;
  // In the Workbench, the open vehicle's RO is the one shown
  const ro = chat.ro || (inVehicle(state) ? state.activeRO : null);
  const all = chatConversations();
  const convs = ro ? all.filter((c) => c.ro === String(ro)) : all;
  if (!chat.channel && ro && convs.length === 1 && convs[0].channelId) { chat.channel = convs[0].channelId; chatAskHistory(chat.channel); }
  const open = chat.channel ? all.find((c) => c.channelId === chat.channel) || { channelId: chat.channel, ro } : null;

  const head = el('div', { class: 'chat-head' },
    open || ro ? el('button', { class: 'ghost small', text: '‹ All', title: 'All dealership conversations',
      onclick: () => { chat.channel = null; chat.ro = null; renderChat(); } }) : null,
    el('div', { class: 'chat-title' }, open?.ro || ro ? `RO ${open?.ro || ro}` : 'Dealership chat'),
    el('button', { class: 'icon', text: '↻', title: 'Refresh', onclick: () => { chatAskList(); renderChat(); } }),
    el('button', { class: 'icon', text: '✕', title: 'Close chat', onclick: () => api.send('chat-toggle', { open: false }) }));

  let body;
  if (open) {
    body = el('div', { class: 'chat-conv' },
      el('div', { class: 'chat-sub muted small', text: [open.vehicle, open.customer, open.tech && `Tech: ${open.tech}`].filter(Boolean).join(' · ') }),
      chatMessages(open.channelId));
  } else if (chat.list == null) {
    body = el('div', { class: `chat-note${chat.listError ? ' bad' : ''}`, text: chat.listError ? `Couldn't load Xtime chat: ${chat.listError}` : 'Loading Xtime chat…' });
  } else if (!convs.length) {
    body = el('div', { class: 'chat-note', text: ro ? `No dealership messages on RO ${ro}.` : 'No dealership messages in the last 2 weeks.' });
  } else {
    body = el('div', { class: 'chat-list' },
      chat.listError ? el('div', { class: 'chat-note bad', text: `Couldn't refresh: ${chat.listError}` }) : null,
      ...convs.map((c) => el('button', { class: `chat-item${c.unread ? ' unread' : ''}`,
        onclick: () => { chat.channel = c.channelId; if (c.channelId) chatAskHistory(c.channelId); renderChat(); } },
        el('div', { class: 'chat-meta' }, el('b', { text: c.ro ? `RO ${c.ro}` : 'Conversation' }),
          c.customer ? el('span', { text: c.customer }) : null, el('span', { class: 'when', text: chatWhen(c.time) })),
        el('div', { class: 'chat-prev' }, c.sender ? el('b', { text: `${c.sender}: ` }) : null, ...withMentions(c.text || '', c.mentions)))));
  }
  // The message box: under an open conversation only (new conversations start on Xtime's page)
  composer.set(open?.channelId || null, open?.ro || ro || '');
  const foot = open ? composer.box
    : el('button', { class: 'link-like small chat-foot', text: ro && !convs.length ? "Start a conversation on Xtime's page" : "Open Xtime's message center",
      onclick: () => api.send('open-messages') });
  let main = box.querySelector(':scope > .chat-main');
  if (!main) { main = el('div', { class: 'chat-main' }); box.replaceChildren(main); }
  main.replaceChildren(head, body);
  if (box.lastChild !== foot) { [...box.children].forEach((n) => { if (n !== main) n.remove(); }); box.append(foot); }
}
const usd = (n) => (n == null ? '' : n.toLocaleString([], { style: 'currency', currency: 'USD' }));
function linesTable(r) {
  const info = roLines.get(r.ro) || {};
  if (info.loading && !info.lines) return el('div', { class: 'xlines-note' }, 'Loading the lines from Xtime…');
  if (info.error && !info.lines) return el('div', { class: 'xlines-note bad' }, `Couldn't load the lines: ${info.error}`);
  // Keep each letter together: the diag first, then the repair lines that run under it
  const order = [];
  const groups = new Map();
  for (const l of info.lines || []) {
    const k = l.dms || `#${l.id}`;
    if (!groups.has(k)) { groups.set(k, []); order.push(k); }
    groups.get(k).push(l);
  }
  const lines = order.flatMap((k) => {
    const g = groups.get(k);
    const diag = g.filter((l) => l.primary);
    const rest = g.filter((l) => !l.primary);
    const under = diag.length > 0 && rest.length > 0;
    return [...diag.map((l) => ({ ...l, groupHead: under })),
      ...rest.map((l, i) => ({ ...l, sub: under, lastSub: under && i === rest.length - 1 }))];
  });
  if (!lines.length) return el('div', { class: 'xlines-note' }, 'No lines on this RO.');
  const head = el('tr', {}, ...['Line', 'Concern', 'Approval', 'Done', 'Labor type', 'Hours', 'Labor', 'Parts', 'Total', 'Added by', ''].map((h) => el('th', { text: h })));
  const rows = lines.map((l) => {
    const approval = l.approved ? ['ok', 'Approved'] : l.declined ? ['no', 'Declined'] : ['wait', 'Pending'];
    const done = el('button', { class: `xdone${l.completed ? ' on' : ''}`, text: l.completed ? '✓ Done' : 'Mark done',
      title: l.completed ? 'Done in Xtime: click to undo' : 'Mark this line done in Xtime',
      onclick: (e) => {
        e.stopPropagation();
        e.target.disabled = true; e.target.textContent = 'Saving…';
        api.send('ro-line-done', { ro: r.ro, id: l.id, value: !l.completed });
      } });
    return el('tr', { class: [l.declined && 'declined', l.groupHead && 'ghead', l.sub && 'gsub', l.lastSub && 'glast'].filter(Boolean).join(' ') },
      el('td', { class: 'ln' }, l.sub ? el('span', { class: 'xsub', text: '↳ Repair' }) : `${l.dms || ''}${l.primary ? ' · Diag' : ''}`),
      el('td', { class: 'concern', text: l.concern, title: l.concern }),
      // Primary lines are the customer's original concerns: approved from the start, so no badge
      el('td', {}, l.primary ? null : el('span', { class: `xappr ${approval[0]}`, text: approval[1] })),
      el('td', {}, l.declined ? null : done), // a declined line isn't being done
      el('td', { text: l.payType }),
      el('td', { class: 'num', text: l.hours == null ? '' : String(l.hours) }),
      el('td', { class: 'num', text: usd(l.labor) }),
      el('td', { class: 'num', text: usd(l.parts) }),
      el('td', { class: 'num', text: usd(l.total) }),
      el('td', { text: l.addedBy }),
      el('td', {}, el('button', { class: 'ghost small', text: 'Edit', title: "Edit this line on the RO's Xtime page",
        onclick: (e) => { e.stopPropagation(); api.send('open-ro', { ro: r.ro, vin: r.vin, ymm: ymmOf(r), tab: 'xtime' }); } })));
  });
  return el('div', { class: 'xlines', onclick: (e) => e.stopPropagation() },
    info.error ? el('div', { class: 'xlines-note bad' }, `Couldn't update from Xtime: ${info.error}`) : null,
    el('table', { class: 'xlt' }, el('thead', {}, head), el('tbody', {}, ...rows)));
}

// Xtime's parts status square (P column): 2 waiting at parts, 3 working, 4 complete
function partsDot(v) {
  const t = String(v ?? '').toLowerCase();
  const k = t === '2' || t.startsWith('wait') ? 'waiting' : t === '3' || t.startsWith('work') ? 'working' : t === '4' || t.startsWith('complete') ? 'complete' : '';
  return k ? el('span', { class: `pstat ${k}`, title: { waiting: 'Waiting at parts', working: 'Parts is working on it', complete: 'Parts info complete' }[k] }) : null;
}
function renderDashboard(s) {
  const q = ($('xdash-search').value || '').trim().toLowerCase();
  const rows = (s.myROs || []).filter((r) => !DASH_SKIP.has(r.mode)).filter((r) => !q || [r.ro, r.tag, r.dash?.customer, r.year, r.make, r.model].join(' ').toLowerCase().includes(q));
  $('xdash-total').textContent = `· ${rows.length} RO${rows.length === 1 ? '' : 's'}${s.lastPoll ? ` · updated ${new Date(s.lastPoll).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' })}` : ''}`;
  const known = new Set(DASH_SECTIONS.flatMap(([m]) => m));
  const sections = [...DASH_SECTIONS, [[], '', 'Other']];
  const head = el('tr', {}, ...['', 'RO', 'Tag', 'Promised', 'M', 'Q', 'Customer', 'Vehicle', 'Adv', 'MT', 'PT'].map((h) => el('th', { text: h })));
  const out = sections.map(([modes, num, title]) => {
    const list = rows.filter((r) => (modes.length ? modes.includes(r.mode) : !known.has(r.mode)));
    if (!modes.length && !list.length) return null;
    const key = title;
    const open = !dashClosed.has(key);
    const body = list.map((r) => {
      const d = r.dash || {};
      const cls = ['xrow', (d.notInStock || r.waitingParts) && 'parts', r.waitingWarranty && 'warranty', r.waitingCustomer && 'customer', d.isNew && 'new'].filter(Boolean).join(' ');
      const flags = [(r.waitingParts || d.notInStock) && 'Parts not in stock', r.waitingWarranty && 'Waiting warranty', r.waitingCustomer && 'Waiting customer'].filter(Boolean);
      const expanded = dashOpenROs.has(r.ro);
      const tr = el('tr', { class: `${cls}${expanded ? ' expanded' : ''}`, title: flags.join(' · ') || "Open the RO's Xtime page in the Workbench",
        onclick: () => api.send('open-ro', { ro: r.ro, vin: r.vin, ymm: ymmOf(r), tab: 'xtime' }) },
        el('td', { class: 'p' }, el('button', { class: 'xexp', text: expanded ? '▾' : '▸', title: expanded ? 'Hide the lines' : 'Show the lines',
          onclick: (e) => { e.stopPropagation(); toggleLines(r.ro); } }), d.waiter ? el('b', { class: 'w', text: 'W', title: 'Customer is waiting in the store' }) : null,
          chatROs().has(String(r.ro)) ? el('button', { class: 'xchat', text: '💬', title: 'Dealership chat on this RO',
            onclick: (e) => { e.stopPropagation(); openChat(r.ro); } }) : null,
          partsDot(d.partsStatus)),
        el('td', { class: 'ro', text: r.ro }),
        el('td', { text: r.tag }),
        el('td', {}, d.express ? el('b', { text: 'E ' }) : null, dashDate(d.promised)),
        el('td', { text: String(d.media || '') }),
        el('td', { text: String(d.quote || '') }),
        el('td', { class: 'cust', text: String(d.customer || '') }),
        el('td', { class: 'veh', text: ymmOf(r) }),
        el('td', { class: 'adv', text: String(d.advisorName || d.advisor || ''), title: d.advisor ? `Advisor ${d.advisor}` : '' }),
        el('td', { text: d.modeTime != null ? since(d.modeTime) : since(d.modeSince) }),
        el('td', { text: d.partsTime != null ? since(d.partsTime) : since(d.partsSince) }));
      return expanded ? [tr, el('tr', { class: 'xlines-row' }, el('td', { colspan: '11' },
        el('div', { class: 'xlines-tools' }, el('button', { class: 'ghost small', text: chatROs().has(String(r.ro)) ? '💬 Chat on this RO' : '💬 Chat',
          title: "This RO's dealership chat", onclick: (e) => { e.stopPropagation(); openChat(r.ro); } })),
        linesTable(r)))] : [tr];
    }).flat();
    return el('section', { class: `xsec${open ? '' : ' closed'}` },
      el('h3', { onclick: () => { if (dashClosed.has(key)) dashClosed.delete(key); else dashClosed.add(key); renderDashboard(state); } },
        el('span', { class: 'tw', text: open ? '▾' : '▸' }), `${num ? `${num} - ` : ''}${title}`, el('span', { class: 'count', text: `(${list.length})` })),
      open ? (list.length ? el('table', { class: 'xtable' }, el('thead', {}, head.cloneNode(true)), el('tbody', {}, ...body))
        : el('div', { class: 'muted small xnone', text: 'None' })) : null);
  }).filter(Boolean);
  $('xdash-body').replaceChildren(...(rows.length || !q ? out : [el('div', { class: 'empty-note', text: 'No ROs match that.' })]));
}

function sortROs(list) {
  return [...list].sort((a, b) => ((a.mode ?? 99) - (b.mode ?? 99)) || a.ro.localeCompare(b.ro));
}

// Inline number editor that replaces a button until Enter / Escape / click away
// Local date <-> yyyy-mm-dd for date inputs. A picked date is stored at noon so it can't slip
// into the neighbouring day (or pay period) across time zones / daylight saving.
const isoOf = (t) => { const d = new Date(t); return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`; };
const noonOf = (iso) => { const [y, m, d] = iso.split('-').map(Number); return new Date(y, m - 1, d, 12).getTime(); };

// Inline date editor that replaces a button; saves when a date is picked, Enter, or click away
function editDate(button, ts, onSave) {
  editing = true;
  const input = el('input', { type: 'date', value: isoOf(ts), max: todayISO(), class: 'date-input' });
  let finished = false;
  const done = (save) => {
    if (finished) return;
    finished = true; editing = false;
    if (save && input.value && input.value !== isoOf(ts)) onSave(noonOf(input.value)); else render();
  };
  input.addEventListener('keydown', (ev) => { if (ev.key === 'Enter') done(true); if (ev.key === 'Escape') done(false); });
  input.addEventListener('change', () => { if (input.value) done(true); });
  input.addEventListener('blur', () => done(true));
  input.addEventListener('click', (ev) => ev.stopPropagation());
  button.replaceWith(input);
  input.focus();
  try { input.showPicker(); } catch { /* not supported: user types the date */ }
}

// Inline text editor for renaming a line. Enter/click away saves, Escape cancels; empty = Xtime's name.
function editText(target, startValue, onSave) {
  editing = true;
  const input = el('input', { type: 'text', value: startValue, class: 'name-input', placeholder: "Leave empty to use Xtime's description" });
  let finished = false;
  const done = (save) => {
    if (finished) return;
    finished = true; editing = false;
    if (save && input.value.trim() !== startValue.trim()) onSave(input.value.trim()); else render();
  };
  input.addEventListener('keydown', (ev) => { ev.stopPropagation(); if (ev.key === 'Enter') done(true); if (ev.key === 'Escape') done(false); });
  input.addEventListener('blur', () => done(true));
  input.addEventListener('click', (ev) => ev.stopPropagation());
  target.replaceWith(input);
  input.focus(); input.select();
}
const stripDiag = (label) => String(label || '').replace(/^Diag: /, '');

// Clickable description that renames the line; shows Xtime's original wording underneath when renamed
function renamable(text, original, onRename, cls = 'c-text') {
  const t = el('div', { class: `${cls} renamable`, title: 'Click to rename this line', text: text || '(no description)' });
  t.onclick = (e) => { e.stopPropagation(); editText(t, text, onRename); };
  const wrap = el('div', {}, t);
  if (original) wrap.append(el('div', { class: 'orig', title: "Xtime's description", text: `Xtime: ${original}` }));
  return wrap;
}

function editHours(button, startValue, onSave) {
  editing = true;
  const input = el('input', { type: 'number', step: '0.1', min: '0', value: String(startValue), class: 'hours-input' });
  let finished = false;
  const done = (save) => {
    if (finished) return;
    finished = true; editing = false;
    if (save) onSave(input.value); else render();
  };
  input.addEventListener('keydown', (ev) => { if (ev.key === 'Enter') done(true); if (ev.key === 'Escape') done(false); });
  input.addEventListener('blur', () => done(true));
  input.addEventListener('click', (ev) => ev.stopPropagation());
  button.replaceWith(input);
  input.focus(); input.select();
}

// ---------- lines (approved + primary only) ----------
// "Nov 2 ✎" under a flagged line: click to move the flag to the date it was really flagged
function flagDateBtn(flag) {
  const b = el('button', { class: 'flag-date', title: 'Flagged on this date. Click to change it (e.g. flagged in an earlier pay period).', text: `${shortDate(flag.flaggedAt)} ✎` });
  b.onclick = (e) => { e.stopPropagation(); editDate(b, flag.flaggedAt, (ts) => api.send('update-flag', { id: flag.id, changes: { flaggedAt: ts } })); };
  return b;
}

function lineRow(ro, l, showService = false) {
  const st = LINE_STATUS[l.status];
  const hoursBtn = el('button', {
    class: 'hours' + (l.override != null ? ' over' : ''),
    title: l.override != null
      ? `You set this line to pay ${l.override} h (Xtime says ${l.hours} h). Click to change; clear it to go back.`
      : 'Click to set what this line really pays (e.g. after warranty)',
    text: fmtH(l.paid) + (l.override != null ? '*' : ''),
  });
  hoursBtn.onclick = (e) => {
    e.stopPropagation();
    editHours(hoursBtn, l.paid, (v) => api.send('set-line-hours', { ro, key: l.key, hours: v === '' ? null : v }));
  };
  const flagBtn = l.flag
    ? el('button', {
        class: 'flag on', text: l.flag.confirmed ? '✓ Confirmed' : '✓ Flagged',
        title: `Flagged ${shortDate(l.flag.flaggedAt)}. Click to un-flag.`,
        onclick: (e) => {
          e.stopPropagation();
          if (l.flag.confirmed && !confirm('This line is already confirmed on your time sheet. Un-flag it anyway?')) return;
          api.send('unflag-line', { ro, key: l.key });
        },
      })
    : el('button', {
        class: 'flag', text: 'Flag', title: 'Mark this line done and log its hours for this pay period',
        onclick: (e) => { e.stopPropagation(); api.send('flag-line', { ro, key: l.key }); },
      });
  return el('div', { class: `line ${l.status}` },
    el('span', { class: 'dms', text: l.dms || '·' }),
    el('div', { class: 'concern', title: [l.complaint, l.service].filter(Boolean).join('\n') },
      renamable(l.complaint || l.service, l.original, (name) => api.send('rename-line', { ro, key: l.key, name })),
      showService && l.service ? el('div', { class: 'svc', text: l.service }) : null,
      el('div', { class: 'badges' }, badge(st.label, st.color),
        l.diag ? badge('Diag', '#7a5af5') : l.primary ? badge('Primary', '#6b7280') : null)),
    el('div', { class: 'line-actions' }, hoursBtn, flagBtn, l.flag ? flagDateBtn(l.flag) : null),
  );
}

function linesPanel(ro, info) {
  const wrap = el('div', { class: 'lines' });
  if (!info || !info.lines.length) {
    wrap.append(el('div', { class: 'muted small pad', text: state.lastPoll ? 'No approved lines on this RO yet.' : 'Lines load once Xtime is logged in.' }));
    return wrap;
  }
  // When the diag and repair share a letter and a concern, show Xtime's service name to tell them apart
  const sameConcern = (l) => info.lines.some((o) => o !== l && o.dms === l.dms && o.complaint === l.complaint);
  for (const l of info.lines) wrap.append(lineRow(ro, l, sameConcern(l)));
  wrap.append(el('div', { class: 'lines-foot' },
    el('span', { text: `Job pays ${fmtH(info.total)} · flagged ${fmtH(info.flagged)}` }),
    info.lines.some((l) => l.override != null) ? el('span', { class: 'muted', text: ' · * your hours' }) : null));
  return wrap;
}

// Folded line lists, per RO (this computer only)
const foldedLines = new Set((() => { try { return JSON.parse(localStorage.getItem('foldedLines') || '[]'); } catch { return []; } })());
function toggleFoldedLines(ro) {
  ro = String(ro);
  if (foldedLines.has(ro)) foldedLines.delete(ro); else foldedLines.add(ro);
  try { localStorage.setItem('foldedLines', JSON.stringify([...foldedLines].slice(-200))); } catch {}
  render();
}

function linesSummary(info) {
  if (!info || !info.lines.length) return null;
  return el('div', { class: 'sum' }, `${info.flaggedCount}/${info.lines.length} lines flagged`);
}

// Fetching this vehicle's manual, OASIS, TSBs, labor times and wiring in the background (see prefetchVehicle)
function fetchChip(ws) {
  if (!ws?.fetching) return null;
  const loading = ws.fetching === 'loading';
  return el('span', { class: 'fetch' + (loading ? ' on' : ''),
    title: loading ? 'Getting this vehicle ready in the background: manual, OASIS, TSBs, labor times and wiring'
      : 'Waiting its turn to get ready in the background' },
    el('span', { class: 'spin' }), loading ? 'Loading' : 'Queued');
}

// ---------- compact rows (sidebar) ----------
// Two short lines: RO / tag / pays, then vehicle · stage · waiting · flagged count.
function compactRow(r, { ws = null } = {}) {
  const info = state.roInfo?.[r.ro];
  const m = r.mode != null ? (MODES[r.mode] || { label: `Mode ${r.mode}`, color: '#6b7280' }) : null;
  const waiting = FLAGS.filter(([k]) => r[k]).map(([, label, color]) => [label.replace('Waiting on ', 'Wait: '), color]);
  const asleep = !!ws?.asleep; // closed to save memory; wakes when opened
  const row = el('div', { class: 'ro compact' + (asleep ? ' asleep' : '') },
    el('div', { class: 't' },
      `RO ${r.ro}`, r.tag ? el('span', { class: 'tag', text: `  · Tag ${r.tag}` }) : null,
      asleep ? el('span', { class: 'zz', text: 'Asleep' }) : fetchChip(ws),
      info && info.lines.length ? el('span', { class: 'pays', text: fmtH(info.total) }) : null),
    el('div', { class: 'c2' },
      el('span', { class: 'cap', text: ymmOf(r) || ws?.ymm || info?.ymm || '' }),
      m ? badge(m.label, m.color) : null,
      ...waiting.map(([label, color]) => badge(label, color)),
      info && info.lines.length ? el('span', { class: 'muted', text: `${info.flaggedCount}/${info.lines.length} flagged` }) : null),
  );
  if (asleep) row.title = 'Asleep to save memory: opens again where you left it';
  return row;
}

// ---------- RO cards ----------
function roCard(r, { inDrawer = false, ws = null } = {}) {
  const info = state.roInfo?.[r.ro];
  const active = ws && state.activeRO === ws.ro && inVehicle(state);
  const asleep = !!ws?.asleep && !active; // closed to save memory; wakes when opened
  const card = el('div', { class: 'ro' + (active ? ' sel' : '') + (asleep ? ' asleep' : '') },
    el('div', { class: 't' },
      `RO ${r.ro}`, r.tag ? el('span', { class: 'tag', text: `  · Tag ${r.tag}` }) : null,
      asleep ? el('span', { class: 'zz', text: 'Asleep' }) : fetchChip(ws),
      info && info.lines.length ? el('span', { class: 'pays', title: 'What the approved lines on this job pay', text: `Pays ${fmtH(info.total)}` }) : null),
    badgesFor(r),
    el('div', { class: 'v', text: ymmOf(r) || ws?.ymm || info?.ymm || '' }),
    r.vin ? el('div', { class: 'vin', text: r.vin }) : null,
  );
  if (asleep) card.title = 'Asleep to save memory: opens again where you left it';
  const sm = linesSummary(info);
  if (inDrawer) {
    if (sm) card.append(sm);
    card.append(el('div', { class: 'card-actions' },
      el('button', {
        class: 'primary open-btn', text: 'Open in Workbench',
        onclick: () => api.send('open-ro', { ro: r.ro, vin: r.vin, ymm: ymmOf(r) }),
      }),
      el('button', {
        class: 'ghost small', text: 'Hide', title: 'Hide from your lists. To bring it back, open this RO in Xtime and press Unhide.',
        onclick: () => api.send('hide-ro', r.ro),
      })));
  } else if (ws && !active) {
    // Not the vehicle you're on: compact, still draggable and closable
    const row = compactRow(r, { ws });
    row.dataset.ro = ws.ro;
    makeDraggable(row);
    row.addEventListener('click', () => api.send('select-ro', ws.ro));
    row.append(el('button', {
      class: 'x', title: 'Close this vehicle', text: '×',
      onclick: (e) => { e.stopPropagation(); api.send('close-ro', ws.ro); },
    }));
    return row;
  } else if (ws) {
    card.dataset.ro = ws.ro;
    makeDraggable(card);
    card.addEventListener('click', () => api.send('select-ro', ws.ro));
    card.append(el('button', {
      class: 'x', title: 'Close this vehicle', text: '×',
      onclick: (e) => { e.stopPropagation(); api.send('close-ro', ws.ro); },
    }));
    if (active) {
      // The lines can be folded away (remembered per RO on this computer)
      const folded = foldedLines.has(String(r.ro));
      const hasLines = !!info?.lines?.length;
      card.append(el('button', {
        class: `lines-toggle${folded ? ' folded' : ''}`, type: 'button',
        title: folded ? 'Show this RO\'s lines' : 'Hide this RO\'s lines',
        onclick: (e) => { e.stopPropagation(); toggleFoldedLines(r.ro); },
      }, el('span', { class: 'chev', text: folded ? '▸' : '▾' }), 'Lines',
        hasLines ? el('span', { class: 'muted', text: ` · ${info.flaggedCount}/${info.lines.length} flagged · pays ${fmtH(info.total)}` }) : null));
      if (!folded) card.append(linesPanel(r.ro, info));
      card.append(el('div', { class: 'card-actions end' }, el('button', {
        class: 'ghost small', text: 'Mark completed', title: 'Move this RO to Completed (and close its tabs)',
        onclick: (e) => { e.stopPropagation(); markCompleted(r.ro, info); },
      })));
    }
    else if (sm) card.append(sm);
  } else {
    // My ROs (not open yet): compact, click to open
    const row = compactRow(r);
    row.title = 'Click to open this RO';
    row.append(el('button', {
      class: 'hide-btn', text: 'Hide', title: 'Hide from your lists. To bring it back, open this RO in the Xtime tab and press Unhide.',
      onclick: (e) => { e.stopPropagation(); api.send('hide-ro', r.ro); },
    }));
    row.addEventListener('click', () => api.send('open-ro', { ro: r.ro, vin: r.vin, ymm: ymmOf(r) }));
    return row;
  }
  return card;
}

// ---------- drag to reorder open vehicles ----------
// Drag by the ⋮⋮ grip. While dragging, state updates don't redraw the list.
let dragRO = null;
function makeDraggable(card) {
  const grip = el('span', { class: 'grip', title: 'Drag to reorder', text: '⋮⋮' });
  grip.addEventListener('pointerdown', () => { card.draggable = true; });
  grip.addEventListener('click', (e) => e.stopPropagation());
  card.querySelector('.t').prepend(grip);

  card.addEventListener('dragstart', (e) => {
    dragRO = card.dataset.ro;
    editing = true;
    e.dataTransfer.effectAllowed = 'move';
    e.dataTransfer.setData('text/plain', dragRO);
    requestAnimationFrame(() => card.classList.add('dragging'));
  });
  card.addEventListener('dragend', () => {
    card.draggable = false;
    card.classList.remove('dragging');
    document.querySelectorAll('.drop-before, .drop-after').forEach((n) => n.classList.remove('drop-before', 'drop-after'));
    dragRO = null;
    editing = false;
    render();
  });
  card.addEventListener('dragover', (e) => {
    if (!dragRO || card.dataset.ro === dragRO) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = 'move';
    const r = card.getBoundingClientRect();
    const after = e.clientY > r.top + r.height / 2;
    card.classList.toggle('drop-after', after);
    card.classList.toggle('drop-before', !after);
  });
  card.addEventListener('dragleave', () => card.classList.remove('drop-before', 'drop-after'));
  card.addEventListener('drop', (e) => {
    e.preventDefault();
    if (!dragRO || card.dataset.ro === dragRO) return;
    const after = card.classList.contains('drop-after');
    const order = state.workspaces.map((w) => w.ro).filter((ro) => ro !== dragRO);
    let i = order.indexOf(card.dataset.ro);
    if (after) i++;
    order.splice(i, 0, dragRO);
    // Reorder locally right away, then tell the app (which saves it)
    const byRO = new Map(state.workspaces.map((w) => [w.ro, w]));
    state.workspaces = order.map((ro) => byRO.get(ro));
    api.send('reorder-ws', order);
  });
}

// ---------- Active / Review / Completed ----------
let sideView = 'active';
try { sideView = localStorage.getItem('sideView') || 'active'; } catch { /* storage unavailable */ }
const expanded = new Set(); // Review/Completed cards showing their lines

function markCompleted(ro, info) {
  const left = info ? info.lines.filter((l) => l.status !== 'flagged') : [];
  if (left.length && !confirm(`${left.length} line${left.length === 1 ? '' : 's'} on RO ${ro} ${left.length === 1 ? "isn't" : "aren't"} flagged (${fmtH(info.unflagged)}). Mark it completed anyway?`)) return;
  expanded.delete(ro);
  api.send('mark-completed', ro);
}

function flagCheck(info) {
  if (!info || !info.lines.length) return el('div', { class: 'check-line muted', text: 'No lines saved for this RO' });
  const n = info.lines.length;
  const left = info.lines.filter((l) => l.status !== 'flagged').length;
  return left
    ? el('div', { class: 'check-line warn', text: `${left} of ${n} line${n === 1 ? '' : 's'} not flagged · ${fmtH(info.unflagged)}` })
    : el('div', { class: 'check-line ok', text: `All ${n} line${n === 1 ? '' : 's'} flagged ✓ · ${fmtH(info.flagged)}` });
}

function statusCard(item, kind) {
  const info = state.roInfo?.[item.ro];
  const open = expanded.has(item.ro);
  const sel = inVehicle(state) && state.activeRO === item.ro;
  const card = el('div', { class: 'ro status-card' + (sel ? ' sel' : '') + (open ? ' open' : '') },
    el('div', { class: 't' },
      el('span', { class: 'chev', text: open ? '▾' : '▸' }),
      `RO ${item.ro}`, item.tag ? el('span', { class: 'tag', text: `  · Tag ${item.tag}` }) : null,
      info && info.lines.length ? el('span', { class: 'pays', text: fmtH(info.total) }) : null),
    el('div', { class: 'c2' },
      el('span', { class: 'cap', text: item.ymm || info?.ymm || '' }),
      kind === 'review'
        ? badge(item.reason === 'Closed' ? 'Closed' : 'Pending Review', item.reason === 'Closed' ? '#6b7280' : '#0e7490')
        : el('span', { class: 'muted', text: `Completed ${shortDate(item.completedAt)}` })),
    flagCheck(info),
  );
  card.addEventListener('click', () => { if (open) expanded.delete(item.ro); else expanded.add(item.ro); render(); });
  if (open) {
    const lp = linesPanel(item.ro, info);
    lp.addEventListener('click', (e) => e.stopPropagation()); // working in the lines doesn't collapse the card
    card.append(lp);
  }
  const actions = el('div', { class: 'card-actions' },
    el('button', { class: 'ghost small', text: 'Open', title: 'Open its tabs (Xtime, PTS…)', onclick: (e) => { e.stopPropagation(); api.send('open-any', item.ro); } }));
  if (kind === 'review') {
    const done = info && info.lines.length && info.lines.every((l) => l.status === 'flagged');
    actions.append(el('button', {
      class: done ? 'primary small-btn' : 'ghost small', text: 'Mark completed',
      onclick: (e) => { e.stopPropagation(); markCompleted(item.ro, info); },
    }));
  } else {
    actions.append(el('button', { class: 'ghost small', text: 'Move back to Active', onclick: (e) => { e.stopPropagation(); api.send('move-back', item.ro); } }));
  }
  card.append(actions);
  return card;
}

function renderStatusView() {
  const lists = state?.statusLists || { review: [], completed: [] };
  $('review-count').textContent = lists.review.length ? String(lists.review.length) : '';
  document.querySelectorAll('#side-tabs button').forEach((b) => b.classList.toggle('on', b.dataset.view === sideView));
  $('sidebar').dataset.view = sideView;
  if (sideView === 'active') return;
  const items = sideView === 'review' ? lists.review : lists.completed;
  $('status-help').textContent = sideView === 'review'
    ? 'ROs in Pending Review or closed. Check every line is flagged, then mark the RO completed.'
    : 'ROs you marked completed in the last 60 days.';
  $('status-list').replaceChildren(...(items.length
    ? items.map((it) => statusCard(it, sideView))
    : [el('div', { class: 'empty-note', text: sideView === 'review' ? 'Nothing to review.' : 'Nothing completed yet.' })]));
}

document.querySelectorAll('#side-tabs button').forEach((b) => {
  b.onclick = () => {
    sideView = b.dataset.view;
    try { localStorage.setItem('sideView', sideView); } catch { /* ignore */ }
    render();
  };
});

// ---------- Xtime not updating ----------
// After at least one good refresh: if the list hasn't updated for 5 minutes (or 3 missed checks),
// show a warning with Reload Xtime / Restart app.
function renderXtimeAlert() {
  const s = state;
  const box = $('xtime-alert');
  if (!s || !s.lastPoll) { box.hidden = true; return; }
  const limit = Math.max(5 * 60 * 1000, 3 * (Number(s.settings?.pollSeconds) || 60) * 1000);
  const age = Date.now() - s.lastPoll;
  if (age < limit) { box.hidden = true; return; }
  const mins = Math.round(age / 60000);
  const since = new Date(s.lastPoll).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });
  const howLong = mins >= 60 ? `${Math.floor(mins / 60)} h ${mins % 60} min` : `${mins} min`;
  $('xtime-alert-text').textContent = `Xtime not updating since ${since}`;
  box.title = `Your RO list hasn't updated for ${howLong}.${s.xtimeStatus ? ` Last message: ${s.xtimeStatus}` : ''}`;
  box.hidden = false;
}
$('btn-alert-reload').onclick = () => api.send('reload-xtime');
$('btn-alert-restart').onclick = () => {
  if (confirm('Restart PTS Workbench? Your windows and open vehicles come back, and the clock keeps running.')) api.send('restart-app');
};

// ---------- sync across computers ----------
const clock = (t) => new Date(t).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });
function syncText(sy) {
  if (!sy) return '';
  if (!sy.configured) return "Sync isn't available in this version yet.";
  if (!sy.enabled) return sy.isMain ? 'Off' : '';
  switch (sy.state) {
    case 'syncing': return 'Syncing…';
    case 'ok': return `Synced ${clock(sy.lastSync)}`;
    case 'error': return `Not synced: ${sy.message}${sy.lastSync ? ` (last synced ${clock(sy.lastSync)})` : ''}`;
    default: return sy.lastSync ? `Last synced ${clock(sy.lastSync)}` : 'Waiting to sync…';
  }
}
function renderSync() {
  const sy = state?.sync;
  if (!sy) return;
  const chip = $('sync-chip');
  chip.hidden = !(sy.configured && sy.enabled);
  chip.className = 'sync-chip' + (sy.state === 'error' ? ' bad' : sy.state === 'syncing' ? ' busy' : '');
  chip.textContent = sy.state === 'error' ? '☁ Not synced' : sy.state === 'syncing' ? '☁ Syncing' : '☁ Synced';
  chip.title = syncText(sy);
  // Settings
  $('sync-toggle').hidden = !sy.isMain || !sy.configured;
  $('btn-sync-now').hidden = !sy.enabled || !sy.configured;
  $('btn-sync-now').disabled = sy.state === 'syncing';
  $('sync-status').textContent = sy.isMain ? syncText(sy)
    : (sy.enabled ? `Set in your main window. ${syncText(sy)}` : 'Sync is turned on or off in your main window.');
  $('sync-status').className = 'small' + (sy.state === 'error' && sy.enabled ? ' error-text' : ' muted');
}
$('btn-sync-now').onclick = () => api.send('sync-now');

// Turning sync on when the server already has a copy: restore from it, or overwrite it
let syncChoiceShown = false;
function renderSyncChoice() {
  const c = state?.sync?.choice;
  if (!c) { if (syncChoiceShown) { syncChoiceShown = false; closeModal('modal-sync-choice'); } return; }
  if (syncChoiceShown) return;
  syncChoiceShown = true;
  const n = (k, one, many) => `${c[k]} ${c[k] === 1 ? one : many}`;
  const when = c.updatedAt ? ` Last changed ${new Date(c.updatedAt).toLocaleString([], { month: 'short', day: 'numeric', hour: 'numeric', minute: '2-digit' })}.` : '';
  $('sync-choice-what').textContent = `The server has ${n('ros', 'RO', 'ROs')}, ${n('flags', 'flagged line', 'flagged lines')} and ${n('shifts', 'clock entry', 'clock entries')} for employee ${state.settings.employeeNumber}.${when} What should this computer do?`;
  document.querySelectorAll('.modal:not([hidden])').forEach((m) => { if (m.id !== 'modal-sync-choice') closeModal(m.id); });
  openModal('modal-sync-choice');
}
const syncChoose = (choice) => { syncChoiceShown = false; closeModal('modal-sync-choice'); api.send('sync-choice', choice); };
$('btn-sync-restore').onclick = () => syncChoose('restore');
$('btn-sync-overwrite').onclick = () => {
  if (confirm("Replace the server's copy with this computer's? Anything that's only on the server (on your other computers) will be deleted.")) syncChoose('overwrite');
};
$('btn-sync-cancel').onclick = () => syncChoose('cancel');

// ---------- employee number & first-launch setup ----------
function employeeText() {
  const st = state?.settings || {};
  if (!st.employeeNumber) return 'Enter your employee number so this window shows your ROs.';
  if (state.employeeStatus) return state.employeeStatus;
  if (st.techId && st.techIdFor === st.employeeNumber) return state.profile?.xtimeName ? `Found: ${state.profile.xtimeName}` : 'Found in Xtime';
  return 'Looked up once you are logged in to Xtime.';
}

let setupShown = false; // ask once per session until it's answered
function maybeShowSetup() {
  if (setupShown || !state?.settings || state.settings.setupDone) return;
  if (document.querySelector('.modal:not([hidden])')) return; // don't stack on another dialog
  setupShown = true;
  const f = $('form-setup');
  f.elements.profileName.value = state.profile?.name && state.profile.name !== 'Main' ? state.profile.name : (state.profile?.xtimeName || '');
  f.elements.employeeNumber.value = state.settings.employeeNumber || '';
  f.elements.storeId.value = state.settings.storeId || '';
  f.elements.dealerCode.value = state.settings.dealerCode || '';
  openModal('modal-setup');
  setTimeout(() => (f.elements.profileName.value ? f.elements.employeeNumber : f.elements.profileName).focus(), 50);
}
$('form-setup').onsubmit = (e) => {
  e.preventDefault();
  const d = Object.fromEntries(new FormData(e.target));
  api.send('save-settings', {
    profileName: d.profileName.trim(), employeeNumber: d.employeeNumber.trim(), storeId: d.storeId.trim(), dealerCode: (d.dealerCode || '').trim(), setupDone: true,
  });
  closeModal('modal-setup');
};
$('btn-setup-later').onclick = () => closeModal('modal-setup');

// ---------- version & updates ----------
function updateText(u) {
  if (!u) return '';
  const at = u.checkedAt ? ` (checked ${new Date(u.checkedAt).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' })})` : '';
  switch (u.state) {
    case 'checking': return 'Checking for updates…';
    case 'current': return `Up to date${u.latest ? ` — newest release on GitHub is v${u.latest}` : ''}${at}`;
    case 'downloading': return `Downloading v${u.version}… ${u.percent || 0}%`;
    case 'ready': return `v${u.version} is ready to install`;
    case 'error': return `Update check failed: ${u.message}${at}`;
    case 'off': return u.message;
    default: return '';
  }
}
function renderVersion() {
  const a = state?.app;
  if (!a) return;
  $('app-version').textContent = `v${a.version}`;
  $('about-version').textContent = `Version ${a.version}`;
  $('about-update').textContent = updateText(a.update);
  $('btn-check-update').disabled = a.update.state === 'off' || a.update.state === 'checking' || a.update.state === 'downloading';
  $('btn-update-log').hidden = !a.update.logFile;
  $('app-version').classList.toggle('beta', !!a.beta);
  $('app-version').title = a.beta ? 'Beta version: new features being tested' : 'Installed version';
  $('beta-updates').checked = !!a.betaUpdates;
  $('beta-note').hidden = !(a.beta !== a.betaUpdates);
  $('beta-note').textContent = a.betaUpdates && !a.beta ? 'The newest beta downloads in the background; restart to install it.'
    : !a.betaUpdates && a.beta ? 'This computer goes back to the newest regular version with the next update.' : '';
  const btn = $('btn-update');
  const ready = a.update.state === 'ready';
  btn.hidden = !ready && a.update.state !== 'downloading';
  btn.disabled = !ready;
  btn.textContent = ready ? `Restart to update to v${a.update.version}` : `Downloading v${a.update.version}… ${a.update.percent || 0}%`;
  btn.title = ready ? 'Installs the update and reopens your windows. It will also install the next time you close the app.' : '';
}
$('btn-update').onclick = () => {
  if (confirm(`Restart now to install v${state?.app?.update?.version}? Your windows and open vehicles come back, and the clock keeps running.`)) {
    api.send('install-update');
  }
};
$('btn-check-update').onclick = () => api.send('check-update');
$('btn-update-log').onclick = () => api.send('open-update-log');
$('beta-updates').onchange = (e) => api.send('set-beta-updates', e.target.checked);

// ---------- top bar ----------
function renderTimer() {
  const t = state?.tracking;
  const box = $('timer');
  if (!t) { box.replaceChildren(); return; }
  const today = t.todayWorked + liveExtra();
  if (t.clockedIn) {
    box.className = 'timer running';
    box.replaceChildren(
      el('span', { class: 'dot on' }),
      el('span', { class: 'ro-n', text: `Clocked in ${clockTime(t.shiftStart)}` }),
      el('span', { class: 'mono', title: 'Clocked time today', text: hms(today) }),
      el('button', { class: 'ghost small', text: 'Clock out', onclick: () => api.send('clock-out') }));
  } else {
    box.className = 'timer stopped';
    const auto = t.autoClockOut
      ? el('span', { class: 'auto-note', title: 'You were still clocked in when the app was last closed, so you were clocked out at that time.',
          text: `Auto-out ${new Date(t.autoClockOut.at).toLocaleDateString([], { weekday: 'short' })} ${clockTime(t.autoClockOut.at)}` })
      : null;
    box.replaceChildren(...kids(
      el('span', { class: 'dot off' }),
      el('span', { text: today ? `Clocked out · ${hm(today)} today` : 'Clocked out' }),
      auto,
      el('button', { class: 'primary small-btn', text: 'Clock in', onclick: () => api.send('clock-in') })));
  }
}

function renderPayChip() {
  const btn = $('btn-hours');
  if (!state?.period) { btn.textContent = ''; return; }
  // The chip always shows the current pay period, whatever the dialog is showing
  const cur = state.period.isCurrent ? state.period : null;
  const total = cur ? cur.total : state.currentPeriodTotal;
  btn.replaceChildren(
    el('span', { class: 'muted', text: 'This pay period ' }),
    el('strong', { text: `${fmtH(total)} flagged` }));
}

// ---------- main render ----------
function render() {
  if (!state) return;
  const s = state;
  const body = document.body;
  body.classList.toggle('xtime', s.mode === 'xtime');
  body.classList.toggle('manual', s.mode === 'manual');
  body.classList.toggle('drawer-closed', !s.drawerOpen);
  body.classList.toggle('dash', !!s.settings.xtimeDashboard);
  $('msg-chip').hidden = !(s.msgCount > 0);
  $('msg-chip').textContent = `💬 ${s.msgCount || 0}`;
  if (s.chatOpen && s.msgCount !== lastMsgCount && Date.now() - chat.lastAsk > 3000) chatAskList();
  lastMsgCount = s.msgCount;
  $('btn-chat').classList.toggle('on', !!s.chatOpen);
  renderChat();
  // Feedback: the button for everyone; the admin also gets a count of new reports
  $('btn-feedback').hidden = !s.feedback?.on;
  const fbNew = s.feedback?.admin ? s.feedback.newCount || 0 : 0;
  $('feedback-count').hidden = !fbNew; $('feedback-count').textContent = fbNew ? String(fbNew) : '';
  $('fbtab-all').hidden = !s.feedback?.admin;
  $('fb-all-count').hidden = !fbNew; $('fb-all-count').textContent = fbNew ? String(fbNew) : '';
  $('btn-dash').classList.toggle('on', !!s.settings.xtimeDashboard);
  $('btn-dash').textContent = s.settings.xtimeDashboard ? 'Xtime page' : 'Dashboard';
  if (s.mode === 'xtime' && s.settings.xtimeDashboard) { renderSignin(s); renderDashboard(s); }
  if (!resizing) document.documentElement.style.setProperty('--side', `${s.settings.sidebarWidth || 380}px`);

  if (s.profile) document.title = `PTS Workbench - ${s.profile.displayName}`; // the window's title bar already says who it is
  $('mode-xtime').classList.toggle('on', s.mode === 'xtime');
  $('mode-manual').classList.toggle('on', s.mode === 'manual');
  $('ws-count').textContent = s.workspaces.length ? String(s.workspaces.length) : '';
  $('btn-drawer').classList.toggle('on', s.drawerOpen);
  const ago = s.lastPoll ? ` · ${new Date(s.lastPoll).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' })}` : '';
  $('xtime-status').textContent = s.xtimeStatus + ago;
  renderTimer();
  renderPayChip();
  renderVersion();
  renderSync();
  renderSyncChoice();
  renderXtimeAlert();

  const hidden = new Set(s.hidden || []);
  const statusOf = (ro) => s.roInfo?.[ro]?.status || 'active';
  const allMine = sortROs(s.myROs).filter((r) => statusOf(r.ro) === 'active');
  const mine = allMine.filter((r) => !hidden.has(r.ro));
  const activeWorkspaces = s.workspaces.filter((w) => statusOf(w.ro) === 'active');
  const hiddenHere = allMine.length - mine.length;

  // Viewing a hidden RO's page in Xtime: offer to unhide it
  const unhide = $('btn-unhide');
  const canUnhide = s.mode === 'xtime' && s.xtimeViewingRO && hidden.has(s.xtimeViewingRO);
  unhide.hidden = !canUnhide;
  if (canUnhide) {
    unhide.textContent = `Unhide RO ${s.xtimeViewingRO}`;
    unhide.onclick = () => api.send('unhide-ro', s.xtimeViewingRO);
  }
  $('hidden-count').textContent = hidden.size ? `${hidden.size} RO${hidden.size === 1 ? '' : 's'} hidden` : 'No hidden ROs';
  $('btn-unhide-all').disabled = !hidden.size;
  const byRO = new Map(mine.map((r) => [r.ro, r]));

  if (!editing) {
    $('drawer-list').replaceChildren(...(mine.length
      ? mine.map((r) => roCard(r, { inDrawer: true }))
      : [el('div', { class: 'empty-note', text: 'No ROs yet. Log in to Xtime and they will appear here.' })]));

    $('open-list').replaceChildren(...(activeWorkspaces.length
      ? activeWorkspaces.map((ws) => {
          const live = byRO.get(ws.ro);
          const card = roCard(live || { ro: ws.ro, vin: ws.vin, mode: null }, { ws });
          if (!live && s.lastPoll) card.append(el('div', { class: 'gone', text: 'Not on your Xtime list' }));
          return card;
        })
      : [el('div', { class: 'empty-note', text: 'Nothing open yet.' })]));

    $('open-count').textContent = activeWorkspaces.length ? String(activeWorkspaces.length) : '';
    const openSet = new Set(s.workspaces.map((w) => w.ro));
    const notOpen = mine.filter((r) => !openSet.has(r.ro));
    $('mine-count').textContent = notOpen.length ? String(notOpen.length) : '';
    $('mine-count').title = hiddenHere ? `${hiddenHere} hidden` : '';
    $('mine-hidden').textContent = hiddenHere ? `${hiddenHere} hidden` : '';
    $('mine-list').replaceChildren(...(notOpen.length
      ? notOpen.map((r) => roCard(r))
      : [el('div', { class: 'empty-note', text: mine.length ? 'All of your ROs are open.' : 'Waiting for Xtime…' })]));

    renderStatusView();
    if (!$('modal-hours').hidden) renderPeriod();
    if (!$('modal-settings').hidden) { renderProfiles(); renderLogins(); $('employee-status').textContent = employeeText(); }
  }

  const ws = s.workspaces.find((w) => w.ro === s.activeRO);
  body.classList.toggle('no-ws', !ws);
  // The Workbench sidebar can be tucked away while a vehicle is open
  const sideHidden = !!(ws && s.settings.sidebarHidden);
  body.classList.toggle('side-hidden', sideHidden);
  $('btn-side').textContent = sideHidden ? '☰' : '⟨';
  $('btn-side').title = sideHidden ? 'Show the sidebar (Ctrl+B)' : 'Hide the sidebar (Ctrl+B)';
  if (ws && !editingTab) renderTabs(ws);
}

// One tab (top row or the Service Manual's row): click to show, double-click to rename, × to close
function tabEl(ws, t, activeId) {
  const title = el('span', { text: t.title });
  const tab = el('div', { class: `tab kind-${t.kind}` + (t.id === activeId ? ' on' : ''), title: 'Double-click to rename' },
    el('span', { class: 'tab-dot' }),
    title,
    t.fixed ? null : el('button', { // the Service Manual's main tabs have no ×
      class: 'close', text: '×', title: t.kind === 'service' ? 'Close the Service Manual and its tabs' : 'Close tab',
      onclick: (e) => { e.stopPropagation(); api.send('close-tab', { ro: ws.ro, id: t.id }); },
    }),
  );
  tab.addEventListener('click', () => { if (t.id !== activeId) api.send('select-tab', { ro: ws.ro, id: t.id }); });
  makeTabDraggable(ws, t, tab);
  tab.addEventListener('dblclick', () => {
    const input = el('input', { value: t.title });
    let finished = false;
    editingTab = true;
    const done = (save) => {
      if (finished) return;
      finished = true; editingTab = false;
      if (save) api.send('rename-tab', { ro: ws.ro, id: t.id, title: input.value }); else render();
    };
    input.addEventListener('keydown', (e) => { if (e.key === 'Enter') done(true); if (e.key === 'Escape') done(false); });
    input.addEventListener('blur', () => done(true));
    title.replaceWith(input);
    input.focus(); input.select();
  });
  return tab;
}

// ---------- drag tabs to reorder (within their own row) ----------
// While dragging, state updates don't redraw the tabs.
let dragTab = null; // { ro, id, row: the strip's element id }
function makeTabDraggable(ws, t, tab) {
  tab.draggable = true;
  tab.dataset.id = t.id;
  const rowOf = () => tab.parentElement?.id || '';
  tab.addEventListener('dragstart', (e) => {
    if (e.target.closest('input')) { e.preventDefault(); return; } // renaming
    dragTab = { ro: ws.ro, id: t.id, row: rowOf() };
    editingTab = true;
    e.dataTransfer.effectAllowed = 'move';
    e.dataTransfer.setData('text/plain', String(t.id));
    requestAnimationFrame(() => tab.classList.add('dragging'));
  });
  tab.addEventListener('dragend', () => {
    tab.classList.remove('dragging');
    document.querySelectorAll('.tab.drop-before, .tab.drop-after').forEach((n) => n.classList.remove('drop-before', 'drop-after'));
    dragTab = null;
    editingTab = false;
    render();
  });
  tab.addEventListener('dragover', (e) => {
    if (!dragTab || dragTab.id === t.id || dragTab.row !== rowOf()) return; // only within the same row
    e.preventDefault();
    e.dataTransfer.dropEffect = 'move';
    const r = tab.getBoundingClientRect();
    const after = e.clientX > r.left + r.width / 2;
    tab.classList.toggle('drop-after', after);
    tab.classList.toggle('drop-before', !after);
  });
  tab.addEventListener('dragleave', () => tab.classList.remove('drop-before', 'drop-after'));
  tab.addEventListener('drop', (e) => {
    e.preventDefault();
    if (!dragTab || dragTab.id === t.id || dragTab.row !== rowOf()) return;
    const after = tab.classList.contains('drop-after');
    const strip = tab.parentElement;
    const ids = [...strip.querySelectorAll('.tab')].map((n) => Number(n.dataset.id)).filter((id) => id !== dragTab.id);
    let i = ids.indexOf(t.id);
    if (after) i++;
    ids.splice(i, 0, dragTab.id);
    // Move it on screen right away, then tell the app (which saves it)
    const moved = strip.querySelector(`.tab[data-id="${dragTab.id}"]`);
    if (moved) (after ? tab.after(moved) : tab.before(moved));
    api.send('reorder-tabs', { ro: dragTab.ro, ids });
  });
}

function renderTabs(ws) {
  $('tabs').replaceChildren(...ws.tabs.map((t) => tabEl(ws, t, ws.activeTab)));
  // Service Manual open: its own row of tabs (OASIS | Diagnostics | Workshop Manual | Labor Times)
  const top = ws.tabs.find((t) => t.id === ws.activeTab);
  const showSub = top?.kind === 'service' && !!ws.sub;
  document.body.classList.toggle('has-sub', showSub);
  $('subtabs').replaceChildren(...(showSub ? ws.sub.tabs.map((t) => tabEl(ws, t, ws.sub.active)) : []));
  const active = showSub ? ws.sub.tabs.find((t) => t.id === ws.sub.active) : top;
  $('tab-status').textContent = active?.status || '';
  $('btn-reenter').style.display = ['pts', 'ota', 'cvd', 'prodemand'].includes(active?.kind) ? '' : 'none';
  $('btn-reenter').textContent = active?.kind === 'prodemand' ? 'Re-enter VIN' : 'Reopen vehicle';
}

// ---------- pay period dialog ----------
function renderPeriod() {
  const p = state?.period;
  if (!p) return;
  renderLookup();
  const searching = !!(state.search && $('flag-search').value.trim());
  $('modal-hours').classList.toggle('searching', searching);
  if (searching) { renderSearch(state.search); return; }
  $('period-title').textContent = `Pay period ${periodLabel(p, true)}`;
  $('period-next').disabled = p.isCurrent;

  const worked = p.worked + (p.isCurrent ? liveExtra() : 0);
  const eff = worked > 0 ? `${Math.round((p.total / (worked / 3600000)) * 100)}%` : '—';
  const stat = (label, value, sub) => el('div', { class: 'stat' },
    el('div', { class: 'stat-l', text: label }), el('div', { class: 'stat-v', text: value }),
    sub ? el('div', { class: 'muted small', text: sub }) : null);
  $('hours-stats').replaceChildren(
    stat('Flagged', fmtH(p.total), `${p.flags.length} line${p.flags.length === 1 ? '' : 's'}`),
    stat('Confirmed', fmtH(p.confirmed), 'on your time sheet'),
    stat('Not confirmed', fmtH(p.unconfirmed), p.unconfirmed ? 'still to check' : 'all checked'),
    stat('Clocked', hm(worked), `efficiency ${eff}`));

  // Grouped by RO: an RO header row, then its lines indented underneath
  const head = el('tr', {}, ...['Line', 'Concern', 'Flagged', 'Hours', 'Confirmed', ''].map((c) => el('th', { text: c })));
  const groups = new Map();
  for (const f of p.flags) {
    if (!groups.has(f.ro)) groups.set(f.ro, []);
    groups.get(f.ro).push(f);
  }
  const lineOrder = (a, b) => String(a.dms).localeCompare(String(b.dms)) || (a.label.startsWith('Diag') ? -1 : 0) - (b.label.startsWith('Diag') ? -1 : 0);
  const rows = [];
  for (const [ro, flags] of groups) {
    flags.sort(lineOrder);
    const sub = flags.reduce((a, f) => a + f.hours, 0);
    const allConfirmed = flags.every((f) => f.confirmed);
    const ymm = flags.find((f) => f.ymm)?.ymm || state.roInfo?.[ro]?.ymm || '';
    rows.push(el('tr', { class: 'ro-head' + (allConfirmed ? ' confirmed' : '') },
      el('td', { colspan: '3' }, el('span', { class: 'strong', text: `RO ${ro}` }), ymm ? el('span', { class: 'muted cap', text: `  · ${ymm}` }) : null),
      el('td', { class: 'strong', text: fmtH(sub) }),
      el('td', { class: 'center' }, el('input', {
        type: 'checkbox', checked: allConfirmed, title: 'Confirm every line on this RO',
        onchange: (e) => flags.forEach((f) => { if (f.confirmed !== e.target.checked) api.send('update-flag', { id: f.id, changes: { confirmed: e.target.checked } }); }),
      })),
      el('td'),
    ));
    for (const f of flags) {
      const hoursBtn = el('button', { class: 'hours', text: fmtH(f.hours), title: 'Click to change' });
      hoursBtn.onclick = () => editHours(hoursBtn, f.hours, (v) => { if (v !== '') api.send('update-flag', { id: f.id, changes: { hours: v } }); else render(); });
      rows.push(el('tr', { class: 'ro-line' + (f.confirmed ? ' confirmed' : '') },
        el('td', { class: 'dmscell' }, el('span', { class: 'arrow', text: '↳' }), f.dms || '—'),
        el('td', { class: 'concern-cell' }, (() => {
          const t = el('span', { class: 'renamable', title: 'Click to rename this line', text: f.label || '(no description)' });
          t.onclick = () => editText(t, stripDiag(f.label), (name) => api.send('update-flag', { id: f.id, changes: { label: name } }));
          return t;
        })()),
        el('td', { class: 'nowrap' }, flagDateBtn(f)),
        el('td', {}, hoursBtn),
        el('td', { class: 'center' }, el('input', {
          type: 'checkbox', checked: f.confirmed, title: 'Tick once it shows on your time sheet',
          onchange: (e) => api.send('update-flag', { id: f.id, changes: { confirmed: e.target.checked } }),
        })),
        el('td', {}, el('button', {
          class: 'x-small', text: '×', title: 'Remove this flag',
          onclick: () => { if (confirm(`Remove the flag for RO ${f.ro} line ${f.dms || ''}?`)) api.send('delete-flag', f.id); },
        })),
      ));
    }
  }
  const foot = p.flags.length
    ? el('tr', { class: 'total' }, el('td', { colspan: '3', text: 'Total' }), el('td', { text: fmtH(p.total) }), el('td', { class: 'center muted', text: `${fmtH(p.confirmed)} confirmed` }), el('td'))
    : el('tr', {}, el('td', { colspan: '6', class: 'muted', text: 'Nothing flagged in this pay period yet.' }));
  $('flag-table').replaceChildren(head, ...rows, foot);

}

// ---------- "Add an RO" ----------
function renderLookup() {
  const lk = state.lookup;
  const box = $('lookup-result');
  const manual = $('form-manual');
  manual.hidden = !lk || lk.status === 'loading';
  if (!manual.hidden && !manual.elements.date.value) manual.elements.date.value = todayISO();
  if (!lk) { box.replaceChildren(); return; }
  const info = state.roInfo?.[lk.ro];
  box.replaceChildren(...kids(
    el('div', { class: 'lookup-head' },
      el('strong', { text: `RO ${lk.ro}` }),
      info?.ymm ? el('span', { class: 'muted cap', text: ` · ${info.ymm}` }) : null,
      el('button', { class: 'ghost small', text: 'Done', onclick: () => api.send('clear-lookup') })),
    lk.message ? el('p', { class: lk.status === 'error' ? 'error' : 'muted small', text: lk.message }) : null,
    info?.lines.length ? linesPanel(lk.ro, info) : null,
    el('p', { class: 'muted small', text: 'Add a line by hand (it is flagged straight away):' }),
  ));
}

// ---------- search every pay period by RO ----------
function renderSearch(search) {
  const n = search.results.length;
  $('period-title').textContent = n ? `RO search: “${search.query}”` : `No ROs matching “${search.query}”`;
  const head = el('tr', {}, ...['Line', 'Concern', 'Hours', 'Flagged', 'Paid in pay period', 'Confirmed'].map((c) => el('th', { text: c })));
  const rows = [];
  for (const r of search.results) {
    rows.push(el('tr', { class: 'ro-head' },
      el('td', { colspan: '2' }, el('span', { class: 'strong', text: `RO ${r.ro}` }), r.ymm ? el('span', { class: 'muted cap', text: `  · ${r.ymm}` }) : null),
      el('td', { class: 'strong', text: fmtH(r.flagged) }),
      el('td', { colspan: '3', class: 'muted small', text: r.flagged === r.total ? 'all lines flagged' : `${fmtH(r.total - r.flagged)} not flagged yet` }),
    ));
    if (!r.lines.length) rows.push(el('tr', {}, el('td', { colspan: '6', class: 'muted small', text: 'No approved lines saved for this RO.' })));
    for (const l of r.lines) {
      const flagged = !!l.flagId;
      rows.push(el('tr', { class: 'ro-line' + (l.confirmed ? ' confirmed' : '') },
        el('td', { class: 'dmscell' }, el('span', { class: 'arrow', text: '↳' }), l.dms || '—'),
        el('td', { class: 'concern-cell' }, (() => {
          const t = el('span', { class: 'renamable', title: 'Click to rename this line', text: l.label || '(no description)' });
          t.onclick = () => editText(t, stripDiag(l.label), (name) => l.flagId
            ? api.send('update-flag', { id: l.flagId, changes: { label: name } })
            : api.send('rename-line', { ro: r.ro, key: l.key, name }));
          const cell = el('div', {}, t);
          if (l.original) cell.append(el('div', { class: 'orig', text: `Xtime: ${l.original}` }));
          return cell;
        })()),
        el('td', { class: 'strong', text: fmtH(l.hours) }),
        el('td', { class: 'nowrap' }, flagged
          ? flagDateBtn({ id: l.flagId, flaggedAt: l.flaggedAt })
          : r.canFlag
            ? el('button', { class: 'flag', text: 'Flag', title: 'Flag this line now', onclick: () => api.send('flag-line', { ro: r.ro, key: l.key }) })
            : el('span', { class: 'muted', text: 'Not flagged' })),
        el('td', { class: 'nowrap', text: flagged ? periodLabel({ start: l.periodStart, end: l.periodEnd }, true) : '—' }),
        el('td', { class: 'center' }, flagged ? el('input', {
          type: 'checkbox', checked: l.confirmed, title: 'Tick once it shows on your time sheet',
          onchange: (e) => api.send('update-flag', { id: l.flagId, changes: { confirmed: e.target.checked } }),
        }) : null),
      ));
    }
  }
  if (!n) rows.push(el('tr', {}, el('td', { colspan: '6', class: 'muted', text: 'Nothing saved for that RO number. You can add it below under “Add an RO”.' })));
  $('flag-table').replaceChildren(head, ...rows);
}

let searchTimer = null;
$('flag-search').addEventListener('input', (e) => {
  clearTimeout(searchTimer);
  const q = e.target.value.trim();
  if (!q) { api.send('search-flags', ''); renderPeriod(); return; }
  searchTimer = setTimeout(() => api.send('search-flags', q), 150);
});

$('period-prev').onclick = () => api.send('set-period', (state?.period?.offset ?? 0) - 1);
$('period-next').onclick = () => api.send('set-period', Math.min(0, (state?.period?.offset ?? 0) + 1));
$('form-lookup').onsubmit = (e) => {
  e.preventDefault();
  const ro = e.target.elements.ro.value.trim();
  if (!ro) return;
  api.send('lookup-ro', ro);
  const f = $('form-manual');
  f.reset();
  f.elements.date.value = todayISO();
};
$('form-manual').onsubmit = (e) => {
  e.preventDefault();
  const lk = state?.lookup;
  if (!lk) return;
  const d = Object.fromEntries(new FormData(e.target));
  api.send('add-manual-line', { ro: lk.ro, dms: d.dms, complaint: d.complaint, hours: d.hours, date: d.date });
  const keepDate = e.target.elements.date.value;
  e.target.reset();
  e.target.elements.date.value = keepDate;
};

// "My ROs" can be collapsed to its header; remembered on this computer
let mineCollapsed = false;
try { mineCollapsed = localStorage.getItem('mineCollapsed') === '1'; } catch { /* storage unavailable */ }
function applyMineCollapsed() {
  $('side-mine').classList.toggle('collapsed', mineCollapsed);
  $('btn-mine-toggle').textContent = mineCollapsed ? 'Show' : 'Hide';
}
$('btn-unhide-all').onclick = () => {
  if (confirm('Show every hidden RO in your lists again?')) api.send('unhide-all');
};
$('btn-mine-toggle').onclick = () => {
  mineCollapsed = !mineCollapsed;
  try { localStorage.setItem('mineCollapsed', mineCollapsed ? '1' : '0'); } catch { /* ignore */ }
  applyMineCollapsed();
};
applyMineCollapsed();

// ---------- top bar buttons ----------
$('mode-xtime').onclick = () => api.send('set-mode', 'xtime');
$('btn-dash').onclick = () => api.send('toggle-dashboard');
$('msg-chip').onclick = () => openChat(inVehicle(state) ? state.activeRO : null);
$('btn-chat').onclick = () => { if (state?.chatOpen) api.send('chat-toggle', { open: false }); else openChat(null); };
$('xdash-search').addEventListener('input', () => { if (state) renderDashboard(state); });
$('mode-manual').onclick = () => api.send('set-mode', 'manual');
$('btn-refresh').onclick = () => api.send('refresh-ros');
$('btn-drawer').onclick = () => api.send('toggle-drawer');
$('btn-reload-xtime').onclick = () => api.send('reload-xtime');
$('btn-print').onclick = () => api.send('print-page');
$('btn-hours').onclick = () => {
  $('flag-search').value = '';
  api.send('search-flags', '');
  api.send('set-period', 0);
  renderPeriod();
  openModal('modal-hours');
  $('flag-search').focus();
};

const activeWs = () => state?.workspaces.find((w) => w.ro === state.activeRO);
$('add-tab').onchange = (e) => {
  const kind = e.target.value;
  e.target.value = '';
  const ws = activeWs();
  if (ws && kind) api.send('add-tab', { ro: ws.ro, kind });
};
$('btn-copy-vin').onclick = () => { const ws = activeWs(); if (ws) api.send('copy-vin', ws.ro); };
$('btn-reenter').onclick = () => { const ws = activeWs(); if (ws) api.send('reenter-vin', { ro: ws.ro, id: ws.viewTab }); };
$('btn-reload-tab').onclick = () => { const ws = activeWs(); if (ws) api.send('reload-tab', { ro: ws.ro, id: ws.viewTab }); };

// ---------- resizable sidebar ----------
$('resizer').addEventListener('pointerdown', (e) => {
  e.preventDefault();
  resizing = true;
  api.send('set-overlay', true); // hide PTS while dragging so the mouse isn't swallowed by it
  $('resizer').setPointerCapture(e.pointerId);
  document.body.classList.add('resizing');
});
$('resizer').addEventListener('pointermove', (e) => {
  if (!resizing) return;
  document.documentElement.style.setProperty('--side', `${Math.min(700, Math.max(260, e.clientX))}px`);
});
$('resizer').addEventListener('pointerup', (e) => {
  if (!resizing) return;
  resizing = false;
  document.body.classList.remove('resizing');
  const w = Math.min(700, Math.max(260, e.clientX));
  if (state) state.settings.sidebarWidth = w;
  api.send('set-sidebar', w);
  api.send('set-overlay', false);
});

// ---------- dialogs ----------
function openModal(id) { $(id).hidden = false; api.send('set-overlay', true); }
function closeModal(id) { $(id).hidden = true; api.send('set-overlay', false); }
document.querySelectorAll('[data-close]').forEach((b) => { b.onclick = () => closeModal(b.closest('.modal').id); });
document.addEventListener('keydown', (e) => {
  if (e.key !== 'Escape' || editing) return;
  document.querySelectorAll('.modal:not([hidden])').forEach((m) => { if (m.id === 'modal-sync-choice') syncChoose('cancel'); else closeModal(m.id); });
});

// Windows (profiles): each is one person, with their own logins, settings and hours
function renderProfiles() {
  const list = $('profile-list');
  if (!state?.profiles) return;
  $('settings-for').textContent = state.profile ? `· ${state.profile.displayName}` : '';
  list.replaceChildren(...state.profiles.map((p) => el('div', { class: 'profile-row' + (p.current ? ' current' : '') },
    el('span', { class: 'dot ' + (p.open ? 'on' : 'off') }),
    el('span', { class: 'strong', text: p.displayName }),
    p.displayName !== p.name ? el('span', { class: 'muted small', text: ` (${p.name})` }) : null,
    p.current
      ? el('span', { class: 'muted small push', text: 'This window' })
      : el('button', {
          type: 'button', class: 'ghost small push', text: p.open ? 'Show' : 'Open',
          onclick: () => { api.send('open-profile', p.id); closeModal('modal-settings'); },
        }),
  )));
}
$('btn-new-profile').onclick = () => {
  const name = $('new-profile-name').value.trim();
  if (!name) { $('new-profile-name').focus(); return; }
  api.send('new-profile', name);
  $('new-profile-name').value = '';
  closeModal('modal-settings');
};
$('new-profile-name').addEventListener('keydown', (e) => {
  if (e.key === 'Enter') { e.preventDefault(); $('btn-new-profile').click(); } // don't submit the settings form
});

function renderLogins() {
  const list = $('logins-list');
  const logins = state?.logins || [];
  $('logins-note').textContent = state?.canSavePasswords === false
    ? "This computer can't encrypt passwords for this app, so logins aren't saved."
    : 'Saved when you sign in to Xtime, PTS or ProDemand in this window, encrypted with your Windows account. Only this window uses them.';
  list.replaceChildren(...(logins.length ? logins.map((l) => el('div', { class: 'profile-row' },
    el('span', { class: 'strong', text: l.host }),
    l.username ? el('span', { class: 'muted small', text: ` · ${l.username}` }) : null,
    el('button', { type: 'button', class: 'ghost small push', text: 'Remove', onclick: () => api.send('remove-login', l.host) }),
  )) : [el('div', { class: 'muted small', text: 'None yet — sign in once and it will be remembered.' })]));
}

$('btn-settings').onclick = () => {
  const f = $('form-settings');
  for (const [k, v] of Object.entries(state?.settings || {})) {
    const input = f.elements[k];
    if (!input) continue;
    if (input.type === 'checkbox') input.checked = !!v; else input.value = v;
  }
  f.elements.profileName.value = state?.profile?.name || '';
  $('employee-status').textContent = employeeText();
  renderProfiles();
  renderLogins();
  openModal('modal-settings');
};
$('form-settings').onsubmit = (e) => {
  e.preventDefault();
  const out = {};
  for (const input of e.target.elements) {
    if (!input.name) continue;
    out[input.name] = input.type === 'checkbox' ? input.checked : input.value;
  }
  api.send('save-settings', out);
  closeModal('modal-settings');
};

// VIN check digit (North American VINs)
const VAL = { A:1,B:2,C:3,D:4,E:5,F:6,G:7,H:8,J:1,K:2,L:3,M:4,N:5,P:7,R:9,S:2,T:3,U:4,V:5,W:6,X:7,Y:8,Z:9 };
const W = [8,7,6,5,4,3,2,10,0,9,8,7,6,5,4,3,2];
function vinProblem(v) {
  if (!/^[A-HJ-NPR-Z0-9]{17}$/.test(v)) return 'A VIN is 17 letters and numbers (no I, O or Q).';
  let sum = 0;
  for (let i = 0; i < 17; i++) sum += (/\d/.test(v[i]) ? +v[i] : VAL[v[i]]) * W[i];
  return v[8] === (sum % 11 === 10 ? 'X' : String(sum % 11)) ? '' : 'That VIN fails its check digit. Double-check it.';
}

// "Add vehicle": the VIN is optional; left blank, the app gets it from Xtime
let addingRO = null; // RO being looked up in Xtime while the dialog waits
$('btn-add-vehicle').onclick = () => {
  $('form-add').reset(); $('add-error').textContent = ''; $('add-error').className = 'error';
  addingRO = null; openModal('modal-add');
};
$('form-add').onsubmit = (e) => {
  e.preventDefault();
  const d = Object.fromEntries(new FormData(e.target));
  const ro = d.ro.trim();
  const vin = d.vin.trim().toUpperCase();
  if (vin) {
    const problem = vinProblem(vin);
    if (problem) { $('add-error').textContent = problem; return; }
  }
  if (!vin) addingRO = ro; // wait for the Xtime lookup before closing
  api.send('add-vehicle', { ro, vin, ymm: d.ymm.trim() });
  if (vin) closeModal('modal-add');
};
document.querySelector('#form-add [data-close]').addEventListener('click', () => { if (addingRO) api.send('cancel-add'); addingRO = null; });
function renderAddLookup() {
  if (!addingRO || $('modal-add').hidden) return;
  const a = state?.addLookup;
  const box = $('add-error');
  if (a && a.ro === addingRO) {
    box.textContent = a.message || '';
    box.className = a.status === 'error' ? 'error' : 'muted small';
    $('form-add').querySelector('button[type=submit]').disabled = a.status === 'loading';
  } else if (state?.workspaces.some((w) => w.ro === addingRO)) {
    addingRO = null; // found and opened
    $('form-add').querySelector('button[type=submit]').disabled = false;
    closeModal('modal-add');
  }
}

// Live clock: update running times every second without a full redraw
setInterval(() => {
  if (!state) return;
  renderTimer();
  renderXtimeAlert();
}, 1000);

let noticeTimer = null;
api.onNotice?.((n) => {
  const box = $('notice');
  box.replaceChildren(el('span', { text: n.text }));
  if (n.removeHost) {
    box.append(el('button', { class: 'ghost small', text: 'Remove', title: "Don't keep this login",
      onclick: () => { api.send('remove-login', n.removeHost); box.hidden = true; } }));
  }
  box.hidden = false;
  clearTimeout(noticeTimer);
  noticeTimer = setTimeout(() => { box.hidden = true; }, n.removeHost ? 15000 : 5000);
});

$('btn-side').onclick = () => api.send('toggle-sidebar');
document.addEventListener('keydown', (e) => {
  if (e.ctrlKey && !e.shiftKey && !e.altKey && e.key.toLowerCase() === 'b' && inVehicle(state) && !document.body.classList.contains('no-ws')) {
    e.preventDefault(); api.send('toggle-sidebar');
  }
});
api.onState((s) => { state = s; render(); renderAddLookup(); maybeShowSetup(); });
api.send('ui-ready');


// ---------- Feedback: bug reports & ideas (and, for the app's admin, everyone's) ----------
const FB_STATUS = { new: 'New', planned: 'Planned', 'in-progress': 'Working on it', done: 'Done', 'wont-do': "Won't do" };
const fbDate = (t) => new Date(t).toLocaleDateString([], { month: 'short', day: 'numeric', year: 'numeric' });
let fbTab = 'send';
let fbAllFilter = 'open';
let fbAll = [];
function fbShow(tab) {
  fbTab = tab;
  document.querySelectorAll('[data-fbtab]').forEach((b) => b.classList.toggle('on', b.dataset.fbtab === tab));
  $('fb-send').hidden = tab !== 'send';
  $('fb-mine').hidden = tab !== 'mine';
  $('fb-all').hidden = tab !== 'all';
  if (tab === 'mine') { $('fb-mine').replaceChildren(el('div', { class: 'muted', text: 'Loading…' })); api.send('feedback-mine'); }
  if (tab === 'all') { $('fb-all').replaceChildren(el('div', { class: 'muted', text: 'Loading…' })); api.send('feedback-all'); }
}
document.querySelectorAll('[data-fbtab]').forEach((b) => { b.onclick = () => fbShow(b.dataset.fbtab); });
$('btn-feedback').onclick = () => {
  const ws = typeof activeWs === 'function' ? activeWs() : null;
  const sub = ws?.sub?.tabs.find((t) => t.id === ws.sub.active);
  const top = ws?.tabs.find((t) => t.id === ws.activeTab);
  $('fb-screen').textContent = state?.mode !== 'manual' ? 'Xtime' : ['Workbench', top?.title, top?.kind === 'service' ? sub?.title : ''].filter(Boolean).join(' › ');
  $('fb-sent').textContent = '';
  openModal('modal-feedback');
  fbShow(state?.feedback?.admin && state.feedback.newCount ? 'all' : 'send');
  if (fbTab === 'send') setTimeout(() => $('fb-send').elements.title.focus(), 0);
};
$('fb-send').addEventListener('submit', (e) => {
  e.preventDefault();
  const f = $('fb-send');
  if (!f.elements.title.value.trim()) { f.elements.title.focus(); return; }
  $('fb-submit').disabled = true;
  $('fb-sent').textContent = 'Sending…';
  api.send('feedback-send', { kind: f.elements.kind.value, title: f.elements.title.value, body: f.elements.body.value, screen: $('fb-screen').textContent });
});

function fbStatusChip(st) { return el('span', { class: `fb-status ${st}`, text: FB_STATUS[st] || st }); }

function fbMine(list) {
  if (!list.length) return [el('div', { class: 'muted fb-empty', text: "You haven't sent anything yet." })];
  return list.map((r) => el('div', { class: 'fb-item' },
    el('div', { class: 'fb-top' }, el('span', { class: `fb-kind-tag ${r.kind}`, text: r.kind === 'bug' ? 'Bug' : 'Idea' }),
      el('b', { text: r.title }), el('span', { class: 'fb-grow' }), fbStatusChip(r.status), el('span', { class: 'muted small', text: fbDate(r.created_at) })),
    r.reply ? el('div', { class: 'fb-reply' }, el('b', { text: 'Reply: ' }), r.reply) : null));
}

function fbAllView() {
  const pane = $('fb-all');
  const shown = fbAll.filter((r) => fbAllFilter === 'all' || (fbAllFilter === 'open' ? !['done', 'wont-do'].includes(r.status)
    : fbAllFilter === 'new' ? r.status === 'new' : r.kind === fbAllFilter));
  const filter = el('select', { class: 'fb-filter', onchange: (e) => { fbAllFilter = e.target.value; fbAllView(); } },
    ...[['open', 'Open'], ['new', 'New'], ['bug', 'Bugs'], ['feature', 'Ideas'], ['all', 'Everything']].map(([v, t]) => {
      const o = el('option', { value: v, text: t }); o.selected = v === fbAllFilter; return o; }));
  const items = shown.map((r) => {
    const status = el('select', { class: 'fb-status-pick' }, ...Object.entries(FB_STATUS).map(([v, t]) => { const o = el('option', { value: v, text: t }); o.selected = v === r.status; return o; }));
    status.onchange = () => api.send('feedback-update', { id: r.id, status: status.value });
    const reply = el('textarea', { rows: '2', maxlength: '3000', placeholder: 'Reply (shown to them in "My reports")' });
    reply.value = r.reply || '';
    return el('div', { class: `fb-item${r.status === 'new' ? ' is-new' : ''}` },
      el('div', { class: 'fb-top' }, el('span', { class: `fb-kind-tag ${r.kind}`, text: r.kind === 'bug' ? 'Bug' : 'Idea' }),
        el('b', { text: r.title }), el('span', { class: 'fb-grow' }), status),
      el('div', { class: 'fb-meta muted small', text: [r.name || 'Someone', fbDate(r.created_at), r.version && `v${r.version}`, r.screen, r.platform].filter(Boolean).join(' · ') }),
      r.body ? el('div', { class: 'fb-body', text: r.body }) : null,
      el('div', { class: 'fb-actions' }, reply,
        el('div', { class: 'fb-btns' },
          el('button', { class: 'ghost small', type: 'button', text: 'Save reply', onclick: () => api.send('feedback-update', { id: r.id, reply: reply.value }) }),
          el('button', { class: 'ghost small danger', type: 'button', text: 'Delete', onclick: (e) => {
            if (e.target.dataset.sure) { api.send('feedback-delete', r.id); return; }
            e.target.dataset.sure = '1'; e.target.textContent = 'Really delete?';
          } }))));
  });
  pane.replaceChildren(
    el('div', { class: 'fb-bar' }, el('span', { class: 'muted small', text: `${shown.length} of ${fbAll.length}` }), el('span', { class: 'fb-grow' }), filter,
      el('button', { class: 'ghost small', type: 'button', text: '↻', title: 'Refresh', onclick: () => api.send('feedback-all') })),
    ...(items.length ? items : [el('div', { class: 'muted fb-empty', text: 'Nothing here.' })]));
}

function fbKeyForm(message) {
  const input = el('input', { type: 'password', placeholder: 'Feedback password', autocomplete: 'off' });
  const form = el('form', { class: 'fb-key', onsubmit: (e) => { e.preventDefault(); if (input.value.trim()) api.send('feedback-key', input.value); } },
    el('p', { text: "Enter the Feedback password (the REPORTS_ADMIN_KEY you set in GitHub). It's kept on this computer, encrypted." }),
    message ? el('p', { class: 'fb-err', text: message }) : null,
    el('div', { class: 'row-end' }, input, el('button', { class: 'primary', type: 'submit', text: 'Unlock' })));
  setTimeout(() => input.focus(), 0);
  return form;
}

api.onFeedback?.((d) => {
  if (d.kind === 'sent') {
    $('fb-submit').disabled = false;
    if (d.ok) {
      $('fb-send').reset();
      $('fb-sent').textContent = '✓ Sent. Thanks! You can check on it under My reports.';
    } else $('fb-sent').textContent = d.message || "Couldn't send it.";
  } else if (d.kind === 'mine') {
    $('fb-mine').replaceChildren(...(d.error ? [el('div', { class: 'fb-err', text: d.error })] : fbMine(d.list || [])));
  } else if (d.kind === 'all') {
    if (d.needsKey) { $('fb-all').replaceChildren(fbKeyForm(d.message)); return; }
    if (d.error) { $('fb-all').replaceChildren(el('div', { class: 'fb-err', text: d.error })); return; }
    fbAll = d.list || [];
    fbAllView();
  }
});
