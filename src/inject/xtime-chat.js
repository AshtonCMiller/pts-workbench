// Xtime's Dealership chat, with the same requests Xtime's Dealership message center makes.
// Runs either in Xtime's full Inspect page or in the app's small background page on the Inspect site
// (no Xtime page loaded). In the small page, Xtime's settings come from CFG (saved the last time the full
// page was seen), and the chat token is asked for the same way Xtime's page asks for it.
//   { type: 'list', days, ros, roDays } → your dealership messages from the last `days` days, plus older
//                                          messages on the ROs in `ros` (your assigned ROs) back `roDays` days
//   { type: 'history', channelId }       → one conversation's messages
//   { type: 'count' }                    → how many dealership messages you haven't read
//   { type: 'roster' }                   → people you can @mention
//   { type: 'send', channelId, ro, text } → send one message to an existing conversation (only when you press Send)
//   { type: 'read', channelId }          → mark your unread messages in a conversation read (when you open it)
// Customer chats are left out.
(async (ACTION, CFG) => {
  // ---- Xtime's chat settings (passed to its React chat components through Angular) ----
  const findConfig = () => {
    const seen = new Set();
    const look = (o, depth) => {
      if (!o || typeof o !== 'object' || seen.has(o) || depth > 4) return null;
      seen.add(o);
      if ((typeof o.dealerCode === 'string' || typeof o.dealerCode === 'number') && String(o.dealerCode)) return o;
      for (const k of Object.keys(o)) {
        if (k.startsWith('$') || k === 'parent') continue;
        let v; try { v = o[k]; } catch { continue; }
        const hit = look(v, depth + 1);
        if (hit) return hit;
      }
      return null;
    };
    const ng = window.angular;
    let best = null;
    if (!ng) return null;
    for (const el of document.querySelectorAll('react-component[props]')) {
      try {
        const scope = ng.element(el).scope();
        const props = scope && scope.$eval(el.getAttribute('props'));
        const hit = look(props, 0);
        if (hit && hit.userData && hit.userData.userName) return hit;
        best = best || hit;
      } catch { /* next one */ }
    }
    return best;
  };
  const found = findConfig();
  const cfg = found
    ? { dealerCode: String(found.dealerCode), userName: String((found.userData && found.userData.userName) || ''), env: found.env || '',
      chatgatewayUrl: found.chatgatewayUrl || '', storeId: String(found.storeId || (CFG && CFG.storeId) || '') }
    : (CFG && CFG.dealerCode && CFG.userName ? CFG : null);
  if (!cfg) return { ok: false, needPage: true, error: "Xtime's chat settings aren't known yet (open Xtime's page once)" };
  const config = found ? cfg : undefined; // handed back so the app can work without the full page next time
  const userName = String(cfg.userName || '').toLowerCase();
  if (!userName) return { ok: false, needPage: true, error: "Couldn't find your Xtime username" };
  const dealerRaw = String(cfg.dealerCode);
  const dealerCode = encodeURIComponent(dealerRaw);
  const envPart = cfg.env && cfg.env !== 'prod' && cfg.env !== 'production' ? cfg.env : '';
  const base = (cfg.chatgatewayUrl || `https://chatgateway${envPart}.xtime.com`).replace(/\/?$/, '/');

  // ---- the chat token: Xtime's page keeps it in sessionStorage; otherwise ask Xtime's login service ----
  const expired = (t) => {
    try {
      const p = JSON.parse(atob(String(t.accessToken).split('.')[1].replace(/-/g, '+').replace(/_/g, '/')));
      return !p.exp || p.exp * 1000 < Date.now() + 60000;
    } catch { return false; }
  };
  // signedOut: Xtime's sign-in itself is too old (its login service won't give a chat token, or the chat turns the
  // token down). Xtime's own page shows the same thing: chat is the first to stop. The app then signs in again.
  let signedOut = false;
  const readToken = () => { try { const t = JSON.parse(sessionStorage.getItem('xsToken') || 'null'); return t && t.accessToken && !expired(t) ? t : null; } catch { return null; } };
  const newToken = async () => {
    try {
      const res = await fetch(`https://login${envPart}.xtime.app.coxautoinc.com/rest/login/jwt/xsession`, {
        method: 'POST', credentials: 'include', headers: { 'Content-Type': 'application/json', Accept: 'application/json' }, body: 'null',
      });
      if (!res.ok) { if (res.status === 401 || res.status === 403) signedOut = true; return null; }
      if (/signin\.coxautoinc\.com|\/login(\b|\/|\?)/i.test(res.url || '') && !/\/rest\/login\//i.test(res.url || '')) { signedOut = true; return null; } // sent to sign in
      let j = null;
      try { j = await res.json(); } catch { signedOut = true; return null; } // a sign-in page instead of a token
      const t = (j && j.data && j.data.accessToken ? j.data : j && j.content && j.content.data) || (j && j.accessToken ? j : null);
      if (!t || !t.accessToken) { signedOut = true; return null; }
      try { sessionStorage.setItem('xsToken', JSON.stringify(t)); } catch {}
      return t;
    } catch { return null; }
  };
  let token = readToken() || await newToken();
  if (!token) return signedOut ? { ok: false, signedOut: true, error: "Xtime's sign-in is too old for chat: signing in again" }
    : { ok: false, needPage: true, error: "Couldn't get Xtime's chat sign-in (open Xtime's page once)" };

  const post = async (path, query, body, retried) => {
    let res;
    try {
      res = await fetch(base + path + query, {
        method: 'POST',
        headers: { Authorization: `Bearer ${token.accessToken}`, 'Content-Type': 'application/json', Accept: 'application/json' },
        body: JSON.stringify(body),
      });
    } catch (e) { return { ok: false, error: `Chat request failed: ${e.message}` }; }
    if (res.status === 401 && !retried) {
      try { sessionStorage.removeItem('xsToken'); } catch {}
      token = await newToken();
      if (token) return post(path, query, body, true);
    }
    if (res.status === 401) return { ok: false, signedOut: true, error: "Xtime's sign-in is too old for chat: signing in again" };
    if (!res.ok) return { ok: false, error: `Chat returned HTTP ${res.status}` };
    try { return { ok: true, data: await res.json() }; } catch { return { ok: false, error: "Chat didn't send readable data" }; }
  };

  // Strophe.escapeNode, as Xtime uses for usernames
  const escapeNode = (s) => s.trim().replace(/\\/g, '\\5c').replace(/ /g, '\\20').replace(/"/g, '\\22').replace(/&/g, '\\26')
    .replace(/'/g, '\\27').replace(/\//g, '\\2f').replace(/:/g, '\\3a').replace(/</g, '\\3c').replace(/>/g, '\\3e').replace(/@/g, '\\40');
  const unescapeNode = (s) => s.replace(/\\(5c|20|22|26|27|2f|3a|3c|3e|40)/gi, (_, h) => String.fromCharCode(parseInt(h, 16)));
  const userOf = (jid) => unescapeNode(String(jid || '').replace(/\/.*$/, '').replace(/@.*$/, ''));
  // Real names from Xtime's employee list (on the full page, or loaded from the Inspect site once).
  // Chat usernames look like "amiller@s3590"; the list has "amiller", "tbencsek_xtime" or
  // "aking__S3590_xtime", so both are compared bare. Kept only in this page's memory.
  const bareUser = (u) => String(u || '').toLowerCase().trim().replace(/@s\d+$/, '').replace(/__s\d+_xtime$/, '').replace(/_xtime$/, '');
  let employees = Array.isArray(window.Employees) ? window.Employees : window.__ptsEmployees;
  if (!Array.isArray(employees) && cfg.storeId && ACTION && ACTION.type !== 'count') {
    try {
      const res = await fetch(`/JS/Employees?storeID=${encodeURIComponent(cfg.storeId)}`, { credentials: 'include' });
      const text = res.ok ? await res.text() : '';
      const a = text.indexOf('['), b = text.lastIndexOf(']');
      if (a >= 0 && b > a) { employees = JSON.parse(text.slice(a, b + 1)); window.__ptsEmployees = employees; }
    } catch {}
  }
  const names = new Map();
  try {
    const list = Array.isArray(employees) ? employees : [];
    const full = (e) => [e.FirstName, e.LastName].filter(Boolean).join(' ').trim();
    // usernames first; an email name only when no username claims it (some people share an email)
    for (const e of list) for (const k of [e.Username, e.XtimeUsername]) if (k && full(e) && (!names.has(bareUser(k)) || e.Active)) names.set(bareUser(k), full(e));
    for (const e of list) { const k = bareUser(String(e.Email || '').split('@')[0]); if (k && full(e) && !names.has(k)) names.set(k, full(e)); }
  } catch {}
  const prettyName = (u) => {
    const bare = bareUser(u);
    if (names.has(bare)) return names.get(bare);
    return bare.split(/[._\s]+/).filter(Boolean).map((w) => w[0].toUpperCase() + w.slice(1)).join(' ');
  };
  const arrayIn = (d) => {
    if (Array.isArray(d)) return d;
    if (!d || typeof d !== 'object') return [];
    for (const k of ['content', 'messages', 'data', 'results', 'items', 'history']) if (Array.isArray(d[k])) return d[k];
    return Object.values(d).find((v) => Array.isArray(v)) || [];
  };
  // Message text: Xtime stores markdown-ish links and @mentions
  // Message text. Xtime stores @mentions as markdown links: [Brenton\20Skeeters](jid|bskeeters\40s3590)
  // (also jid_group for groups), and attachments/links as [caption](type:…|url).
  const mentionName = (txt, user) => {
    const t = unescapeNode(String(txt || '').replace(/^@/, '')).trim();
    return t || prettyName(userOf(user || ''));
  };
  const plain = (t) => {
    const mentions = [];
    const text = String(t || '')
      .replace(/@?\[([^\]]*)\]\((jid|jid_group)\|([^)]*)\)/g, (_, txt, kind, user) => { const n = mentionName(txt, user); mentions.push(n); return `@${n}`; })
      .replace(/\[([^\]]*)\]\(([^)]+)\)/g, (_, txt, url) => (txt && !/^https?:/i.test(txt) ? txt : url.split('|').pop()))
      .replace(/\\([\\`*_{}\[\]()#+\-.!>~|])/g, '$1')
      .replace(/&amp;/g, '&').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&quot;/g, '"').replace(/&#39;/g, "'")
      .trim();
    return { text, mentions };
  };
  const roFromChannel = (id) => { const m = /([0-9]+)-([0-9]+)-([0-9]+)/.exec(id || ''); return m ? m[2] : ''; };

  // One raw history record → a plain message. Raw records hold the XMPP stanza as text (chatmsg).
  const parser = new DOMParser();
  const norm = (raw) => {
    let from = '', body = '', ro = '', notification = false;
    if (raw.chatmsg) {
      const doc = parser.parseFromString(raw.chatmsg, 'text/xml').documentElement;
      if (doc && !doc.getElementsByTagName('parsererror').length) {
        from = doc.getAttribute('from') || '';
        const b = [...doc.children].find((n) => n.localName === 'body');
        body = b ? b.textContent : '';
        for (const c of doc.getElementsByTagName('xtctx')) {
          const kid = (name) => [...c.children].find((n) => n.localName === name)?.textContent || '';
          const ns = c.getAttribute('namespace') || kid('namespace');
          const id = c.getAttribute('contextId') || kid('contextId');
          if (ns === 'RO' && id) ro = id;
        }
        notification = doc.getElementsByTagName('notification').length > 0;
      }
    }
    // Some bodies are JSON ({ body, attachments })
    if (/^\s*\{/.test(body)) { try { const j = JSON.parse(body); if (j && typeof j.body === 'string') body = j.body; } catch {} }
    const sentByMe = raw.dir === 'send' || bareUser(userOf(from)) === bareUser(userName);
    const c = raw.createdAt ? String(raw.createdAt) : '';
    const created = c ? new Date(/^\d+$/.test(c) ? Number(c) : c.replace(' ', 'T') + (/[zZ]$|[+-]\d\d:?\d\d$/.test(c) ? '' : 'Z')) : null;
    return {
      id: String(raw.archiveId ?? raw.id ?? ''),
      channelId: String(raw.channelId || ''),
      ro: ro || roFromChannel(raw.channelId),
      sender: prettyName(userOf(from)) || (notification ? 'Xtime' : ''),
      mine: sentByMe,
      ...plain(body),
      time: created && !Number.isNaN(created.getTime()) ? created.getTime() : null,
      unread: !sentByMe && !/^(read|seen)$/i.test(String(raw.status ?? raw.state ?? raw.readStatus ?? '')),
      notification,
    };
  };
  const fields = (m) => (m ? Object.keys(m).map((k) => (m[k] && typeof m[k] === 'object' && !Array.isArray(m[k])) ? `${k}{${Object.keys(m[k]).join(',')}}` : k) : []);

  const a = ACTION || { type: 'list' };
  if (a.type === 'history') {
    const r = await post(`chatgateway/rest/data/history?uri=/chat/channel/history/pages?dealerCode=${dealerCode}`,
      encodeURIComponent('&limit=200'),
      { context: { DEALER_CODE: dealerRaw }, includeNotification: true, channelIds: [a.channelId], chatType: 'dealerChat' });
    if (!r.ok) return r;
    const raw = arrayIn(r.data);
    return { ok: true, config, channelId: a.channelId, messages: raw.map(norm).filter((m) => m.text), fieldNames: fields(raw[0]) };
  }

  // ---- the people you can @mention (Xtime's chat roster; Xtime's employee list as a fallback) ----
  const get = async (path, query) => {
    try {
      const res = await fetch(base + path + query, { headers: { Authorization: `Bearer ${token.accessToken}`, Accept: 'application/json' } });
      if (res.status === 401) return { ok: false, signedOut: true, error: "Xtime's sign-in is too old for chat: signing in again" };
      if (!res.ok) return { ok: false, error: `Chat returned HTTP ${res.status}` };
      return { ok: true, data: await res.json() };
    } catch (e) { return { ok: false, error: `Chat request failed: ${e.message}` }; }
  };
  if (a.type === 'roster') {
    const me = bareUser(userName);
    const people = new Map(); // id → name
    const add = (local, name) => {
      local = String(local || '').toLowerCase().replace(/@chat\.xtime\.com$/, '');
      if (!local || !name || bareUser(local) === me) return;
      people.set(`jid|${escapeNode(local)}`, name);
    };
    const r = await get('chatgateway/rest/roster/users', `?dealerCode=${dealerCode}`);
    if (r.ok) {
      for (const u of arrayIn(r.data)) {
        if (!u || typeof u !== 'object') continue;
        const jid = unescapeNode(String(u.jid || u.username || u.userName || u.user || ''));
        const local = jid.replace(/@chat\.xtime\.com$/i, '');
        add(local, String(u.name || u.displayName || u.fullName || '').trim() || prettyName(local));
      }
    }
    if (!people.size && Array.isArray(employees)) {
      for (const e of employees) {
        if (!e || e.Active === false || !e.Username || /api|bdc/i.test(String(e.EmployeeType && e.EmployeeType.Name || ''))) continue;
        const full = [e.FirstName, e.LastName].filter(Boolean).join(' ').trim();
        add(`${bareUser(e.Username)}@s${e.StoreID || cfg.storeId}`, full);
      }
    }
    return { ok: true, config, people: [...people].map(([id, name]) => ({ id, name })).sort((x, y) => x.name.localeCompare(y.name)) };
  }

  // ---- a short connection to Xtime's chat server (XMPP over WebSocket), like Xtime's page makes:
  //      open → sign in (SASL, with the chat token) → bind → send the stanzas → close ----
  const DOMAIN = 'chat.xtime.com';
  const esc = (v) => String(v).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;').replace(/'/g, '&apos;');
  const uuid = () => (crypto.randomUUID ? crypto.randomUUID() : `${Date.now()}-${Math.random().toString(16).slice(2)}`);
  const xmppSend = async (stanzas, watchIds = []) => {
    // Where Xtime's chat server is (Xtime's login service tells its page the same way)
    let server = '';
    try { server = sessionStorage.getItem('xtEjdUrl') || ''; } catch {}
    if (!server) {
      try {
        const res = await fetch(`https://login${envPart}.xtime.app.coxautoinc.com/rest/login/ejdUrl`, { credentials: 'include', headers: { Authorization: `Bearer ${token.accessToken}` } });
        if (res.ok) { server = (await res.text()).trim().replace(/^"|"$/g, ''); try { sessionStorage.setItem('xtEjdUrl', server); } catch {} }
      } catch {}
    }
    const m = /https:\/\/(.*)\/http-bind/.exec(server);
    const wsUrl = m ? `wss://${m[1]}:443/ws` : (/^wss:/.test(server) ? server : '');
    if (!wsUrl) return { ok: false, error: "Couldn't find Xtime's chat server" };

    // XMPP over WebSocket: open → sign in (SASL) → bind → send → close
    const b64 = (s) => btoa(unescape(encodeURIComponent(s)));
    const unb64 = (s) => decodeURIComponent(escape(atob(s)));
    const local = escapeNode(userName);
    const jid = `${local}@${DOMAIN}`;
    const enc = new TextEncoder();
    const scram = async (hash, bits) => {
      const nonce = b64(uuid()).replace(/=/g, '');
      const first = `n=${local.replace(/=/g, '=3D').replace(/,/g, '=2C')},r=${nonce}`;
      return {
        start: b64(`n,,${first}`),
        async next(challenge) {
          const c = unb64(challenge);
          const p = Object.fromEntries(c.split(',').map((kv) => [kv[0], kv.slice(2)]));
          if (!p.r || !p.r.startsWith(nonce)) throw new Error('bad server nonce');
          const salt = Uint8Array.from(atob(p.s), (ch) => ch.charCodeAt(0));
          const key = await crypto.subtle.importKey('raw', enc.encode(token.accessToken), 'PBKDF2', false, ['deriveBits']);
          const salted = new Uint8Array(await crypto.subtle.deriveBits({ name: 'PBKDF2', salt, iterations: Number(p.i), hash }, key, bits));
          const hmac = async (k, d) => new Uint8Array(await crypto.subtle.sign('HMAC', await crypto.subtle.importKey('raw', k, { name: 'HMAC', hash }, false, ['sign']), typeof d === 'string' ? enc.encode(d) : d));
          const clientKey = await hmac(salted, 'Client Key');
          const storedKey = new Uint8Array(await crypto.subtle.digest(hash, clientKey));
          const finalNoProof = `c=biws,r=${p.r}`;
          const authMsg = `${first},${c},${finalNoProof}`;
          const sig = await hmac(storedKey, authMsg);
          const proof = clientKey.map((v, i) => v ^ sig[i]);
          return b64(`${finalNoProof},p=${btoa(String.fromCharCode(...proof))}`);
        },
      };
    };
    return new Promise((resolve) => {
      let ws;
      let step = 'open';
      let mech = null;
      let done = false;
      const finish = (r) => {
        if (done) return; done = true;
        try { ws.send('<close xmlns="urn:ietf:params:xml:ns:xmpp-framing"/>'); } catch {}
        setTimeout(() => { try { ws.close(); } catch {} }, 200);
        resolve(r);
      };
      const timer = setTimeout(() => finish({ ok: false, error: 'Xtime’s chat server didn’t answer' }), 20000);
      const openStream = () => ws.send(`<open xmlns="urn:ietf:params:xml:ns:xmpp-framing" to="${DOMAIN}" version="1.0"/>`);
      try { ws = new WebSocket(wsUrl, 'xmpp'); } catch (e) { clearTimeout(timer); resolve({ ok: false, error: `Couldn't reach Xtime's chat server: ${e.message}` }); return; }
      ws.onerror = () => { clearTimeout(timer); finish({ ok: false, error: "Couldn't reach Xtime's chat server" }); };
      ws.onclose = () => { clearTimeout(timer); finish({ ok: false, error: 'Xtime’s chat server closed the connection' }); };
      ws.onopen = openStream;
      ws.onmessage = async (ev) => {
        let el;
        try { el = parser.parseFromString(String(ev.data), 'text/xml').documentElement; } catch { return; }
        if (!el) return;
        const name = el.localName;
        try {
          if (name === 'open') return;
          if (name === 'close') { clearTimeout(timer); return finish({ ok: false, error: 'Xtime’s chat server ended the connection' }); }
          if (name === 'features') {
            if (step === 'open') {
              const offered = [...el.getElementsByTagName('mechanism')].map((x) => x.textContent.trim());
              const order = ['SCRAM-SHA-512', 'SCRAM-SHA-256', 'SCRAM-SHA-1', 'PLAIN', 'X-OAUTH2'];
              const pickM = order.find((x) => offered.includes(x));
              if (!pickM) { clearTimeout(timer); return finish({ ok: false, error: `Xtime's chat sign-in isn't supported (${offered.join(', ') || 'none offered'})` }); }
              let first;
              if (pickM.startsWith('SCRAM')) {
                const [hash, bits] = pickM === 'SCRAM-SHA-512' ? ['SHA-512', 512] : pickM === 'SCRAM-SHA-256' ? ['SHA-256', 256] : ['SHA-1', 160];
                mech = await scram(hash, bits); first = mech.start;
              } else first = b64(`\u0000${local}\u0000${token.accessToken}`);
              step = 'auth';
              ws.send(`<auth xmlns="urn:ietf:params:xml:ns:xmpp-sasl" mechanism="${pickM}">${first}</auth>`);
            } else if (step === 'restart') {
              step = 'bind';
              ws.send('<iq xmlns="jabber:client" type="set" id="bind1"><bind xmlns="urn:ietf:params:xml:ns:xmpp-bind"><resource>pts-workbench-' + uuid().slice(0, 8) + '</resource></bind></iq>');
            }
            return;
          }
          if (name === 'challenge' && mech) { ws.send(`<response xmlns="urn:ietf:params:xml:ns:xmpp-sasl">${await mech.next(el.textContent.trim())}</response>`); return; }
          if (name === 'success') { step = 'restart'; openStream(); return; }
          if (name === 'failure') { clearTimeout(timer); return finish({ ok: false, signedOut: true, error: 'Xtime’s chat server refused the sign-in' }); }
          if (name === 'iq' && el.getAttribute('id') === 'bind1') {
            if (el.getAttribute('type') !== 'result') { clearTimeout(timer); return finish({ ok: false, error: 'Xtime’s chat server refused the connection' }); }
            step = 'sent';
            for (const st of stanzas) ws.send(st);
            // Nothing comes back for a good message; an error would come back quickly
            setTimeout(() => { clearTimeout(timer); finish({ ok: true }); }, 2500);
            return;
          }
          if (name === 'message' && el.getAttribute('type') === 'error' && watchIds.includes(el.getAttribute('id'))) {
            clearTimeout(timer);
            const why = el.getElementsByTagName('text')[0]?.textContent || el.getElementsByTagName('error')[0]?.firstElementChild?.localName || 'error';
            return finish({ ok: false, error: `Xtime's chat didn't take the message (${why})` });
          }
        } catch (e) { clearTimeout(timer); finish({ ok: false, error: `Chat error: ${e.message}` }); }
      };
    });
  };

  // ---- sending one message ----
  if (a.type === 'send') {
    const text = String(a.text || '').trim();
    if (!text) return { ok: false, error: 'Nothing to send' };
    if (!a.channelId) return { ok: false, error: 'Start this conversation on Xtime’s page first' };

    // The conversation's context, as Xtime does: the latest message's (when it's for this RO), else RO + dealer code
    let ctx = null;
    const h = await post(`chatgateway/rest/data/history?uri=/chat/channel/history/pages?dealerCode=${dealerCode}`,
      encodeURIComponent('&limit=20'),
      { context: { DEALER_CODE: dealerRaw }, includeNotification: false, channelIds: [a.channelId], chatType: 'dealerChat' });
    const roNum = String(a.ro || roFromChannel(a.channelId) || '');
    if (h.ok) {
      const recs = arrayIn(h.data).filter((x) => x && x.chatmsg)
        .sort((x, y) => String(x.createdAt || '').localeCompare(String(y.createdAt || '')) || (Number(x.archiveId) || 0) - (Number(y.archiveId) || 0));
      const last = recs[recs.length - 1];
      if (last) {
        const doc = parser.parseFromString(last.chatmsg, 'text/xml').documentElement;
        const list = [...doc.getElementsByTagName('xtctx')].map((c) => ({ namespace: c.getAttribute('namespace') || '', contextId: c.getAttribute('contextId') || '' }))
          .filter((c) => c.namespace);
        const ro = list.find((c) => c.namespace === 'RO');
        if (ro && (!roNum || ro.contextId === roNum)) ctx = list;
      }
    }
    if (!ctx) ctx = [{ namespace: 'RO', contextId: roNum }, { namespace: 'DEALER_CODE', contextId: dealerRaw }];
    ctx = ctx.filter((c) => !['CHAT_MSG_ID', 'src_msgid', 'sent_msg_uuid'].includes(c.namespace)).concat([{ namespace: 'sent_msg_uuid', contextId: uuid() }]);
    const msgId = uuid();
    const stanza = `<message xmlns="jabber:client" to="${esc(`${a.channelId}+${dealerRaw}@${DOMAIN}`)}" type="chat" id="${msgId}">`
      + `<body>${esc(text)}</body><xtmsg xmlns="http://www.xtime.com/chat">`
      + ctx.map((c) => `<xtctx namespace="${esc(c.namespace)}" contextId="${esc(c.contextId)}"/>`).join('')
      + `<xtchannel xmlns="http://www.xtime.com/channel"><channel dealercode="${esc(dealerRaw)}" channel="${esc(a.channelId)}"/></xtchannel>`
      + '</xtmsg></message>';

    return { ...(await xmppSend([stanza], [msgId])), config };
  }

  // ---- read receipts: what Xtime's page sends when you open a conversation ----
  // Asks Xtime which of your messages in the conversation are still unread, then marks each one read.
  if (a.type === 'read') {
    if (!a.channelId) return { ok: false, error: 'No conversation' };
    const r = await post('chatgateway/rest/data/history?uri=/chat/getUnreadArchiveIds', `?dealerCode=${dealerCode}`,
      { context: { DEALER_CODE: dealerRaw }, usernames: [escapeNode(userName)], channelIds: [a.channelId], days: Math.max(5, Number(a.days) || 120) });
    if (!r.ok) return r;
    // [101, 102] · [{ archiveId: 101, … }] · { channelId: [101, …] } · { data: … }
    const pickIds = (d) => {
      if (d == null) return [];
      if (Array.isArray(d)) return d.flatMap((x) => (x && typeof x === 'object' ? (x.archiveId ?? x.id ?? x.messageId ?? x.archive_id) != null ? [x.archiveId ?? x.id ?? x.messageId ?? x.archive_id] : pickIds(x) : [x]));
      if (typeof d === 'object') return Object.values(d).flatMap(pickIds);
      return [d];
    };
    const shape = (d) => (Array.isArray(d) ? `[${d.length}${d.length ? ` × ${shape(d[0])}` : ''}]` : d && typeof d === 'object' ? `{${Object.keys(d).slice(0, 8).join(',')}}` : typeof d);
    const ids = [...new Set(pickIds(r.data).map(String).filter((x) => /^\d+$/.test(x)))];
    if (!ids.length) return { ok: true, config, ids: [], shape: shape(r.data) };
    const stanzas = ids.map((id) => '<message xmlns="jabber:client">'
      + `<xtreceived xmlns="urn:xtime:chatstates" id="${esc(id)}" state="read"/>`
      + '<xtmsg xmlns="http://www.xtime.com/chat"><xtchannel xmlns="http://www.xtime.com/xtchannel">'
      + `<channel dealercode="${esc(dealerRaw)}" channel="${esc(a.channelId)}"/></xtchannel></xtmsg></message>`);
    const res = await xmppSend(stanzas);
    return { ...res, config, ids: res.ok ? ids : [], tried: ids.length, shape: shape(r.data) };
  }

  if (a.type === 'count') {
    // Xtime's own unread count for the Dealership tab
    const r = await post('chatgateway/rest/data/history?uri=/chat/channel/history/count', `?dealerCode=${dealerCode}`,
      { days: 14, context: { DEALER_CODE: dealerRaw }, usernames: [escapeNode(userName)], state: 'NEW', dir: 'recv', chatType: 'dealerChat' });
    if (!r.ok) return r;
    const d = r.data;
    const sum = (v) => (typeof v === 'number' ? v : v && typeof v === 'object' ? Object.values(v).reduce((t, x) => t + (typeof x === 'number' ? x : 0), 0) : 0);
    const n = typeof d === 'number' ? d : d && typeof d.count === 'number' ? d.count : sum(d && (d.data || d.content || d));
    return { ok: true, config, count: n };
  }
  // Your dealership messages (what Xtime's Dealership tab loads, over a longer window)
  const days = Math.max(1, Number(a.days) || 14);
  const roDays = Math.max(days, Number(a.roDays) || 0);
  const r = await post('chatgateway/rest/data/history?uri=/chat/channel/history', `?dealerCode=${dealerCode}`,
    { usernames: [escapeNode(userName)], context: { DEALER_CODE: dealerRaw }, days: roDays, includeNotification: true, chatType: 'dealerChat' });
  if (!r.ok) return r;
  const raw = arrayIn(r.data);
  const mine = new Set((a.ros || []).map(String));
  const since = Date.now() - days * 86400000;
  const all = raw.map(norm).filter((m) => m.text && ((m.time || 0) >= since || mine.has(m.ro)));
  return { ok: true, config, conversations: all, fieldNames: fields(raw[0]) };
})
