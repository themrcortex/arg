export function requestTimeoutMs(path: string): number
export function createRequestDeadline(timeoutMs: number, callerSignal?: AbortSignal | null): {
  signal: AbortSignal
  timedOut: () => boolean
  dispose: () => void
}
