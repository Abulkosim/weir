import { clock } from './format.ts'
import type { Schedule } from './model.ts'

export const scheduleOptions: { value: Schedule; label: string }[] = [
  { value: 'off', label: 'Off' },
  { value: 'every6Hours', label: 'Every 6 hours' },
  { value: 'every12Hours', label: 'Every 12 hours' },
  { value: 'daily', label: 'Once a day' },
]

const hours: Record<Schedule, number | null> = { off: null, every6Hours: 6, every12Hours: 12, daily: 24 }

/** Data a month at this schedule, from the per-test estimate in MB. */
export function monthlyText(schedule: Schedule, mb: number): string | null {
  const every = hours[schedule]
  if (every === null) return null
  const gb = mb * (30 * 24 / every) / 1000
  const amount = gb < 10 ? gb.toFixed(1).replace(/\.0$/, '') : String(Math.round(gb))
  return `About ${mb} MB a test, ${amount} GB a month`
}

/** When the next scheduled test is due, as the scheduler in Rust decides it. */
export function nextText(schedule: Schedule, lastTestMs: number | null, now: number, metered: boolean): string | null {
  const every = hours[schedule]
  if (every === null) return null
  if (metered) return 'Paused while this connection is metered'
  const next = lastTestMs === null ? now : lastTestMs + every * 3_600_000
  if (next - now < 5 * 60_000) return 'Next test in a few minutes'
  const day = (at: number) => new Date(at).toDateString()
  const tomorrow = day(now + 86_400_000) === day(next)
  return `Next test ${tomorrow ? 'tomorrow ' : ''}around ${clock(next)}`
}
