// PTS Workbench sync server.
//
//   GET /v1/store/<id>                 -> { version, data }   (version 0 and no data if new)
//   PUT /v1/store/<id>  { base, data } -> { version }          saved
//                                      -> 409 { version, data } someone else saved first: merge and retry
//
//   POST   /v1/reports                 { kind, title, body, name, reporter, version, platform, screen } -> { id }
//   GET    /v1/reports/mine?reporter=… -> { reports }  what this computer sent, with status and reply
//   GET    /v1/reports                 -> { reports }  everything        (admin: Authorization: Bearer ADMIN_KEY)
//   PATCH  /v1/reports/<id>  { status?, reply? }                          (admin)
//   DELETE /v1/reports/<id>                                               (admin)
//
// Sync data is encrypted on each computer before it is sent, so this server only ever stores
// an opaque ID and unreadable text for it. Conflicts are handled by the app: when two computers save
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

    if (url.pathname.startsWith('/v1/reports')) return reports(request, env, url);

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

// ---------- bug reports & feature ideas ----------
const KINDS = ['bug', 'feature'];
const STATUSES = ['new', 'planned', 'in-progress', 'done', 'wont-do'];
const REPORTER = /^[a-f0-9]{32}$/;
const text = (v, max) => String(v ?? '').replace(/\u0000/g, '').trim().slice(0, max);

// Constant-time compare, so the admin key can't be guessed a character at a time
function sameKey(a, b) {
  const x = new TextEncoder().encode(String(a)), y = new TextEncoder().encode(String(b));
  if (x.length !== y.length) return false;
  let d = 0;
  for (let i = 0; i < x.length; i++) d |= x[i] ^ y[i];
  return d === 0;
}
function isAdmin(request, env) {
  const got = (request.headers.get('authorization') || '').replace(/^Bearer\s+/i, '');
  return !!env.ADMIN_KEY && !!got && sameKey(got, env.ADMIN_KEY);
}

async function reports(request, env, url) {
  const path = url.pathname.replace(/\/+$/, '');

  // Anyone with the app: send one
  if (path === '/v1/reports' && request.method === 'POST') {
    if (Number(request.headers.get('content-length') || 0) > 64 * 1024) return json({ error: 'too large' }, 413);
    let b;
    try { b = await request.json(); } catch { return json({ error: 'bad json' }, 400); }
    const kind = KINDS.includes(b?.kind) ? b.kind : null;
    const title = text(b?.title, 140);
    const body = text(b?.body, 6000);
    const reporter = String(b?.reporter || '');
    if (!kind || !title || !REPORTER.test(reporter)) return json({ error: 'bad request' }, 400);
    // A computer can send at most 20 an hour (stops a stuck button from flooding the list)
    const now = Date.now();
    const recent = await env.DB.prepare('SELECT COUNT(*) AS n FROM reports WHERE reporter = ? AND created_at > ?').bind(reporter, now - 3600e3).first();
    if ((recent?.n || 0) >= 20) return json({ error: 'too many' }, 429);
    const r = await env.DB.prepare(`INSERT INTO reports (created_at, updated_at, kind, title, body, name, reporter, version, platform, screen)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`)
      .bind(now, now, kind, title, body, text(b?.name, 80), reporter, text(b?.version, 40), text(b?.platform, 60), text(b?.screen, 120)).run();
    return json({ id: r.meta.last_row_id });
  }

  // Anyone: the ones this computer sent
  if (path === '/v1/reports/mine' && request.method === 'GET') {
    const reporter = url.searchParams.get('reporter') || '';
    if (!REPORTER.test(reporter)) return json({ error: 'bad request' }, 400);
    const { results } = await env.DB.prepare(`SELECT id, created_at, updated_at, kind, title, status, reply FROM reports
      WHERE reporter = ? ORDER BY created_at DESC LIMIT 100`).bind(reporter).all();
    return json({ reports: results || [] });
  }

  // The rest is for the admin only
  if (!env.ADMIN_KEY) return json({ error: 'no admin key set up on the server' }, 503);
  if (!isAdmin(request, env)) return json({ error: 'not allowed' }, 401);

  if (path === '/v1/reports' && request.method === 'GET') {
    const { results } = await env.DB.prepare(`SELECT id, created_at, updated_at, kind, title, body, name, version, platform, screen, status, reply
      FROM reports ORDER BY created_at DESC LIMIT 1000`).all();
    return json({ reports: results || [] });
  }

  const m = path.match(/^\/v1\/reports\/(\d+)$/);
  if (!m) return json({ error: 'not found' }, 404);
  const id = Number(m[1]);
  if (request.method === 'PATCH') {
    let b;
    try { b = await request.json(); } catch { return json({ error: 'bad json' }, 400); }
    const sets = [], vals = [];
    if (b?.status !== undefined) {
      if (!STATUSES.includes(b.status)) return json({ error: 'bad status' }, 400);
      sets.push('status = ?'); vals.push(b.status);
    }
    if (b?.reply !== undefined) { sets.push('reply = ?'); vals.push(text(b.reply, 3000)); }
    if (!sets.length) return json({ error: 'nothing to change' }, 400);
    const r = await env.DB.prepare(`UPDATE reports SET ${sets.join(', ')}, updated_at = ? WHERE id = ?`).bind(...vals, Date.now(), id).run();
    return r.meta.changes ? json({ ok: true }) : json({ error: 'not found' }, 404);
  }
  if (request.method === 'DELETE') {
    const r = await env.DB.prepare('DELETE FROM reports WHERE id = ?').bind(id).run();
    return r.meta.changes ? json({ ok: true }) : json({ error: 'not found' }, 404);
  }
  return json({ error: 'method not allowed' }, 405);
}