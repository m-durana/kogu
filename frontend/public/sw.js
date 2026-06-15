// Kogu service worker — installability + fast loads. Offline is deferred (DESIGN Appendix A),
// so this caches the app shell + hashed assets but never caches API responses.
const CACHE = 'kogu-v4'
const SHELL = ['/', '/index.html', '/manifest.webmanifest', '/favicon.svg', '/icon-192.png?v=3', '/icon-512.png?v=3']

self.addEventListener('install', (e) => {
  e.waitUntil(caches.open(CACHE).then((c) => c.addAll(SHELL)).then(() => self.skipWaiting()))
})

self.addEventListener('activate', (e) => {
  e.waitUntil(
    caches.keys().then((keys) => Promise.all(keys.filter((k) => k !== CACHE).map((k) => caches.delete(k)))).then(() => self.clients.claim()),
  )
})

self.addEventListener('fetch', (e) => {
  const req = e.request
  if (req.method !== 'GET') return
  const url = new URL(req.url)
  if (url.origin !== location.origin) return // let cross-origin (fonts, etc.) pass through
  if (url.pathname.startsWith('/api/')) return // never cache the API

  // Navigations: cache-FIRST so the installed PWA opens instantly (no black screen waiting on the
  // network); revalidate the shell in the background for next launch.
  if (req.mode === 'navigate') {
    e.respondWith(
      caches.match('/index.html').then((cached) => {
        const network = fetch(req)
          .then((res) => {
            if (res.ok) caches.open(CACHE).then((c) => c.put('/index.html', res.clone()))
            return res
          })
          .catch(() => cached)
        return cached || network
      }),
    )
    return
  }

  // Hashed assets: stale-while-revalidate.
  e.respondWith(
    caches.match(req).then((cached) => {
      const network = fetch(req)
        .then((res) => {
          if (res.ok) caches.open(CACHE).then((c) => c.put(req, res.clone()))
          return res
        })
        .catch(() => cached)
      return cached || network
    }),
  )
})
