import type { Entry } from './model.ts'
import { median } from './stats.ts'

export interface HourBucket {
  hour: number
  count: number
  median: number | null
}

export interface SlowHours {
  buckets: HourBucket[]
  usual: number
  slow: { from: number; to: number; median: number } | null
}

export const MIN_TESTS = 10
export const MIN_HOURS = 3
const SLOW_SHARE = 0.8

/** Median download by local hour of day, and the slowest run of hours if one stands out. */
export function slowHours(entries: Entry[], hourOf: (ms: number) => number = ms => new Date(ms).getHours()): SlowHours | null {
  const results = entries.flatMap(entry => entry.kind === 'result' ? [entry.report] : [])
  const byHour: number[][] = Array.from({ length: 24 }, () => [])
  for (const report of results) byHour[hourOf(report.startedAtMs)]!.push(report.download.mbps)
  const buckets = byHour.map((values, hour) => ({ hour, count: values.length, median: median(values) }))
  const covered = buckets.filter(bucket => bucket.count > 0).length
  const usual = median(results.map(report => report.download.mbps))
  if (results.length < MIN_TESTS || covered < MIN_HOURS || usual === null) return null

  const isSlow = (bucket: HourBucket) => bucket.median !== null && bucket.median < usual * SLOW_SHARE
  const measured = buckets.filter(bucket => bucket.median !== null)
  const runs: HourBucket[][] = []
  measured.forEach((bucket, index) => {
    if (!isSlow(bucket)) return
    if (index > 0 && isSlow(measured[index - 1]!)) runs.at(-1)!.push(bucket)
    else runs.push([bucket])
  })
  if (runs.length > 1 && isSlow(measured[0]!) && isSlow(measured.at(-1)!)) runs[0] = [...runs.pop()!, ...runs[0]!]

  let slow: SlowHours['slow'] = null
  for (const run of runs) {
    const value = median(run.flatMap(bucket => byHour[bucket.hour]!))!
    if (!slow || value < slow.median) slow = { from: run[0]!.hour, to: (run.at(-1)!.hour + 1) % 24, median: value }
  }
  return { buckets, usual, slow }
}

export function hourLabel(hour: number): string {
  return `${String(hour).padStart(2, '0')}:00`
}
