export type RequestEpoch = {
  next(): number
  isCurrent(request: number): boolean
  invalidate(): void
}

export function createRequestEpoch(): RequestEpoch

export type SingleFlightGate = {
  tryEnter(): number | null
  release(token: number): void
  reset(): void
}

export function createSingleFlightGate(): SingleFlightGate
