import test from 'node:test'
import assert from 'node:assert/strict'
import { pointOnPolyline } from '../src/lib/mesh-motion.mjs'

test('mesh packet interpolation follows the whole polyline at constant distance', () => {
  const path = [[0, 0], [10, 0], [10, 30]]
  assert.deepEqual(pointOnPolyline(path, 0), { x: 0, y: 0 })
  assert.deepEqual(pointOnPolyline(path, 0.25), { x: 10, y: 0 })
  assert.deepEqual(pointOnPolyline(path, 0.5), { x: 10, y: 10 })
  assert.deepEqual(pointOnPolyline(path, 1), { x: 10, y: 30 })
})

test('mesh packet interpolation clamps progress and tolerates degenerate paths', () => {
  assert.deepEqual(pointOnPolyline([[2, 4], [2, 4]], 0.5), { x: 2, y: 4 })
  assert.deepEqual(pointOnPolyline([[2, 4], [8, 4]], -1), { x: 2, y: 4 })
  assert.deepEqual(pointOnPolyline([[2, 4], [8, 4]], 2), { x: 8, y: 4 })
  assert.deepEqual(pointOnPolyline([], 0.5), { x: 0, y: 0 })
})
