/** Choose bounded deadlines for the shared JSON API wrapper. */
export function requestTimeoutMs(path) {
  const route = path.split('?', 1)[0].replace(/^\/+/, '')

  if (/(?:^|\/)(?:upload|download)(?:\/|$)/.test(route) && /(?:^|\/)(?:s3|files?|backups?)(?:\/|$)/.test(route)) {
    return 15 * 60 * 1000
  }
  if (/^nginx\/sites\/[^/]+\/ssl(?:\/|$)/.test(route)
    || /^nginx\/action(?:\/|$)/.test(route)
    || /^(?:mysql|firewall)\/install(?:\/|$)/.test(route)
    || /^maintenance\/action(?:\/|$)/.test(route)
    || /^maintenance\/backups\/[^/]+\/restore(?:\/|$)/.test(route)
    || /^apps\/[^/]+\/restore(?:\/|$)/.test(route)) return 15 * 60 * 1000
  if (/^(?:packages|apps\/[^/]+\/backups?)(?:\/|$)/.test(route)
    || /^(?:maintenance\/backups?)(?:\/|$)/.test(route)
    || /(?:^|\/)restore(?:\/|$)/.test(route)) return 5 * 60 * 1000
  if (/(?:^|\/)(?:jobs?|deployments?)\/[^/]+\/(?:logs|events|progress)(?:\/|$)/.test(route)) return 12 * 1000
  return 30 * 1000
}

export function createRequestDeadline(timeoutMs, callerSignal) {
  const controller = new AbortController()
  let timedOut = false
  const timeout = setTimeout(() => {
    timedOut = true
    controller.abort(new DOMException('Request timed out', 'TimeoutError'))
  }, timeoutMs)
  const abortFromCaller = () => controller.abort(callerSignal?.reason)
  if (callerSignal?.aborted) abortFromCaller()
  else callerSignal?.addEventListener('abort', abortFromCaller, { once: true })
  return {
    signal: controller.signal,
    timedOut: () => timedOut,
    dispose: () => {
      clearTimeout(timeout)
      callerSignal?.removeEventListener('abort', abortFromCaller)
    },
  }
}
