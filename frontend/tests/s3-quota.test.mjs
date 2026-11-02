import test from 'node:test'
import assert from 'node:assert/strict'
import { quotaBytesFromGib } from '../src/lib/s3-quota.mjs'

test('manual bucket quota converts fractional GiB to bytes without losing the chosen size', () => {
  assert.equal(quotaBytesFromGib('1.5'), 1_610_612_736)
  assert.equal(quotaBytesFromGib('0.125'), 134_217_728)
})

test('manual bucket quota rejects empty, zero, negative and unsafe values', () => {
  for (const value of ['', '0', '-1', 'Infinity', '999999999']) {
    assert.throws(() => quotaBytesFromGib(value), RangeError)
  }
})
