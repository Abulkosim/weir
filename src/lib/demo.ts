import { viewSchema, type Report, type View } from './model.ts'

export const demoStates = ['never', 'result', 'testing', 'testing-latency', 'testing-upload', 'failed'] as const
export type DemoState = (typeof demoStates)[number]

const minute = 60_000

export function demoReport(now: number): Report {
  return {
    startedAtMs: now - 12 * minute,
    durationMs: 15_400,
    provider: 'Cloudflare',
    colo: 'FRA',
    latency: { medianMs: 14, jitterMs: 2.1, answered: 19, sent: 19 },
    download: { mbps: 312.4, p10: 296, p90: 324, varied: false, capped: false, rampedSlowly: false, rough: false, loadedMs: 62 },
    upload: { mbps: 41.2, p10: 39, p90: 43, varied: false, capped: false, rampedSlowly: false, rough: false, loadedMs: 48 },
    uploadError: null,
    bytesUsed: 238_000_000,
  }
}

/** Live download samples that ramp up and settle, one per 100 ms. */
export function demoSamples(count: number, target: number): number[] {
  return Array.from({ length: count }, (_, index) => {
    const ramp = target * (1 - Math.exp(-index / 4.5))
    return Math.max(2, ramp + Math.sin(index * 1.7) * target * 0.025 + Math.cos(index * 0.9) * target * 0.018)
  })
}

export function createDemoView(state: string | null, now = Date.now()): View {
  const scenario: DemoState = demoStates.includes(state as DemoState) ? state as DemoState : 'result'
  const view: View = { revision: 1, testing: null, last: null, failure: null, estimate: { upTo: true, mb: 600 } }
  if (scenario === 'never') return viewSchema.parse(view)
  view.last = demoReport(now)
  view.estimate = { upTo: false, mb: 250 }
  if (scenario === 'testing') {
    view.testing = { phase: 'download', phaseMs: 3_600, liveMbps: 297, downloadMbps: null, latencyMs: 14, loadedMs: 58, bytesUsed: 96_000_000, colo: 'FRA' }
  }
  if (scenario === 'testing-latency') {
    view.testing = { phase: 'latency', phaseMs: 700, liveMbps: null, downloadMbps: null, latencyMs: 14, loadedMs: null, bytesUsed: 12_000, colo: 'FRA' }
  }
  if (scenario === 'testing-upload') {
    view.testing = { phase: 'upload', phaseMs: 2_900, liveMbps: 39.6, downloadMbps: 312.4, latencyMs: 14, loadedMs: 51, bytesUsed: 214_000_000, colo: 'FRA' }
  }
  if (scenario === 'failed') {
    view.failure = { error: 'stalled', atMs: now - 2 * minute }
    if (view.last) view.last = { ...view.last, startedAtMs: now - 3 * 60 * minute }
  }
  return viewSchema.parse(view)
}

/** Progress timeline for a simulated run in the browser preview, one step per 100 ms. */
export function demoProgress(step: number): View['testing'] {
  const at = step * 100
  if (at < 400) return { phase: 'connecting', phaseMs: at, liveMbps: null, downloadMbps: null, latencyMs: null, loadedMs: null, bytesUsed: 0, colo: null }
  if (at < 2_000) return { phase: 'latency', phaseMs: at - 400, liveMbps: null, downloadMbps: null, latencyMs: 14, loadedMs: null, bytesUsed: 20_000, colo: 'FRA' }
  if (at < 8_000) {
    const index = (at - 2_000) / 100
    const live = demoSamples(index + 1, 312)[index] ?? 0
    return { phase: 'download', phaseMs: at - 2_000, liveMbps: live, downloadMbps: null, latencyMs: 14, loadedMs: index > 15 ? 62 : null, bytesUsed: Math.round(index * 3_900_000), colo: 'FRA' }
  }
  if (at < 13_500) {
    const index = (at - 8_000) / 100
    const live = demoSamples(index + 1, 41)[index] ?? 0
    return { phase: 'upload', phaseMs: at - 8_000, liveMbps: live, downloadMbps: 312.4, latencyMs: 14, loadedMs: index > 15 ? 48 : null, bytesUsed: 234_000_000 + Math.round(index * 500_000), colo: 'FRA' }
  }
  return null
}
