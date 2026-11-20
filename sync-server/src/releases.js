// The desktop app's releases, from the R2 bucket pts-releases (binding RELEASES), so they don't come from GitHub.
//
// The release workflow (.github/workflows/release.yml in the Rust app) puts each release's files in the bucket:
//   releases/<tag>/<file>              the program files, the installer and workbench.json (their sizes and SHA-256)
//   channels/latest/workbench.json     the newest regular version's workbench.json
//   channels/beta/workbench.json       the newest beta's
//
//   GET /updates/latest/workbench.json  what the app checks (crates/app/src/updates.rs)
//   GET /updates/beta/workbench.json    … with Beta updates on (it takes the newer of the two)
//   GET /updates/<tag>/<file>           a release's file (never changes once it's there)
//   GET /download                       the newest installer, for setting up a new computer
//
// Every night, old releases are deleted so the bucket stays small (R2's free 10 GB): the newest few of each kind
// are kept, and always the ones the channels point at. The app checks each file's size and SHA-256 from
// workbench.json, so a file can't be swapped without it noticing.

const KEEP_REGULAR = 5;
const KEEP_BETAS = 3;
const TAG = /^v\d+\.\d+\.\d+(-[0-9A-Za-z.]+)?$/;
const NAME = /^[A-Za-z0-9][A-Za-z0-9._-]*$/;
const CHANNELS = ['latest', 'beta'];

const text = (body, status) => new Response(body, {
  status,
  headers: { 'content-type': 'text/plain; charset=utf-8', 'cache-control': 'no-store' },
});

function typeOf(name) {
  if (name.endsWith('.json')) return 'application/json';
  if (name.endsWith('.exe')) return 'application/vnd.microsoft.portable-executable';
  return 'application/octet-stream';
}

/** The launcher's compare_versions (crates/launcher): 1.2.3 < 1.2.4-beta.1 < 1.2.4 */
export function compareVersions(a, b) {
  const parse = (v) => {
    const s = String(v).replace(/^v/, '');
    const at = s.indexOf('-');
    const core = (at < 0 ? s : s.slice(0, at)).split('.').map((n) => parseInt(n, 10) || 0);
    const pre = at < 0 ? [] : s.slice(at + 1).split('.');
    return [core, pre];
  };
  const [[x, xp], [y, yp]] = [parse(a), parse(b)];
  for (let i = 0; i < 3; i++) {
    const d = (x[i] || 0) - (y[i] || 0);
    if (d) return Math.sign(d);
  }
  if (!xp.length || !yp.length) return xp.length === yp.length ? 0 : xp.length ? -1 : 1;
  for (let i = 0; i < Math.max(xp.length, yp.length); i++) {
    if (i >= xp.length) return -1;
    if (i >= yp.length) return 1;
    const [p, q] = [xp[i], yp[i]];
    const [pn, qn] = [/^\d+$/.test(p), /^\d+$/.test(q)];
    const o = pn && qn ? Math.sign(Number(p) - Number(q)) : pn ? -1 : qn ? 1 : p < q ? -1 : p > q ? 1 : 0;
    if (o) return o;
  }
  return 0;
}

async function channel(env, name) {
  const obj = await env.RELEASES.get(`channels/${name}/workbench.json`);
  if (!obj) return null;
  try { return await obj.json(); } catch { return null; }
}

export async function releases(request, env, url) {
  if (request.method !== 'GET' && request.method !== 'HEAD') return text('Not allowed', 405);
  if (!env.RELEASES) return text("Updates aren't set up on this server yet.", 503);

  if (url.pathname === '/download' || url.pathname === '/download/') {
    const m = await channel(env, 'latest');
    if (!m?.setup?.name || !NAME.test(m.setup.name)) return text('There is no version to download yet.', 404);
    return Response.redirect(`${url.origin}/updates/v${m.version}/${m.setup.name}`, 302);
  }

  const parts = url.pathname.split('/').slice(2); // /updates/<a>/<b>
  if (parts.length !== 2) return text('Not found', 404);
  const [where, name] = parts.map(decodeURIComponent);

  if (CHANNELS.includes(where) && name === 'workbench.json') {
    const obj = await env.RELEASES.get(`channels/${where}/workbench.json`);
    if (!obj) return text('Not found', 404);
    return new Response(request.method === 'HEAD' ? null : obj.body, {
      headers: { 'content-type': 'application/json', 'cache-control': 'no-store' },
    });
  }

  if (!TAG.test(where) || !NAME.test(name)) return text('Not found', 404);
  const obj = request.method === 'HEAD'
    ? await env.RELEASES.head(`releases/${where}/${name}`)
    : await env.RELEASES.get(`releases/${where}/${name}`);
  if (!obj) return text('Not found', 404);
  const headers = {
    'content-type': typeOf(name),
    'content-length': String(obj.size),
    etag: obj.httpEtag,
    // a release's files never change once they're there
    'cache-control': name === 'workbench.json' ? 'no-store' : 'public, max-age=31536000, immutable',
  };
  if (name.endsWith('.exe')) headers['content-disposition'] = `attachment; filename="${name}"`;
  return new Response(request.method === 'HEAD' ? null : obj.body, { headers });
}

/** Every night: deletes old releases, keeping the newest few of each kind and the ones the channels point at */
export async function pruneReleases(env) {
  if (!env.RELEASES) return;
  const tags = [];
  let cursor;
  do {
    const page = await env.RELEASES.list({ prefix: 'releases/', delimiter: '/', cursor });
    for (const p of page.delimitedPrefixes || []) {
      const tag = p.slice('releases/'.length, -1);
      if (TAG.test(tag)) tags.push(tag);
    }
    cursor = page.truncated ? page.cursor : undefined;
  } while (cursor);

  const keep = new Set();
  for (const c of CHANNELS) {
    const m = await channel(env, c);
    if (m?.version) keep.add(`v${m.version}`);
  }
  const newestFirst = (list) => list.sort((a, b) => compareVersions(b, a));
  newestFirst(tags.filter((t) => !t.includes('-'))).slice(0, KEEP_REGULAR).forEach((t) => keep.add(t));
  newestFirst(tags.filter((t) => t.includes('-'))).slice(0, KEEP_BETAS).forEach((t) => keep.add(t));

  for (const tag of tags.filter((t) => !keep.has(t))) {
    let c;
    do {
      const page = await env.RELEASES.list({ prefix: `releases/${tag}/`, cursor: c });
      const keys = page.objects.map((o) => o.key);
      if (keys.length) await env.RELEASES.delete(keys);
      c = page.truncated ? page.cursor : undefined;
    } while (c);
    console.log('deleted old release', tag);
  }
}
