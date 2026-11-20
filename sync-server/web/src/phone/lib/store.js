// What's kept on this phone (localStorage: the same keys as the old page, so a pairing carries over)
export const store = {
  get(k) { try { return JSON.parse(localStorage.getItem(k)); } catch { return null; } },
  set(k, v) { try { localStorage.setItem(k, JSON.stringify(v)); } catch { /* private mode */ } },
  drop(k) { try { localStorage.removeItem(k); } catch { /* private mode */ } },
};
