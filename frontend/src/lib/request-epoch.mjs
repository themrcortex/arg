export function createRequestEpoch() {
  let current = 0
  return {
    next() {
      current += 1
      return current
    },
    isCurrent(request) {
      return request === current
    },
    invalidate() {
      current += 1
    },
  }
}

export function createSingleFlightGate() {
  let nextToken = 0
  let activeToken = 0
  return {
    tryEnter() {
      if (activeToken !== 0) return null
      activeToken = ++nextToken
      return activeToken
    },
    release(token) {
      if (activeToken === token) activeToken = 0
    },
    reset() {
      activeToken = 0
    },
  }
}
