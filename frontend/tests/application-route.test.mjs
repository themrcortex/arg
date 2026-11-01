import test from 'node:test'
import assert from 'node:assert/strict'
import { buildApplicationRoute, parseApplicationRoute } from '../src/lib/application-route.mjs'

test('application detail route retains selected section for refresh and sharing', () => {
  const hash = buildApplicationRoute({ appId: 'app 1', serverUuid: 'server/2', section: 'runtime' })
  assert.equal(hash, '#/apps?app=app+1&server=server%2F2&section=runtime')
  assert.deepEqual(parseApplicationRoute(hash), { appId: 'app 1', serverUuid: 'server/2', section: 'runtime' })
})

test('legacy and invalid application routes safely default to deployments', () => {
  assert.deepEqual(parseApplicationRoute('#/apps?app=a&server=b'), { appId: 'a', serverUuid: 'b', section: 'deploy' })
  assert.deepEqual(parseApplicationRoute('#/apps?app=a&server=b&section=terminal'), { appId: 'a', serverUuid: 'b', section: 'deploy' })
  assert.equal(buildApplicationRoute({ appId: null, serverUuid: null }), '#/apps')
})
