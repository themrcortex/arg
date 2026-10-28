import assert from 'node:assert/strict'
import test from 'node:test'
import { createRequestEpoch, createSingleFlightGate } from '../src/lib/request-epoch.mjs'

test('an older asynchronous response cannot replace newer page state', async () => {
  const requests = createRequestEpoch()
  let page = null
  const start = async (response) => {
    const request = requests.next()
    const value = await response
    if (requests.isCurrent(request)) page = value
  }
  let finishFirst
  let finishSecond
  const firstResponse = new Promise((resolve) => { finishFirst = resolve })
  const secondResponse = new Promise((resolve) => { finishSecond = resolve })
  const first = start(firstResponse)
  const second = start(secondResponse)

  finishSecond('new bucket')
  await second
  finishFirst('old bucket')
  await first

  assert.equal(page, 'new bucket')
})

test('changing the resource invalidates its in-flight request', () => {
  const requests = createRequestEpoch()
  const request = requests.next()

  requests.invalidate()

  assert.equal(requests.isCurrent(request), false)
})

test('single-flight polling rejects overlap and stale releases cannot unlock a newer request', () => {
  const gate = createSingleFlightGate()
  const first = gate.tryEnter()

  assert.notEqual(first, null)
  assert.equal(gate.tryEnter(), null)
  gate.reset()

  const second = gate.tryEnter()
  assert.notEqual(second, null)
  gate.release(first)
  assert.equal(gate.tryEnter(), null)
  gate.release(second)
  assert.notEqual(gate.tryEnter(), null)
})
