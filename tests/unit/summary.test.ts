import assert from 'node:assert/strict'
import { test } from 'node:test'
import { demoMeaning, demoReport } from '../../src/lib/demo.ts'
import { summaryText } from '../../src/lib/format.ts'

test('summary is plain text with server, speeds, latency and verdict', () => {
  const lines = summaryText(demoReport(Date.UTC(2026, 9, 25, 12)), demoMeaning).split('\n')
  assert.match(lines[0]!, /^Weir speed test · .+ · Cloudflare, Frankfurt$/)
  assert.equal(lines[1], 'Download 312 Mbps · Upload 41 Mbps')
  assert.equal(lines[2], 'Latency 14 ms, 62 ms under load · Jitter 2 ms')
  assert.equal(lines[3], 'Good for 4K video and calls')
})

test('summary says what was not measured', () => {
  const report = demoReport(0)
  report.upload = null
  report.latency = null
  const lines = summaryText(report, null).split('\n')
  assert.equal(lines[1], 'Download 312 Mbps · Upload not measured')
  assert.equal(lines.length, 2)
})
