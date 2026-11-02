import test from 'node:test'
import assert from 'node:assert/strict'
import { buildApplicationRoute, parseApplicationRoute } from '../src/lib/application-route.mjs'

test('application detail route retains selected section for refresh and sharing', () => {
  const hash = buildApplicationRoute({ appId: 'app 1', serverUuid: 'server/2', section: 'runtime' })
  assert.equal(hash, '#/apps?app=app+1&server=server%2F2&section=runtime')
  assert.deepEqual(parseApplicationRoute(hash), { appId: 'app 1', serverUuid: 'server/2', section: 'runtime' })
})

test('new deployment has a shareable route that stays open after refresh', () => {
  const hash = buildApplicationRoute({ appId: 'node-app', serverUuid: 'vps-1', section: 'launch' })
  assert.equal(hash, '#/apps?app=node-app&server=vps-1&section=launch')
  assert.deepEqual(parseApplicationRoute(hash), { appId: 'node-app', serverUuid: 'vps-1', section: 'launch' })
})

test('application nginx tab has a stable shareable route', () => {
  const hash = buildApplicationRoute({ appId: 'app-nginx', serverUuid: 'vps-1', section: 'nginx' })
  assert.equal(hash, '#/apps?app=app-nginx&server=vps-1&section=nginx')
  assert.deepEqual(parseApplicationRoute(hash), { appId: 'app-nginx', serverUuid: 'vps-1', section: 'nginx' })
})

test('legacy and invalid application routes safely default to deployments', () => {
  assert.deepEqual(parseApplicationRoute('#/apps?app=a&server=b'), { appId: 'a', serverUuid: 'b', section: 'deploy' })
  assert.deepEqual(parseApplicationRoute('#/apps?app=a&server=b&section=terminal'), { appId: 'a', serverUuid: 'b', section: 'deploy' })
  assert.equal(buildApplicationRoute({ appId: null, serverUuid: null }), '#/apps')
})
