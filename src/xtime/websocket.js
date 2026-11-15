// A small WebSocket client for Xtime's chat server, in place of the browser's WebSocket (the app's Electron runs on
// Node 20, which has none built in). Text messages only, which is all XMPP over WebSocket uses.
// It goes through the same proxy as the rest of the app (the window's session says which), like Chromium would.
//   const ws = await openWebSocket(url, { protocol: 'xmpp', origin, ses })  → { send(text), close(), onmessage(text), onclose() }
// Throws when it can't connect (no route, refused, the proxy said no, not a WebSocket server).
'use strict';
const net = require('net');
const tls = require('tls');
const crypto = require('crypto');

const GUID = '258EAFA5-E914-47DA-95CA-C5AB0DC85B11';

// The proxy Chromium would use for this address: "DIRECT" or "PROXY host:port; …" (from the session's settings)
async function proxyFor(ses, url) {
  try {
    const p = String(await ses.resolveProxy(url.replace(/^wss:/, 'https:')));
    const m = p.match(/(?:PROXY|HTTPS)\s+([^\s;:]+):(\d+)/i);
    return m ? { host: m[1], port: Number(m[2]) } : null;
  } catch { return null; }
}

function connectSocket(host, port, proxy, timeout) {
  return new Promise((resolve, reject) => {
    const fail = (e) => { try { raw.destroy(); } catch {} reject(e); };
    const timer = setTimeout(() => fail(new Error('timed out connecting')), timeout);
    const secure = (sock) => {
      const t = tls.connect({ socket: sock, host, servername: host, port }, () => { clearTimeout(timer); resolve(t); });
      t.once('error', (e) => { clearTimeout(timer); reject(e); });
    };
    let raw;
    if (!proxy) { raw = net.connect(port, host); raw.once('connect', () => secure(raw)); raw.once('error', (e) => { clearTimeout(timer); fail(e); }); return; }
    // through the proxy: ask it for a tunnel first
    raw = net.connect(proxy.port, proxy.host);
    raw.once('error', (e) => { clearTimeout(timer); fail(e); });
    raw.once('connect', () => {
      raw.write(`CONNECT ${host}:${port} HTTP/1.1\r\nHost: ${host}:${port}\r\n\r\n`);
      let head = '';
      const onData = (d) => {
        head += d.toString('latin1');
        if (!head.includes('\r\n\r\n')) return;
        raw.removeListener('data', onData);
        if (!/^HTTP\/1\.\d 200/.test(head)) { clearTimeout(timer); return fail(new Error(`the proxy refused (${head.split('\r\n')[0]})`)); }
        secure(raw);
      };
      raw.on('data', onData);
    });
  });
}

