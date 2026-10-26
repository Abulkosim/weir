import type { UpdateState } from './updater.ts'
import { comparePlan } from './plan.ts'
import { testErrorSchema, viewSchema, type Diagnosis, type Entry, type Meaning, type Report, type View } from './model.ts'

export const demoStates = ['never', 'result', 'testing', 'testing-latency', 'testing-upload', 'failed', 'metered', 'offline', 'stutter', 'stale', 'capped', 'varied', 'partial', 'check-fine', 'check-isp', 'check-router', 'check-wifi', 'check-dns', 'checking', 'outage', 'noplan'] as const
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

export const demoMeaning: Meaning = {
  health: 'good',
  headline: 'Good for 4K video and calls',
  lines: [
    { topic: 'Video', text: 'Good for 4K video', health: 'good' },
    { topic: 'Calls', text: 'Good for video calls', health: 'good' },
    { topic: 'Games', text: 'Fine for online games', health: 'good' },
    { topic: 'Under load', text: 'Slows under load: +48 ms', health: 'fair' },
  ],
}

export const demoStutter: Meaning = {
  health: 'fair',
  headline: 'Calls may stutter: latency jumps to 180 ms under load',
  lines: [
    { topic: 'Video', text: 'Good for 4K video', health: 'good' },
    { topic: 'Calls', text: 'Calls may stutter: latency jumps to 180 ms under load', health: 'fair' },
    { topic: 'Games', text: 'Online games may lag', health: 'fair' },
    { topic: 'Under load', text: 'Lags under load: +166 ms', health: 'poor' },
  ],
}

/** Live download samples that ramp up and settle, one per 100 ms. */
export function demoSamples(count: number, target: number): number[] {
  return Array.from({ length: count }, (_, index) => {
    const ramp = target * (1 - Math.exp(-index / 4.5))
    return Math.max(2, ramp + Math.sin(index * 1.7) * target * 0.025 + Math.cos(index * 0.9) * target * 0.018)
  })
}

