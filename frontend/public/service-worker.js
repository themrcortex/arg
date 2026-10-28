/* ARG shell-only offline support. Authenticated responses and all API traffic are excluded. */
const CACHE_NAME = 'arg-shell-v2'
const SHELL = ['/', '/manifest.webmanifest', '/arg/brand/arg-favicon.png', '/arg/brand/arg-app-icon.png']

async function cacheAsset(request, response) {
  const cache = await caches.open(CACHE_NAME)
  await cache.put(request, response)
  const assets = (await cache.keys()).filter((key) => new URL(key.url).pathname.startsWith('/assets/'))
  await Promise.all(assets.slice(0, Math.max(0, assets.length - 64)).map((key) => cache.delete(key)))
}

self.addEventListener('install', (event) => {
  event.waitUntil(caches.open(CACHE_NAME).then((cache) => cache.addAll(SHELL)).then(() => self.skipWaiting()))
})

self.addEventListener('activate', (event) => {
  event.waitUntil(caches.keys().then((keys) => Promise.all(keys.filter((key) => key.startsWith('arg-shell-') && key !== CACHE_NAME).map((key) => caches.delete(key)))).then(() => self.clients.claim()))
})

self.addEventListener('fetch', (event) => {
  const request = event.request
  if (request.method !== 'GET') return
  const url = new URL(request.url)
  if (url.origin !== self.location.origin || url.pathname.startsWith('/api/') || url.pathname === '/api') return

  if (request.mode === 'navigate') {
    event.respondWith(fetch(request).then((response) => {
      if (response.ok) void caches.open(CACHE_NAME).then((cache) => cache.put('/', response.clone()))
      return response
    }).catch(async () => (await caches.match('/')) || Response.error()))
    return
  }

  if (url.pathname.startsWith('/assets/')) {
    event.respondWith(caches.match(request).then((cached) => cached || fetch(request).then((response) => {
      if (response.ok && response.type === 'basic') void cacheAsset(request, response.clone())
      return response
    })))
  }
})
