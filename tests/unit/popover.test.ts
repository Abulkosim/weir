import assert from 'node:assert/strict'
import { test } from 'node:test'
import { arrowOffset, fitHeight, nextPlacement, POPOVER_HEIGHT } from '../../src/lib/popover.ts'

test('placement keeps the newest revision and drops malformed payloads', () => {
  const current = { revision: 4, arrowX: 120 }
  assert.deepEqual(nextPlacement(current, { revision: 5, arrowX: 200 }), { revision: 5, arrowX: 200 })
  assert.deepEqual(nextPlacement(current, { revision: 3, arrowX: 10 }), current)
  assert.deepEqual(nextPlacement(current, { revision: 'x' }), { revision: 4, arrowX: null })
})

test('arrow stays inside the popover edges', () => {
  assert.equal(arrowOffset(null, 380), null)
  assert.equal(arrowOffset(20, 380), null)
  assert.equal(arrowOffset(190, 380), 190)
  assert.equal(arrowOffset(360, 380), null)
})

test('fit height rounds up and never exceeds the full popover', () => {
  assert.equal(fitHeight(14, 6, 300.2), 321)
  assert.equal(fitHeight(14, 6, 2000), POPOVER_HEIGHT)
})

test('tween easing stays within the start and end values', async () => {
  const { easeOut } = await import('../../src/lib/tween.ts')
  assert.equal(easeOut(0), 0)
  assert.equal(easeOut(1), 1)
  assert.ok(easeOut(0.5) > 0.5 && easeOut(0.5) < 1)
})
