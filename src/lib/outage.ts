import type { Causes } from './model.ts'

type Known = 'router' | 'pastRouter' | 'noNetwork'

const phrase: Record<Known, string> = {
  router: 'at your router',
  pastRouter: 'past your router',
  noNetwork: 'no network connection',
}

/** Where an outage broke, in one sentence. Mirrors `Causes::sentence` in `core/outage.rs`. */
export function causeText(causes: Causes | undefined): string | null {
  if (!causes) return null
  const total = causes.noNetwork + causes.router + causes.pastRouter + causes.unknown
  const known = (['router', 'pastRouter', 'noNetwork'] as const).filter(cause => causes[cause] > 0)
  const top = known.reduce<Known | null>((best, cause) => best === null || causes[cause] > causes[best] ? cause : best, null)
  if (top === null) return null
  const count = causes[top]
  if (count === total) {
    if (top === 'router') return 'Your router didn’t answer. The problem was at home: the Wi-Fi, a cable or the router.'
    if (top === 'pastRouter') return 'Your router was reachable throughout. The break was past it.'
    return 'Your computer had no network connection.'
  }
  if (count * 2 > total) return `Mostly ${phrase[top]}: ${count} of ${total} checks.`
  if (known.length === 1) {
    const words = phrase[top]
    return `${words[0]!.toUpperCase()}${words.slice(1)} in ${count} of ${total} checks. The rest were unclear.`
  }
  const parts: [number, string][] = []
  if (causes.router) parts.push([causes.router, 'at your router'])
  if (causes.pastRouter) parts.push([causes.pastRouter, causes.router ? 'past it' : 'past your router'])
  if (causes.noNetwork) parts.push([causes.noNetwork, 'with no network'])
  if (causes.unknown) parts.push([causes.unknown, 'unclear'])
  return `Checks disagreed: ${parts.map(([n, words], index) => index ? `${n} ${words}` : `${n} of ${total} ${words}`).join(', ')}.`
}
