// PTS Workbench sync server.
//
//   GET /v1/store/<id>                 -> { version, data }   (version 0 and no data if new)
//   PUT /v1/store/<id>  { base, data } -> { version }          saved
//                                      -> 409 { version, data } someone else saved first: merge and retry
//
// The data is encrypted on each computer before it is sent, so this server only ever stores
// an opaque ID and unreadable text. Conflicts are handled by the app: when two computers save
// at the same time, the second gets 409 with the newer copy, merges it with its own, and tries again.

const MAX_BYTES = 4 * 1024 * 1024; // plenty for years of flags
const ID = /^[a-f0-9]{64}$/;

const json = (body, status = 200) => new Response(JSON.stringify(body), {
  status,
  headers: { 'content-type': 'application/json', 'cache-control': 'no-store' },
});

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    if (url.pathname === '/' || url.pathname === '/health') return json({ ok: true, service: 'pts-workbench-sync' });

    const m = url.pathname.match(/^\/v1\/store\/([^/]+)$/);
    if (!m) return json({ error: 'not found' }, 404);
    const id = m[1];
    if (!ID.test(id)) return json({ error: 'bad id' }, 400);

    if (request.method === 'GET') {
      const row = await env.DB.prepare('SELECT version, data FROM stores WHERE id = ?').bind(id).first();
      return json(row ? { version: row.version, data: row.data } : { version: 0, data: null });
    }

    if (request.method === 'PUT') {
      const length = Number(request.headers.get('content-length') || 0);
      if (length > MAX_BYTES) return json({ error: 'too large' }, 413);
      let body;
      try { body = await request.json(); } catch { return json({ error: 'bad json' }, 400); }
      const base = Number(body?.base);
      if (!Number.isInteger(base) || base < 0 || typeof body?.data !== 'string' || body.data.length > MAX_BYTES) {
        return json({ error: 'bad request' }, 400);
      }
      const now = Date.now();
      // Save only if nobody else saved since this computer last read (version still equals base)
      const result = base === 0
        ? await env.DB.prepare('INSERT INTO stores (id, version, data, updated_at) VALUES (?, 1, ?, ?) ON CONFLICT(id) DO NOTHING')
          .bind(id, body.data, now).run()
        : await env.DB.prepare('UPDATE stores SET version = version + 1, data = ?, updated_at = ? WHERE id = ? AND version = ?')
          .bind(body.data, now, id, base).run();
      if (result.meta.changes === 1) return json({ version: base + 1 });
      const row = await env.DB.prepare('SELECT version, data FROM stores WHERE id = ?').bind(id).first();
      return json(row ? { version: row.version, data: row.data } : { version: 0, data: null }, 409);
    }

    return json({ error: 'method not allowed' }, 405);
  },
};