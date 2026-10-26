import { PLAN_MAX_MBPS, type PlanComparison, type Report, type Settings, type Share } from './model.ts'

/** Mirrors `core::plan::share` for the browser preview. */
export function share(measured: number, paidMbps: number, atLeast: boolean): Share {
  const percent = Math.round(Math.max(0, measured) / paidMbps * 100)
  const health = percent >= 80 ? 'good' : atLeast || percent >= 50 ? 'fair' : 'poor'
  return { paidMbps, percent, atLeast, health }
}

export function comparePlan(report: Report, settings: Settings): PlanComparison | null {
  const download = settings.planDownloadMbps === null ? null : share(report.download.mbps, settings.planDownloadMbps, report.download.capped)
  const upload = settings.planUploadMbps === null || !report.upload ? null : share(report.upload.mbps, settings.planUploadMbps, report.upload.capped)
  return download || upload ? { download, upload } : null
}

export function shareText(value: Share): string {
  return `${value.atLeast ? 'At least ' : ''}${value.percent}% of your ${value.paidMbps} Mbps plan`
}

/** A plan speed typed in Settings: a whole number of Mbps, empty for none. */
export function parsePlan(text: string): number | null | 'invalid' {
  const trimmed = text.trim()
  if (!trimmed) return null
  if (!/^\d+$/.test(trimmed)) return 'invalid'
  const value = Number(trimmed)
  return value >= 1 && value <= PLAN_MAX_MBPS ? value : 'invalid'
}