async function openWebSocket(url, { protocol, origin, ses, timeout = 15000 } = {}) {
  const u = new URL(url);
  if (u.protocol !== 'wss:') throw new Error('only secure (wss:) chat addresses are used');
  const host = u.hostname;
  const port = Number(u.port) || 443;
  const proxy = ses ? await proxyFor(ses, url) : null;
  const sock = await connectSocket(host, port, proxy, timeout);

  // the handshake: an HTTP request asking to switch to WebSocket
  const key = crypto.randomBytes(16).toString('base64');
  sock.write([`GET ${u.pathname}${u.search} HTTP/1.1`, `Host: ${host}${port === 443 ? '' : `:${port}`}`, 'Upgrade: websocket', 'Connection: Upgrade',
    `Sec-WebSocket-Key: ${key}`, 'Sec-WebSocket-Version: 13', protocol ? `Sec-WebSocket-Protocol: ${protocol}` : '', origin ? `Origin: ${origin}` : '',
    'User-Agent: Mozilla/5.0 (Windows NT 10.0; Win64; x64)', '', ''].filter((l, i, a) => l || i >= a.length - 2).join('\r\n'));

  let buf = Buffer.alloc(0);
  await new Promise((resolve, reject) => {
    const timer = setTimeout(() => { sock.destroy(); reject(new Error('the chat server didn’t answer')); }, timeout);
    const onData = (d) => {
      buf = Buffer.concat([buf, d]);
      const end = buf.indexOf('\r\n\r\n');
      if (end < 0) return;
      sock.removeListener('data', onData);
      clearTimeout(timer);
      const head = buf.slice(0, end).toString('latin1');
      buf = buf.slice(end + 4);
      const accept = (head.match(/^sec-websocket-accept:\s*(.+)$/im) || [])[1]?.trim();
      const want = crypto.createHash('sha1').update(key + GUID).digest('base64');
      if (!/^HTTP\/1\.1 101/.test(head) || accept !== want) { sock.destroy(); return reject(new Error(`not a WebSocket answer (${head.split('\r\n')[0]})`)); }
      resolve();
    };
    sock.on('data', onData);
    sock.once('error', (e) => { clearTimeout(timer); reject(e); });
  });

  const ws = { onmessage: null, onclose: null, closed: false };
  // frames to the server are always masked (the WebSocket rule for clients)
  const frame = (opcode, payload) => {
    const len = payload.length;
    const head = len < 126 ? Buffer.from([0x80 | opcode, 0x80 | len])
      : len < 65536 ? Buffer.from([0x80 | opcode, 0x80 | 126, len >> 8, len & 255])
        : Buffer.concat([Buffer.from([0x80 | opcode, 0x80 | 127]), (() => { const b = Buffer.alloc(8); b.writeBigUInt64BE(BigInt(len)); return b; })()]);
    const mask = crypto.randomBytes(4);
    const body = Buffer.from(payload);
    for (let i = 0; i < body.length; i++) body[i] ^= mask[i & 3];
    return Buffer.concat([head, mask, body]);
  };
  ws.send = (text) => { if (!ws.closed) sock.write(frame(1, Buffer.from(String(text), 'utf8'))); };
  const end = () => { if (ws.closed) return; ws.closed = true; try { ws.onclose?.(); } catch {} };
  ws.close = () => {
    if (ws.closed) return;
    try { sock.write(frame(8, Buffer.from([0x03, 0xe8]))); } catch {}
    setTimeout(() => sock.destroy(), 200);
    end();
  };

  // frames from the server: text (and its continuations), ping → pong, close
  let parts = [];
  const read = () => {
    while (buf.length >= 2) {
      const fin = (buf[0] & 0x80) !== 0;
      const opcode = buf[0] & 0x0f;
      const masked = (buf[1] & 0x80) !== 0;
      let len = buf[1] & 0x7f;
      let at = 2;
      if (len === 126) { if (buf.length < 4) return; len = buf.readUInt16BE(2); at = 4; }
      else if (len === 127) { if (buf.length < 10) return; len = Number(buf.readBigUInt64BE(2)); at = 10; }
      const maskKey = masked ? buf.slice(at, at + 4) : null;
      if (masked) at += 4;
      if (buf.length < at + len) return;
      let payload = buf.slice(at, at + len);
      buf = buf.slice(at + len);
      if (maskKey) { payload = Buffer.from(payload); for (let i = 0; i < payload.length; i++) payload[i] ^= maskKey[i & 3]; }
      if (opcode === 8) { sock.destroy(); end(); return; }
      if (opcode === 9) { try { sock.write(frame(10, payload)); } catch {} continue; }
      if (opcode === 10) continue;
      if (opcode === 1 || opcode === 2 || opcode === 0) {
        parts.push(payload);
        if (fin) { const text = Buffer.concat(parts).toString('utf8'); parts = []; try { ws.onmessage?.(text); } catch {} }
      }
    }
  };
  sock.on('data', (d) => { buf = Buffer.concat([buf, d]); read(); });
  sock.on('close', end);
  sock.on('error', end);
  if (buf.length) setImmediate(read); // anything that came with the handshake
  return ws;
}

module.exports = { openWebSocket };
