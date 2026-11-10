import assert from 'node:assert/strict'
import { test } from 'node:test'
import { formatMb, formatMbps, formatMs, relativeAge } from '../../src/lib/format.ts'

test('speeds keep three figures and switch precision after rounding', () => {
  assert.equal(formatMbps(312.4), '312')
  assert.equal(formatMbps(41.26), '41')
  assert.equal(formatMbps(9.97), '10')
  assert.equal(formatMbps(9.94), '9.9')
  assert.equal(formatMbps(0.996), '1.0')
  assert.equal(formatMbps(0.42), '0.42')
  assert.equal(formatMbps(Number.NaN), '–')
  assert.equal(formatMbps(-1), '–')
})

test('latency rounds to whole milliseconds', () => {
  assert.equal(formatMs(14.4), '14')
  assert.equal(formatMs(0.4), '<1')
  assert.equal(formatMs(Number.POSITIVE_INFINITY), '–')
})

test('data used reads in MB', () => {
  assert.equal(formatMb(500_000), '<1 MB')
  assert.equal(formatMb(2_340_000), '2.3 MB')
  assert.equal(formatMb(9_970_000), '10 MB')
  assert.equal(formatMb(212_400_000), '212 MB')
})

test('age reads in the largest whole unit', () => {
  const minute = 60_000
  assert.equal(relativeAge(0, 30_000), 'just now')
  assert.equal(relativeAge(0, 12 * minute), '12 min ago')
  assert.equal(relativeAge(0, 3 * 60 * minute), '3 h ago')
  assert.equal(relativeAge(0, 24 * 60 * minute), '1 day ago')
  assert.equal(relativeAge(0, 3 * 24 * 60 * minute), '3 days ago')
  assert.equal(relativeAge(5_000, 0), 'just now')
})
