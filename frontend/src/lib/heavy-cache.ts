type CachedValue = { value: unknown; expiresAt: number }

const values = new Map<string, CachedValue>()
const pending = new Map<string, Promise<unknown>>()
const versions = new Map<string, number>()
let generation = 0

export function cachedSnapshot<T>(key: string): T | null {
  const entry = values.get(key)
  return entry && entry.expiresAt > Date.now() ? entry.value as T : null
}

export function invalidateCached(key: string) {
  values.delete(key)
  pending.delete(key)
  versions.set(key, (versions.get(key) || 0) + 1)
}

export function clearHeavyCache() {
  values.clear()
  pending.clear()
  versions.clear()
  generation++
}

export async function cachedRequest<T>(key: string, loader: () => Promise<T>, ttlMs: number, force = false): Promise<T> {
  if (!force) {
    const snapshot = cachedSnapshot<T>(key)
    if (snapshot !== null) return snapshot
  }
  const inFlight = pending.get(key)
  if (inFlight) return inFlight as Promise<T>
  const currentGeneration = generation
  const currentVersion = versions.get(key) || 0
  const request = loader().then((value) => {
    if (currentGeneration === generation && currentVersion === (versions.get(key) || 0)) values.set(key, { value, expiresAt: Date.now() + ttlMs })
    return value
  }).finally(() => { if (pending.get(key) === request) pending.delete(key) })
  pending.set(key, request)
  return request
}
