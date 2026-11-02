import assert from 'node:assert/strict'
import test from 'node:test'
import { validateWordPressCreation } from '../src/lib/wordpress-form-validation.mjs'

const valid = {
  name: 'Test site',
  domain: 'site.example.com',
  version: '6.8.3',
  siteTitle: 'Test site',
  username: 'site-manager',
  email: 'admin@example.com',
  password: 'Strong-password-123',
}

test('WordPress creation accepts a valid setup and mirrors backend input limits', () => {
  assert.equal(validateWordPressCreation(valid), '')
  assert.equal(validateWordPressCreation({ ...valid, password: '😀'.repeat(6) }), '')
  assert.equal(validateWordPressCreation({ ...valid, email: 'admin@example.com.' }), '')
})

test('WordPress creation reports the specific invalid field before sending the request', () => {
  assert.match(validateWordPressCreation({ ...valid, name: ' ' }), /نام اپلیکیشن/)
  assert.match(validateWordPressCreation({ ...valid, domain: 'https://site.example.com' }), /دامنه/)
  assert.match(validateWordPressCreation({ ...valid, version: '6.8' }), /نسخهٔ هسته/)
  assert.match(validateWordPressCreation({ ...valid, siteTitle: ' ' }), /عنوان سایت/)
  assert.match(validateWordPressCreation({ ...valid, username: 'site manager' }), /نام کاربری مدیر/)
  assert.match(validateWordPressCreation({ ...valid, email: 'bad-email' }), /ایمیل کامل/)
  assert.match(validateWordPressCreation({ ...valid, password: 'short' }), /گذرواژه/)
  assert.match(validateWordPressCreation({ ...valid, password: '😀'.repeat(65) }), /گذرواژه/)
})
