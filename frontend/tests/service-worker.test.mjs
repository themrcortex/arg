import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import { runInNewContext } from 'node:vm'
import test from 'node:test'

const source = await readFile(new URL('../public/service-worker.js', import.meta.url), 'utf8')

function loadWorker() {
  const listeners = new Map()
  const self = { location: { origin: 'https://arg.example' }, addEventListener: (name, handler) => listeners.set(name, handler) }
  const cache = { put: async () => {}, keys: async () => [], delete: async () => {}, addAll: async () => {} }
  const caches = { match: async () => null, open: async () => cache, keys: async () => [], delete: async () => {} }
  runInNewContext(source, { self, URL, caches, fetch: () => Promise.resolve({ ok: true, type: 'basic', clone() { return this } }), Response: { error() { return {} } } })
  return listeners
}

function fetchEvent(url, { method = 'GET', mode = 'cors' } = {}) {
  let handled = false
  const event = { request: { url, method, mode }, respondWith() { handled = true } }
  return { event, handled: () => handled }
}

test('offline worker never intercepts API responses or mutations', () => {
  const fetch = loadWorker().get('fetch')
  for (const sample of [
    fetchEvent('https://arg.example/api/dashboard'),
    fetchEvent('https://arg.example/api/s3/buckets/1/objects'),
    fetchEvent('https://arg.example/assets/app.js', { method: 'POST' }),
    fetchEvent('https://third-party.example/assets/app.js'),
  ]) {
    fetch(sample.event)
    assert.equal(sample.handled(), false)
  }
})

test('offline worker may cache only same-origin asset GETs and navigation shell', () => {
  const fetch = loadWorker().get('fetch')
  const asset = fetchEvent('https://arg.example/assets/app-a1b2.js')
  fetch(asset.event)
  assert.equal(asset.handled(), true)
  const navigation = fetchEvent('https://arg.example/system', { mode: 'navigate' })
  fetch(navigation.event)
  assert.equal(navigation.handled(), true)
})
