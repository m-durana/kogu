// Kogu service worker.
// Goal: stop iOS dumping the installed PWA to the Safari address bar on relaunch. When iOS kills the
// backgrounded WebView it COLD-navigates to start_url ("/") on reopen; that request is no-cache, so a
// flaky network at that moment (Wi-Fi↔cellular handoff, just back in signal, VPN reconnect) fails and,
// with no fetch handler, nothing serves the shell and the standalone view falls back to Safari chrome.
//
// Fix: a navigation-ONLY, network-first handler with a precached app-shell fallback. Non-navigation
// requests (hashed /assets, /api, fonts) are NOT intercepted, so there is no per-request SW cold-boot
// cost and we don't re-enable the SW-controlled-fetch freeze class. Online relaunch still fetches the
// live index.html (instant deploys preserved); the cached shell is used only when the network fails.
const VERSION = 'kogu-v15'
const SHELL_CACHE = `${VERSION}-shell`
const SHELL_URL = '/index.html'

self.addEventListener('install', (e) => {
  e.waitUntil(
    caches
      .open(SHELL_CACHE)
      // cache:'reload' bypasses the HTTP cache so we precache a FRESH shell, not a stale one
      .then((c) => c.add(new Request(SHELL_URL, { cache: 'reload' })))
      .then(() => self.skipWaiting()),
  )
})

self.addEventListener('activate', (e) => {
  e.waitUntil(
    caches
      .keys()
      .then((keys) => Promise.all(keys.filter((k) => k !== SHELL_CACHE).map((k) => caches.delete(k))))
      .then(() => self.clients.claim()),
  )
})

self.addEventListener('fetch', (e) => {
  const req = e.request
  // ONLY handle top-level navigations; everything else goes straight to the network / HTTP cache.
  if (req.mode !== 'navigate') return

  e.respondWith(
    (async () => {
      try {
        // Network-first so a deploy still propagates instantly (matches the no-cache HTML policy).
        const fresh = await fetch(req)
        const cache = await caches.open(SHELL_CACHE)
        cache.put(SHELL_URL, fresh.clone()) // keep the offline shell current
        return fresh
      } catch {
        // Offline / flaky relaunch: serve the precached shell instead of the Safari address bar.
        const cache = await caches.open(SHELL_CACHE)
        const cached = await cache.match(SHELL_URL)
        if (cached) return cached
        return new Response(
          '<!doctype html><meta charset=utf-8><title>Kogu</title>' +
            '<body style="margin:0;background:#0b0b0c;color:#ededeb;font-family:serif;' +
            'display:flex;align-items:center;justify-content:center;height:100vh">' +
            '<div style="text-align:center">古古<br><small>offline, reopen when connected</small></div>',
          { headers: { 'Content-Type': 'text/html; charset=utf-8' } },
        )
      }
    })(),
  )
})
