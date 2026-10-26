import assert from 'node:assert/strict'
import { test } from 'node:test'
import { demoReport } from '../../src/lib/demo.ts'
import { summaryText } from '../../src/lib/format.ts'
import { defaultSettings } from '../../src/lib/model.ts'
import { comparePlan, parsePlan, share, shareText } from '../../src/lib/plan.ts'

test('plan bands match the Rust core: 80 % good, 50 % fair', () => {
  assert.equal(share(400, 500, false).health, 'good')
  assert.equal(share(395, 500, false).health, 'fair')
  assert.equal(share(245, 500, false).health, 'poor')
  assert.equal(share(940, 2_000, true).health, 'fair')
  assert.equal(shareText(share(940, 2_000, true)), 'At least 47% of your 2000 Mbps plan')
})

test('only speeds that were set and measured are compared', () => {
  const report = demoReport(0)
  assert.equal(comparePlan(report, defaultSettings), null)
  const plan = comparePlan(report, { ...defaultSettings, planDownloadMbps: 500, planUploadMbps: 50 })
  assert.equal(plan?.download?.percent, 62)
  assert.equal(plan?.upload?.percent, 82)
  report.upload = null
  assert.equal(comparePlan(report, { ...defaultSettings, planUploadMbps: 50 }), null)
})

test('plan speeds are whole Mbps or empty', () => {
  assert.equal(parsePlan(' 500 '), 500)
  assert.equal(parsePlan(''), null)
  for (const text of ['0', '12.5', '-3', 'fast', '100001']) assert.equal(parsePlan(text), 'invalid')
})

test('summary names the plan share for the provider', () => {
  const report = demoReport(0)
  const plan = comparePlan(report, { ...defaultSettings, planDownloadMbps: 500, planUploadMbps: 50 })
  assert.ok(summaryText(report, null, plan).split('\n').includes('Download 62% of the 500 Mbps plan · Upload 82% of the 50 Mbps plan'))
})
