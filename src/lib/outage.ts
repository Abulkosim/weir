import type { Causes } from './model.ts'

type Known = 'router' | 'pastRouter' | 'noNetwork'

const known: Known[] = ['router', 'pastRouter', 'noNetwork']
const phrase: Record<Known, string> = { router: 'at your router', pastRouter: 'past your router', noNetwork: 'no network connection' }

/** Where an outage broke, in the same words as Rust's `Causes::sentence`. */
export function causeText(causes?: Causes): string | null {
  if (!causes) return null
  const total = causes.noNetwork + causes.router + causes.pastRouter + causes.unknown
  const [cause, count] = known.map(key => [key, causes[key]] as const).reduce((best, next) => next[1] > best[1] ? next : best)
  if (count === 0) return null
  if (count === total) {
    if (cause === 'router') return 'Your router didn’t answer. The problem was at home: the Wi-Fi, a cable or the router.'
    if (cause === 'pastRouter') return 'Your router was reachable throughout. The break was past it.'
    return 'Your computer had no network connection.'
  }
  if (count * 2 > total) return `Mostly ${phrase[cause]}: ${count} of ${total} checks.`
  if (known.filter(key => causes[key] > 0).length === 1) {
    const text = phrase[cause]
    return `${text[0]!.toUpperCase()}${text.slice(1)} in ${count} of ${total} checks. ${total - count === 1 ? 'The other was unclear.' : 'The rest were unclear.'}`
  }
  const parts = ([
    [causes.router, 'at your router'],
    [causes.pastRouter, causes.router > 0 ? 'past it' : 'past your router'],
    [causes.noNetwork, 'with no network'],
    [causes.unknown, 'unclear'],
  ] as const).filter(([value]) => value > 0).map(([value, label], index) => index === 0 ? `${value} of ${total} ${label}` : `${value} ${label}`)
  return `Checks disagreed: ${parts.join(', ')}.`
}
