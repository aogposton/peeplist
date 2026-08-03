// Hand-rolled PWA service worker (2026-08-02) — Dioxus 0.7 has no built-in
// PWA/service-worker support, so this is entirely manual. Deliberately kept
// OUT of Dioxus's asset!() hashing pipeline (see scripts/deploy.sh, which
// copies this file verbatim into the build output) — a service worker needs
// a stable, predictable registration URL across deploys, which a
// content-hash suffix that changes every build would break.
//
// Strategy:
//   - /assets/* (every JS/CSS/WASM/icon file `dx build` emits) is content-
//     hashed — a given URL's content never changes, so cache-first is safe
//     and permanent; no explicit precache list needed, each file just gets
//     cached the first time it's actually fetched.
//   - Navigations (the HTML shell) are network-first, falling back to
//     whatever was last cached if the network fails — so a reload while
//     offline still loads *something* (possibly stale, never nothing).
const CACHE_NAME = 'bsb-shell-v1';

self.addEventListener('install', () => {
    self.skipWaiting();
});

self.addEventListener('activate', (event) => {
    event.waitUntil(
        caches.keys()
            .then((names) => Promise.all(names.filter((n) => n !== CACHE_NAME).map((n) => caches.delete(n))))
            .then(() => self.clients.claim())
    );
});

self.addEventListener('fetch', (event) => {
    const req = event.request;
    if (req.method !== 'GET') {
        return;
    }

    const url = new URL(req.url);
    if (url.pathname.startsWith('/assets/')) {
        event.respondWith(
            caches.match(req).then((cached) => {
                if (cached) {
                    return cached;
                }
                return fetch(req).then((res) => {
                    if (res.ok) {
                        const copy = res.clone();
                        caches.open(CACHE_NAME).then((cache) => cache.put(req, copy));
                    }
                    return res;
                });
            })
        );
        return;
    }

    if (req.mode === 'navigate') {
        event.respondWith(
            fetch(req)
                .then((res) => {
                    const copy = res.clone();
                    caches.open(CACHE_NAME).then((cache) => cache.put(req, copy));
                    return res;
                })
                .catch(() => caches.match(req).then((cached) => cached || caches.match('/')))
        );
    }
});
