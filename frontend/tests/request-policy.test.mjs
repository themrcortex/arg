import assert from 'node:assert/strict'
import test from 'node:test'
import { createRequestDeadline, requestTimeoutMs } from '../src/lib/request-policy.mjs'

test('ordinary and polling API calls have short bounded deadlines', () => {
  assert.equal(requestTimeoutMs('/api/system/status'), 30_000)
  assert.equal(requestTimeoutMs('/api/deployments/job-1/logs?after=10'), 12_000)
})

test('large transfers and backup operations receive longer route-specific deadlines', () => {
  assert.equal(requestTimeoutMs('/api/s3/buckets/1/objects/upload'), 900_000)
  assert.equal(requestTimeoutMs('/api/files/download?path=%2Ftmp%2Fa'), 900_000)
  assert.equal(requestTimeoutMs('/api/backups/restore'), 300_000)
  assert.equal(requestTimeoutMs('maintenance/backups/arg-1/restore'), 900_000)
  assert.equal(requestTimeoutMs('apps/app-1/backups'), 300_000)
})

test('synchronous system operations are not reported as failed after the ordinary read deadline', () => {
  assert.equal(requestTimeoutMs('nginx/sites/example.test/ssl'), 900_000)
  assert.equal(requestTimeoutMs('nginx/action'), 900_000)
  assert.equal(requestTimeoutMs('packages'), 300_000)
  assert.equal(requestTimeoutMs('mysql/install'), 900_000)
})

test('request deadlines abort timed out fetches and distinguish caller cancellation', async () => {
  const timed = createRequestDeadline(5)
  await new Promise((resolve) => setTimeout(resolve, 10))
  assert.equal(timed.signal.aborted, true)
  assert.equal(timed.signal.reason.name, 'TimeoutError')
  assert.equal(timed.timedOut(), true)
  timed.dispose()

  const caller = new AbortController()
  const cancelled = createRequestDeadline(100, caller.signal)
  caller.abort(new DOMException('Navigation changed', 'AbortError'))
  assert.equal(cancelled.signal.aborted, true)
  assert.equal(cancelled.signal.reason.name, 'AbortError')
  assert.equal(cancelled.timedOut(), false)
  cancelled.dispose()
})

test('disposing a completed request removes its deadline timer', async () => {
  const deadline = createRequestDeadline(5)
  deadline.dispose()
  await new Promise((resolve) => setTimeout(resolve, 10))
  assert.equal(deadline.signal.aborted, false)
  assert.equal(deadline.timedOut(), false)
})