export function createDemoView(state: string | null, now = Date.now(), error: string | null = null): View {
  const scenario: DemoState = demoStates.includes(state as DemoState) ? state as DemoState : 'result'
  const view: View = { revision: 1, testing: null, last: null, failure: null, estimate: { upTo: true, mb: 600 }, network: { online: true, metered: null, offlineSinceMs: null }, meaning: null, plan: null, historyCount: 0, notice: null, settings: { theme: 'system', traySpeed: false, autostart: true, autoCheckUpdates: true, watchOutages: true, notifyOutages: true, planDownloadMbps: 500, planUploadMbps: 50 }, checking: false, diagnosis: null, outageSinceMs: null }
  if (scenario === 'never') return viewSchema.parse(view)
  view.last = demoReport(now)
  view.meaning = demoMeaning
  view.historyCount = 14
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
  if (scenario === 'metered') view.network.metered = 'hotspot'
  if (scenario.startsWith('check-')) view.diagnosis = demoDiagnosis(scenario.slice(6), now)
  if (scenario === 'checking') view.checking = true
  if (scenario === 'outage') view.outageSinceMs = new Date(now).setHours(14, 2, 0, 0)
  if (scenario === 'stutter' && view.last) {
    view.last.download.loadedMs = 180
    view.meaning = demoStutter
  }
  if (scenario === 'offline') view.network = { online: false, metered: null, offlineSinceMs: new Date(now).setHours(14, 2, 0, 0) }
  if (scenario === 'stale' && view.last) view.last.startedAtMs = now - 2 * 24 * 60 * minute - 5 * minute
  if (scenario === 'capped' && view.last) {
    view.last.download = { ...view.last.download, mbps: 942.6, p10: 918, p90: 960, capped: true }
    view.estimate = { upTo: false, mb: 480 }
  }
  if (scenario === 'varied' && view.last) view.last.download = { ...view.last.download, mbps: 86.4, p10: 52, p90: 118, varied: true }
  if (scenario === 'partial' && view.last) {
    view.last.latency = null
    view.last.upload = null
    view.last.uploadError = 'stalled'
    view.last.download.loadedMs = null
    view.meaning = { health: 'fair', headline: 'Upload not measured', lines: [{ topic: 'Video', text: 'Good for 4K video', health: 'good' }, { topic: 'Calls', text: 'Upload not measured', health: 'fair' }] }
  }
  if (scenario === 'noplan') view.settings = { ...view.settings, planDownloadMbps: null, planUploadMbps: null }
  if (scenario === 'failed') {
    const parsed = testErrorSchema.safeParse(error)
    view.failure = { error: parsed.success ? parsed.data : 'stalled', atMs: now - 2 * minute }
    if (view.last) view.last = { ...view.last, startedAtMs: now - 3 * 60 * minute }
  }
  view.plan = view.last && comparePlan(view.last, view.settings)
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

/** Two weeks of tests with a slow evening and one failure. */
export function demoHistory(now: number): Entry[] {
  const hour = 3_600_000
  const plan: [number, number, number, number][] = [
    [0.2, 312, 41, 14], [5, 298, 40, 15], [21, 305, 41, 14], [27, 144, 38, 31], [46, 301, 39, 15], [52, 310, 41, 13],
    [70, 287, 37, 16], [76, 162, 35, 28], [95, 306, 40, 14], [118, 299, 41, 15], [140, 309, 42, 14], [166, 296, 40, 15], [190, 303, 41, 14],
    [196, 151, 36, 30], [220, 140, 34, 33], [244, 158, 35, 29], [268, 305, 41, 14], [292, 300, 40, 15],
  ]
  const entries: Entry[] = plan.map(([hoursAgo, down, up, latency]) => {
    const report = demoReport(now)
    report.startedAtMs = now - hoursAgo * hour - 12 * 60_000
    report.download = { ...report.download, mbps: down, p10: down * 0.95, p90: down * 1.04 }
    report.upload = report.upload && { ...report.upload, mbps: up }
    report.latency = report.latency && { ...report.latency, medianMs: latency }
    return { kind: 'result', report }
  })
  entries.splice(4, 0, { kind: 'failed', atMs: now - 30 * hour, error: 'stalled' })
  entries.splice(2, 0, { kind: 'outage', startMs: now - 9 * hour, endMs: now - 9 * hour + 7 * 60_000 })
  return entries
}

export function createDemoUpdateState(scenario: string | null): UpdateState {
  return scenario === 'update-available' ? { kind: 'available', version: '0.9.0' } : { kind: 'idle' }
}

type Step = Diagnosis['steps'][number]
const step = (key: Step['key'], status: Step['status'], text: string): Step =>
  ({ key, label: { wifi: 'Wi-Fi', router: 'Router', dns: 'DNS', internet: 'Internet' }[key], status, text })

export function demoDiagnosis(kind: string, now = Date.now()): Diagnosis {
  const wifi = step('wifi', 'good', 'Strong · -52 dBm · 866 Mbps')
  const router = step('router', 'good', 'Answers in 3 ms')
  const dns = step('dns', 'good', 'Lookups in 18 ms')
  const internet = step('internet', 'good', 'Cloudflare in 24 ms')
  const at = now - 20_000
  switch (kind) {
    case 'isp': return { status: 'bad', headline: 'Your Wi-Fi is fine. The problem is past your router.', steps: [wifi, router, step('dns', 'bad', 'Lookups fail'), step('internet', 'bad', 'Cloudflare not reachable')], checkedAtMs: at }
    case 'router': return { status: 'bad', headline: 'Your router isn’t answering. Restart it, or check the cable.', steps: [wifi, step('router', 'bad', 'Not answering'), step('dns', 'bad', 'Lookups fail'), step('internet', 'bad', 'Cloudflare not reachable')], checkedAtMs: at }
    case 'wifi': return { status: 'bad', headline: 'Your Wi-Fi signal is weak (-81 dBm). Move closer to the router.', steps: [step('wifi', 'bad', 'Weak · -81 dBm · 24 Mbps'), step('router', 'good', 'Answers in 9 ms'), dns, internet], checkedAtMs: at }
    case 'dns': return { status: 'fair', headline: 'Websites are slow to open: looking up names takes 450 ms.', steps: [wifi, router, step('dns', 'fair', 'Slow lookups · 450 ms'), internet], checkedAtMs: at }
    default: return { status: 'good', headline: 'Everything looks fine from here.', steps: [wifi, router, dns, internet], checkedAtMs: at }
  }
}
