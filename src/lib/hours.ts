import type { Entry } from './model.ts'

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

function median(values: number[]): number | null {
  if (!values.length) return null
  const sorted = [...values].sort((a, b) => a - b)
  const middle = Math.floor(sorted.length / 2)
  return sorted.length % 2 ? sorted[middle]! : (sorted[middle - 1]! + sorted[middle]!) / 2
}

/** Median download by local hour of day, and the slowest run of hours if one stands out. */
export function slowHours(entries: Entry[], hourOf: (ms: number) => number = ms => new Date(ms).getHours()): SlowHours | null {
  const results = entries.flatMap(entry => entry.kind === 'result' ? [entry.report] : [])
  const byHour: number[][] = Array.from({ length: 24 }, () => [])
  for (const report of results) byHour[hourOf(report.startedAtMs)]!.push(report.download.mbps)
  const buckets = byHour.map((values, hour) => ({ hour, count: values.length, median: median(values) }))
  const covered = buckets.filter(bucket => bucket.count > 0).length
  const usual = median(results.map(report => report.download.mbps))
  if (results.length < MIN_TESTS || covered < MIN_HOURS || usual === null) return null

  let slow: SlowHours['slow'] = null
  let run: HourBucket[] = []
  const finish = () => {
    if (!run.length) return
    const values = run.flatMap(bucket => byHour[bucket.hour]!)
    const value = median(values)!
    if (!slow || value < slow.median) slow = { from: run[0]!.hour, to: (run.at(-1)!.hour + 1) % 24, median: value }
    run = []
  }
  for (const bucket of buckets) {
    if (bucket.median !== null && bucket.median < usual * SLOW_SHARE) run.push(bucket)
    else if (bucket.median !== null || !run.length) finish()
  }
  finish()
  return { buckets, usual, slow }
}

export function hourLabel(hour: number): string {
  return `${String(hour).padStart(2, '0')}:00`
}
