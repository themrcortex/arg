import assert from 'node:assert/strict'
import test from 'node:test'
import { emptyNodeServiceLogsMessage } from '../src/lib/node-service-logs.mjs'

test('empty logs explain that the application service has not been created', () => {
  assert.match(emptyNodeServiceLogsMessage({ configured: false, state: 'not-configured' }), /پاک‌سازی شده/)
})

test('empty logs include the real inactive service state', () => {
  assert.match(emptyNodeServiceLogsMessage({ configured: true, state: 'failed' }), /failed/)
})

test('empty active journal gives a refresh hint', () => {
  assert.match(emptyNodeServiceLogsMessage({ configured: true, state: 'active' }), /journald/)
})
