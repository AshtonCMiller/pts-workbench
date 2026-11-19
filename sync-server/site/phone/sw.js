// PTS Workbench on a phone: shows the notifications the desktop app sends (through /v1/push/send).
// Nothing is cached here, so the page is always the newest one on the server.

self.addEventListener('install', () => self.skipWaiting());
self.addEventListener('activate', (e) => e.waitUntil(self.clients.claim()));

self.addEventListener('push', (e) => {
  let n = {};
  try { n = e.data ? e.data.json() : {}; } catch { n = { body: e.data ? e.data.text() : '' }; }
  const title = n.title || 'PTS Workbench';
  const options = {
    body: n.body || '',
    icon: 'icon-192.png',
    badge: 'icon-192.png',
    data: { url: n.url || '/phone/' },
  };
  // the same RO's again replaces the last one, instead of piling up
  if (n.tag) { options.tag = n.tag; options.renotify = true; }
  e.waitUntil(Promise.all([
    self.registration.showNotification(title, options),
    // the page, when it's open, shows it too
    self.clients.matchAll({ type: 'window' }).then((list) => list.forEach((c) => c.postMessage({ kind: 'push', title, body: options.body }))),
  ]));
});

self.addEventListener('notificationclick', (e) => {
  e.notification.close();
  const url = new URL((e.notification.data && e.notification.data.url) || '/phone/', self.location.origin).href;
  e.waitUntil(self.clients.matchAll({ type: 'window', includeUncontrolled: true }).then((list) => {
    for (const c of list) {
      if (c.url.startsWith(self.location.origin + '/phone/') && 'focus' in c) return c.focus();
    }
    return self.clients.openWindow(url);
  }));
});