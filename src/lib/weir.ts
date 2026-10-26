import { onMounted, onUnmounted, ref } from 'vue'
import { demo, desktop, invoke, listen, query } from './bridge.ts'
import { comparePlan } from './plan.ts'
import { createDemoView, demoDiagnosis, demoHistory, demoMeaning, demoProgress, demoReport, demoSamples } from './demo.ts'
import { emptyView, historySchema, startSchema, viewSchema, type Entry, type Phase, type Settings, type View } from './model.ts'

export function useWeir() {
  const view = ref<View>(demo ? createDemoView(query.get('state'), Date.now(), query.get('error')) : emptyView)
  const loading = ref(desktop && !demo)
  const connectionError = ref<string | null>(null)
  const now = ref(Date.now())
  const samples = ref<number[]>([])
  const confirming = ref(false)
  const message = ref<string | null>(null)
  let messageTimer: ReturnType<typeof setTimeout> | undefined
  const history = ref<Entry[]>(demo ? demoHistory(Date.now()) : [])
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
    const historyChanged = result.data.historyCount !== view.value.historyCount
    track(result.data)
    view.value = result.data
    connectionError.value = null
    now.value = Date.now()
    if (historyChanged) void loadHistory()
  }

  async function loadHistory(): Promise<void> {
    if (demo || !desktop) return
    const result = historySchema.safeParse(await invoke('get_history', { limit: 500 }).catch(() => null))
    if (result.success) history.value = result.data
  }

  function say(text: string): void {
    clearTimeout(messageTimer)
    message.value = text
    messageTimer = setTimeout(() => { message.value = null }, 4_000)
  }

  async function copy(text: string): Promise<void> {
    try {
      await navigator.clipboard.writeText(text)
    } catch {
      const area = document.createElement('textarea')
      area.value = text
      area.setAttribute('readonly', '')
      area.style.position = 'fixed'
      area.style.opacity = '0'
      document.body.append(area)
      area.select()
      const copied = document.execCommand('copy')
      area.remove()
      if (!copied) return say('Couldn’t copy. Select the text instead.')
    }
    say('Copied the summary')
  }

  async function exportHistory(): Promise<void> {
    if (demo) return say(`Saved weir-history-${new Date().toISOString().slice(0, 10)}.csv`)
    try {
      const saved = await invoke('export_history')
      if (typeof saved === 'string') say(`Saved ${saved}`)
    } catch (error) {
      say(typeof error === 'string' ? error : 'Weir couldn’t export the history.')
    }
  }

  async function checkConnection(): Promise<void> {
    if (view.value.checking) return
    if (demo) {
      view.value = { ...view.value, revision: view.value.revision + 1, checking: true }
      setTimeout(() => { view.value = { ...view.value, revision: view.value.revision + 1, checking: false, diagnosis: demoDiagnosis('fine') } }, 1_600)
      return
    }
    await invoke('check_connection').catch(() => { say('Weir couldn’t check the connection.') })
  }

  async function saveSettings(settings: Settings): Promise<void> {
    if (demo) {
      view.value = { ...view.value, revision: view.value.revision + 1, settings, plan: view.value.last && comparePlan(view.value.last, settings) }
      return
    }
    try {
      accept(await invoke('save_settings', { settings }))
    } catch (error) {
      say(typeof error === 'string' ? error : 'Weir couldn’t save settings.')
    }
  }

  async function clearHistory(): Promise<void> {
    if (demo) {
      history.value = []
      view.value = { ...view.value, revision: view.value.revision + 1, historyCount: 0 }
      return
    }
    await invoke('clear_history').catch(() => { connectionError.value = 'Weir couldn’t clear the history.' })
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
        next.plan = comparePlan(next.last, next.settings)
        next.historyCount += 1
        history.value = [{ kind: 'result', report: next.last }, ...history.value]
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
      await loadHistory()
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
    clearTimeout(messageTimer)
    stopListening?.()
  })

  return { view, loading, connectionError, now, samples, confirming, history, message, start, cancel, clearHistory, exportHistory, saveSettings, checkConnection, copy }
}
