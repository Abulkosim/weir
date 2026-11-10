import { entryTime, type Entry } from './model.ts'
import { median } from './stats.ts'

export interface MonthSummary {
  /** `null` for the current month, else the month's name, e.g. "October". */
  name: string | null
  tests: number
  failed: number
  download: number | null
  upload: number | null
  latency: number | null
  /** Outage time inside the month; an outage that started earlier counts from the 1st. */
  outageMs: number
  outages: number
  /** Median download of the month before, to compare against. */
  before: number | null
}

function bounds(now: number, back: number): [number, number] {
  const date = new Date(now)
  return [new Date(date.getFullYear(), date.getMonth() - back, 1).getTime(), new Date(date.getFullYear(), date.getMonth() - back + 1, 1).getTime()]
}

function month(entries: Entry[], from: number, to: number) {
  const inside = entries.filter(entry => entry.kind === 'outage' ? entry.endMs > from && entry.startMs < to : entryTime(entry) >= from && entryTime(entry) < to)
  const reports = inside.flatMap(entry => entry.kind === 'result' ? [entry.report] : [])
  const outages = inside.flatMap(entry => entry.kind === 'outage' ? [Math.min(entry.endMs, to) - Math.max(entry.startMs, from)] : [])
  return {
    tests: reports.length,
    failed: inside.filter(entry => entry.kind === 'failed').length,
    download: median(reports.map(report => report.download.mbps)),
    upload: median(reports.flatMap(report => report.upload ? [report.upload.mbps] : [])),
    latency: median(reports.flatMap(report => report.latency ? [report.latency.medianMs] : [])),
    outageMs: outages.reduce((sum, ms) => sum + ms, 0),
    outages: outages.length,
  }
}

/** Medians for this calendar month; last month instead while this one has no tests yet. */
export function monthSummary(entries: Entry[], now: number): MonthSummary | null {
  const [thisFrom, thisTo] = bounds(now, 0)
  const [lastFrom, lastTo] = bounds(now, 1)
  const [olderFrom] = bounds(now, 2)
  const current = month(entries, thisFrom, thisTo)
  const last = month(entries, lastFrom, lastTo)
  if (current.tests) return { name: null, ...current, before: last.download }
  if (!last.tests) return null
  const name = new Date(lastFrom).toLocaleDateString([], { month: 'long' })
  return { name, ...last, before: month(entries, olderFrom, lastFrom).download }
}

export function outageText(summary: MonthSummary, watching: boolean): string {
  if (!summary.outages) return watching ? 'No outages seen' : 'Outage watch is off'
  const minutes = Math.max(1, Math.ceil(summary.outageMs / 60_000))
  const time = minutes < 60 ? `${minutes} min` : `${Math.floor(minutes / 60)} h ${minutes % 60} min`
  return `Offline ${time} · ${summary.outages} ${summary.outages === 1 ? 'outage' : 'outages'}`
}
