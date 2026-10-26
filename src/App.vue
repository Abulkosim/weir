<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watchEffect } from 'vue'
import { TooltipProvider } from 'reka-ui'
import { navigateSchema } from './lib/model'
import { demo, desktop, invoke, isWindows, listen, query } from './lib/bridge'
import DesignGallery from './components/DesignGallery.vue'
import HistoryView from './components/HistoryView.vue'
import DiagnoseView from './components/DiagnoseView.vue'
import SettingsPanel from './components/SettingsPanel.vue'
import ResultView from './components/ResultView.vue'
import TestingView from './components/TestingView.vue'
import { useWeir } from './lib/weir'
import { useUpdater } from './lib/updater'
import { createDemoUpdateState } from './lib/demo'
import { arrowOffset, fitHeight, nextPlacement, type PopoverPlacement } from './lib/popover'

const { view, connectionError, now, samples, confirming, history, message, start, cancel, exportHistory, exportReport, clearHistory, saveSettings, checkConnection, copy } = useWeir()
const updater = useUpdater({ demo, desktop, initial: demo ? createDemoUpdateState(query.get('update')) : undefined })
const requested = demo ? query.get('page') : null
const startState = demo ? query.get('state') ?? '' : ''
const page = ref<'overview' | 'history' | 'settings' | 'diagnose'>(requested === 'history' || requested === 'settings' || requested === 'diagnose' ? requested : startState.startsWith('check') ? 'diagnose' : 'overview')
function navigate(payload: unknown): void {
  const target = navigateSchema.safeParse(payload)
  if (!target.success) return
  if (target.data === 'check') return openCheck()
  if (target.data === 'history' || target.data === 'settings') {
    page.value = target.data
    return
  }
  page.value = 'overview'
  if (target.data === 'confirm') void start(false)
}
function openCheck(): void {
  page.value = 'diagnose'
  if (!view.value.diagnosis || Date.now() - view.value.diagnosis.checkedAtMs > 60_000) void checkConnection()
}
const media = window.matchMedia('(prefers-color-scheme: dark)')
const systemDark = ref(media.matches)
const forcedTheme = demo ? query.get('theme') : null
const scenario = demo ? query.get('state') : null
const dark = computed(() => {
  const theme = forcedTheme ?? view.value.settings.theme
  return theme === 'system' ? systemDark.value : theme === 'dark'
})
const placement = ref<PopoverPlacement>({ revision: 0, arrowX: null })
const viewportWidth = ref(window.innerWidth)
const arrowX = computed(() => !desktop || demo || isWindows ? null : arrowOffset(placement.value.arrowX, viewportWidth.value))
const shell = ref<HTMLElement | null>(null)
const popover = ref<HTMLElement | null>(null)
const scroller = ref<HTMLElement | null>(null)
const content = ref<HTMLElement | null>(null)
const framed = desktop && !demo || demo && query.get('frame') === '1'
const embedded = demo && query.get('embed') === '1'
const measured = ref(0)
let frame = 0
let observer: ResizeObserver | undefined
let stopPlacement: (() => void) | undefined
let stopNavigate: (() => void) | undefined
let destroyed = false

watchEffect(() => {
  document.documentElement.dataset.theme = dark.value ? 'dark' : 'light'
  document.documentElement.dataset.platform = isWindows ? 'windows' : 'mac'
})

function measure(): void {
  frame = 0
  if (!shell.value || !popover.value) return
  const style = getComputedStyle(shell.value)
  // Natural height: the popover chrome plus the full content, even while the window
  // is still shorter than that content and the scroller is clipping it. Fractional
  // sizes, rounded once in fitHeight, so it doesn't wobble a pixel as the window resizes.
  const height = (element: HTMLElement | null) => element?.getBoundingClientRect().height ?? 0
  const chrome = height(popover.value) - height(scroller.value)
  const natural = chrome + (content.value ? height(content.value) : scroller.value?.scrollHeight ?? 0)
  const fitted = fitHeight(parseFloat(style.paddingTop), parseFloat(style.paddingBottom), natural)
  if (fitted === measured.value) return
  measured.value = fitted
  if (desktop && !demo) invoke('fit_popover', { height: fitted }).catch(() => {})
  if (embedded) window.parent.postMessage({ weirHeight: fitted }, '*')
}

function scheduleMeasure(): void {
  if (!frame) frame = requestAnimationFrame(measure)
}

