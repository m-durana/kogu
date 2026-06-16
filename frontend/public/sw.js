// Kogu service worker — intentionally minimal. Offline is deferred (DESIGN Appendix A), and on iOS a
// fetch-handling, navigation-controlling SW must COLD-BOOT before it can answer the standalone launch
// navigation — adding seconds to every cold start, and the iOS PWA freeze/black-screen class is
// exclusive to SW-controlled fetches. So this SW has NO fetch handler: navigations and hashed assets
// go straight to the network / HTTP cache and never wait on the SW. It exists only to retire the old
// caching SW (delete its caches) and keep the app installable.
const CACHE = 'kogu-v14'

self.addEventListener('install', () => self.skipWaiting())

self.addEventListener('activate', (e) => {
  e.waitUntil(
    caches
      .keys()
      .then((keys) => Promise.all(keys.map((k) => caches.delete(k))))
      .then(() => self.clients.claim()),
  )
})
