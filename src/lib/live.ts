import type { Phase, View } from './model.ts'

export interface Samples {
  phase: Phase | null
  phaseMs: number
  values: number[]
}

export const noSamples: Samples = { phase: null, phaseMs: -1, values: [] }

/** One chart point per progress tick; other updates during a test (settings, outage watch) add none. */
export function nextSamples(current: Samples, view: View): Samples {
  const progress = view.testing
  if (!progress || (progress.phase !== 'download' && progress.phase !== 'upload')) {
    return { ...noSamples, phase: progress?.phase ?? null }
  }
  const base = progress.phase === current.phase ? current : { ...noSamples, phase: progress.phase }
  if (progress.liveMbps === null || progress.phaseMs <= base.phaseMs) return base
  return { phase: progress.phase, phaseMs: progress.phaseMs, values: [...base.values, progress.liveMbps] }
}

/** Changes whenever History gains or loses an entry, even when pruning keeps the count. */
export function historyKey(view: View): string {
  return [view.historyCount, view.last?.startedAtMs, view.failure?.atMs, view.outageSinceMs].join(':')
}