function syncTheme(event: MediaQueryListEvent): void { systemDark.value = event.matches }
function syncWidth(): void { viewportWidth.value = window.innerWidth }
function onKeydown(event: KeyboardEvent): void {
  if ((event.metaKey || event.ctrlKey) && event.key === ',') {
    event.preventDefault()
    page.value = 'settings'
    return
  }
  if (event.defaultPrevented || document.querySelector('[role="alertdialog"][data-state="open"]')) return
  const command = event.metaKey || event.ctrlKey
  if (command && event.key.toLowerCase() === 't' && !view.value.testing) {
    event.preventDefault()
    page.value = 'overview'
    void start(false)
    return
  }
  if (command && event.key.toLowerCase() === 'k') {
    event.preventDefault()
    openCheck()
    return
  }
  if (event.key !== 'Escape') return
  if (confirming.value) confirming.value = false
  else if (page.value !== 'overview') page.value = 'overview'
  else if (desktop && !demo) invoke('hide_window').catch(() => {})
}

onMounted(async () => {
  media.addEventListener('change', syncTheme)
  window.addEventListener('resize', syncWidth)
  window.addEventListener('keydown', onKeydown)
  if (typeof ResizeObserver === 'function' && popover.value) {
    observer = new ResizeObserver(scheduleMeasure)
    observer.observe(popover.value)
    if (content.value) observer.observe(content.value)
  }
  scheduleMeasure()
  if (!desktop || demo) return
  try {
    const unlistenNavigate = await listen('navigate', navigate)
    if (destroyed) { unlistenNavigate(); return }
    stopNavigate = unlistenNavigate
    const unlisten = await listen('popover_placement_changed', payload => { placement.value = nextPlacement(placement.value, payload) })
    if (destroyed) { unlisten(); return }
    stopPlacement = unlisten
    placement.value = nextPlacement(placement.value, await invoke('get_popover_placement'))
  } catch {
    placement.value = { ...placement.value, arrowX: null }
  }
})

onUnmounted(() => {
  destroyed = true
  observer?.disconnect()
  cancelAnimationFrame(frame)
  stopPlacement?.()
  stopNavigate?.()
  media.removeEventListener('change', syncTheme)
  window.removeEventListener('resize', syncWidth)
  window.removeEventListener('keydown', onKeydown)
})
</script>

<template>
  <TooltipProvider :delay-duration="450">
  <div class="preview-stage" :class="{ 'desktop-stage': framed, 'embed-stage': embedded }">
    <div ref="shell" class="app-shell" :data-fit-height="measured">
      <div v-if="!desktop || demo || arrowX !== null" class="popover-arrow" :style="arrowX === null ? undefined : { left: `${arrowX - 6}px` }" aria-hidden="true"/>
      <div ref="popover" class="popover">
        <DesignGallery v-if="scenario === 'design'"/>
        <main v-else ref="scroller">
          <div ref="content">
          <p v-if="connectionError" class="connection-error" role="alert">{{ connectionError }}</p>
          <p v-if="view.notice" class="notice" role="status">{{ view.notice }}</p>
          <DiagnoseView v-if="page === 'diagnose'" :diagnosis="view.diagnosis" :checking="view.checking" :now="now" @back="page = 'overview'" @check="checkConnection"/>
          <HistoryView v-else-if="page === 'history'" :entries="history" :now="now" :plan-mbps="view.settings.planDownloadMbps" @back="page = 'overview'" @export="exportHistory" @report="exportReport"/>
          <SettingsPanel v-else-if="page === 'settings'" :settings="view.settings" :history-count="view.historyCount" :update-state="updater.state.value" :update-busy="updater.busy.value" :demo="demo" @back="page = 'overview'" @save="saveSettings" @export="exportHistory" @report="exportReport" @clear="clearHistory" @check-update="updater.check" @install-update="updater.install"/>
          <TestingView v-else-if="view.testing" :progress="view.testing" :samples="samples" @cancel="cancel"/>
          <ResultView v-else :view="view" :now="now" :update-available="updater.available.value" :confirming="confirming" @test="start" @dismiss="confirming = false" @history="page = 'history'" @settings="page = 'settings'" @check="openCheck" @copy="copy"/>
          </div>
        </main>
        <Transition name="toast"><p v-if="message" class="toast" role="status">{{ message }}</p></Transition>
        <div v-if="!desktop || demo" class="preview-ribbon">Interface preview<span>Sample data</span></div>
      </div>
    </div>
  </div>
  </TooltipProvider>
</template>
