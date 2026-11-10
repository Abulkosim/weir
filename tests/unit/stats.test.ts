import assert from 'node:assert/strict'
import { test } from 'node:test'
import { median } from '../../src/lib/stats.ts'

test('median of odd, even and empty lists', () => {
  assert.equal(median([3, 1, 2]), 2)
  assert.equal(median([4, 1, 3, 2]), 2.5)
  assert.equal(median([]), null)
})
