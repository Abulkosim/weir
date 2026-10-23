import { onMounted, onUnmounted, ref } from 'vue'
import { demo, desktop, invoke, listen, query } from './bridge.ts'
import { createDemoView, demoMeaning, demoProgress, demoReport, demoSamples } from './demo.ts'
import { emptyView, startSchema, viewSchema, type Phase, type View } from './model.ts'

export function useWeir() {
  const view = ref<View>(demo ? createDemoView(query.get('state')) : emptyView)
  const loading = ref(desktop && !demo)
  const connectionError = ref<string | null>(null)
  const now = ref(Date.now())
  const samples = ref<number[]>([])
  const confirming = ref(false)
  let samplePhase: Phase | null = null
  let stopListening: (() => void) | undefined
  let simulation: ReturnType<typeof setInterval> | undefined
  let destroyed = false
  const clock = setInterval(() => { now.value = Date.now() }, 15_000)

  if (demo && view.value.testing?.liveMbps) {
    const phase = view.value.testing.phase
    samplePhase = phase
    samples.value = demoSamples(view.value.testing.phaseMs / 100, phase === 'upload' ? 41 : 300)
  }

  function track(next: View): void {
    const progress = next.testing
    if (!progress || (progress.phase !== 'download' && progress.phase !== 'upload')) {
      samplePhase = progress?.phase ?? null
      samples.value = []
      return
    }
    if (progress.phase !== samplePhase) {
      samplePhase = progress.phase
      samples.value = []
    }
    if (progress.liveMbps !== null) samples.value = [...samples.value, progress.liveMbps]
  }

  function accept(payload: unknown): void {
    const result = viewSchema.safeParse(payload)
    if (!result.success) {
      connectionError.value = 'Weir received an unreadable update.'
      return
    }
    if (result.data.revision < view.value.revision) return
    track(result.data)
    view.value = result.data
    connectionError.value = null
    now.value = Date.now()
  }

  function simulate(): void {
    let step = 0
    clearInterval(simulation)
    simulation = setInterval(() => {
      const testing = demoProgress(step++)
      const next = { ...view.value, revision: view.value.revision + 1, testing, failure: null }
      if (!testing) {
        clearInterval(simulation)
        next.last = demoReport(Date.now())
        next.last.startedAtMs = Date.now()
        next.meaning = demoMeaning
      }
      track(next)
      view.value = next
    }, 100)
  }

  async function start(confirmed = false): Promise<void> {
    if (view.value.testing) return
    if (view.value.network.metered && !confirmed) {
      confirming.value = true
      return
    }
    confirming.value = false
    if (demo) return simulate()
    try {
      const result = startSchema.safeParse(await invoke('start_test', { confirmed }))
      if (result.success && result.data === 'needsConfirmation') confirming.value = true
    } catch {
      connectionError.value = 'Weir couldn’t start the test.'
    }
  }

  async function cancel(): Promise<void> {
    if (demo) {
      clearInterval(simulation)
      view.value = { ...view.value, revision: view.value.revision + 1, testing: null }
      samples.value = []
      return
    }
    await invoke('cancel_test').catch(() => {})
  }

  onMounted(async () => {
    if (!desktop || demo) return
    try {
      const unlisten = await listen('state_changed', accept)
      if (destroyed) { unlisten(); return }
      stopListening = unlisten
      accept(await invoke('get_state'))
    } catch {
      connectionError.value = 'Weir couldn’t connect to its background service.'
    } finally {
      loading.value = false
    }
  })

  onUnmounted(() => {
    destroyed = true
    clearInterval(clock)
    clearInterval(simulation)
    stopListening?.()
  })

  return { view, loading, connectionError, now, samples, confirming, start, cancel }
}
