import assert from 'node:assert/strict'
import { test } from 'node:test'
import { historyKey, nextSamples, noSamples } from '../../src/lib/live.ts'
import { demoProgress, demoReport } from '../../src/lib/demo.ts'
import { emptyView, type View } from '../../src/lib/model.ts'

const at = (step: number): View => ({ ...emptyView, testing: demoProgress(step) })

test('one sample per progress tick, reset between phases', () => {
  let samples = noSamples
  for (const step of [20, 21, 22]) samples = nextSamples(samples, at(step))
  assert.equal(samples.values.length, 3)
  const repeat = nextSamples(samples, { ...at(22), revision: 9, checking: true })
  assert.equal(repeat, samples, 'an unrelated update during the test adds nothing')
  samples = nextSamples(samples, at(80))
  assert.equal(samples.phase, 'upload')
  assert.equal(samples.values.length, 1)
  assert.deepEqual(nextSamples(samples, emptyView).values, [])
})

test('history key changes when a test replaces a pruned entry', () => {
  const before: View = { ...emptyView, historyCount: 5_000, last: demoReport(1_000) }
  const after: View = { ...before, last: demoReport(2_000) }
  assert.notEqual(historyKey(before), historyKey(after))
  assert.equal(historyKey(before), historyKey({ ...before, revision: 7 }))
  assert.notEqual(historyKey(before), historyKey({ ...before, outageSinceMs: 3 }))
})
