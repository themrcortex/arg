import assert from 'node:assert/strict'
import test from 'node:test'
import { latestBuildPlans } from '../src/lib/build-plans.mjs'

test('only the latest manual decision is shown for the same release and root', () => {
  const oldAmbiguous = { id: 'old', release_id: 'release', subdirectory: '', created_at: 10, plan: { ambiguous: true } }
  const latestManual = { id: 'new', release_id: 'release', subdirectory: '', created_at: 10, plan: { ambiguous: false, override: true } }
  const monorepoRoot = { id: 'root', release_id: 'release', subdirectory: 'apps/web', created_at: 9 }

  assert.deepEqual(latestBuildPlans([latestManual, oldAmbiguous, monorepoRoot]), [latestManual, monorepoRoot])
})

test('analyses from different releases remain visible independently', () => {
  const first = { id: 'a', release_id: 'one', subdirectory: '' }
  const second = { id: 'b', release_id: 'two', subdirectory: '' }

  assert.deepEqual(latestBuildPlans([first, second]), [first, second])
})
