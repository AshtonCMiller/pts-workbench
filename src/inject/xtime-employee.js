// Runs inside the Xtime Inspect frame. Looks up one employee number in the store's employee list
// (with your existing login) and returns ONLY that person's Xtime technician ID and name.
// Nobody's email or phone number leaves the page.
(async (NUMBER, STORE, PATH) => {
  const get = (obj, name) => {
    if (!obj || typeof obj !== 'object') return undefined;
    const key = Object.keys(obj).find((k) => k.toLowerCase() === name.toLowerCase());
    return key === undefined ? undefined : obj[key];
  };
  let text;
  try {
    const res = await fetch(PATH.replace('{STORE}', encodeURIComponent(STORE)), {
      credentials: 'include', headers: { Accept: 'application/json, text/javascript, */*' },
    });
    if (res.status === 401 || res.status === 403 || res.redirected) return { ok: false, expired: true, error: 'Xtime session expired' };
    if (!res.ok) return { ok: false, error: `Xtime returned HTTP ${res.status} for the employee list` };
    text = await res.text();
  } catch (e) {
    return { ok: false, error: `Couldn't load the employee list: ${e.message}` };
  }

  // JSON, or JSON wrapped in a little script
  let data = null;
  try { data = JSON.parse(text); } catch {
    const a = text.indexOf('['), b = text.lastIndexOf(']');
    if (a >= 0 && b > a) { try { data = JSON.parse(text.slice(a, b + 1)); } catch { /* not usable */ } }
  }
  let list = Array.isArray(data) ? data : null;
  if (!list && data && typeof data === 'object') list = Object.values(data).find(Array.isArray) || null;
  if (!list) return { ok: false, error: "The employee list wasn't in a format the app understands" };

  const norm = (x) => String(x ?? '').trim().replace(/^0+(?=\d)/, '');
  const want = norm(NUMBER);
  const matches = list.filter((e) => e && norm(get(e, 'Number')) === want);
  if (!matches.length) return { ok: false, notFound: true, error: `Employee number ${NUMBER} isn't in store ${STORE}'s list` };

  // Prefer an active technician if the number appears more than once
  const isTech = (e) => /tech/i.test(String(get(get(e, 'EmployeeType'), 'Name') ?? ''));
  const pick = matches.find((e) => get(e, 'Active') !== false && isTech(e))
    || matches.find((e) => get(e, 'Active') !== false) || matches[0];
  const id = get(pick, 'ID') ?? get(pick, 'Id');
  if (id == null) return { ok: false, error: 'That employee has no technician ID in Xtime' };
  return {
    ok: true,
    id: String(id),
    name: [get(pick, 'FirstName'), get(pick, 'LastName')].filter((x) => typeof x === 'string' && x.trim()).join(' ').trim(),
    active: get(pick, 'Active') !== false,
  };
})