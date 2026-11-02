import assert from 'node:assert/strict'
import test from 'node:test'
import { isValidWordPressVersion, WORDPRESS_VERSION_PATTERN } from '../src/lib/wordpress-version.mjs'

test('WordPress core version pattern accepts dotted stable triples', () => {
  assert.equal(isValidWordPressVersion('6.8.3'), true)
  assert.equal(isValidWordPressVersion('7.1.4'), true)
  assert.equal(new RegExp(`^(?:${WORDPRESS_VERSION_PATTERN})$`).test('12.34.56'), true)
})

test('WordPress core version pattern rejects incomplete and injected values', () => {
  for (const value of ['', '7.1', '7.1.4-rc1', '7.1.4;id', 'v7.1.4', '1234.1.1', '07.1.4']) {
    assert.equal(isValidWordPressVersion(value), false, value)
  }
})
