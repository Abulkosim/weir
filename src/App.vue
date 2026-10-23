<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watchEffect } from 'vue'
import { demo, desktop, invoke, isWindows, listen, query } from './lib/bridge'
import DesignGallery from './components/DesignGallery.vue'
import HistoryView from './components/HistoryView.vue'
import SettingsPanel from './components/SettingsPanel.vue'
import ResultView from './components/ResultView.vue'
import TestingView from './components/TestingView.vue'
import { useWeir } from './lib/weir'
import { arrowOffset, fitHeight, nextPlacement, type PopoverPlacement } from './lib/popover'

const { view, connectionError, now, samples, confirming, history, message, start, cancel, exportHistory, clearHistory, saveSettings } = useWeir()
const requested = demo ? query.get('page') : null
const page = ref<'overview' | 'history' | 'settings'>(requested === 'history' || requested === 'settings' ? requested : 'overview')
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
const measured = ref(0)
let frame = 0
let observer: ResizeObserver | undefined
let stopPlacement: (() => void) | undefined
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
  // is still shorter than that content and the scroller is clipping it.
  const chrome = popover.value.offsetHeight - (scroller.value?.clientHeight ?? 0)
  const natural = chrome + (content.value?.offsetHeight ?? scroller.value?.scrollHeight ?? 0)
  const height = fitHeight(parseFloat(style.paddingTop), parseFloat(style.paddingBottom), natural)
  if (height === measured.value) return
  measured.value = height
  if (desktop && !demo) invoke('fit_popover', { height }).catch(() => {})
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
  media.removeEventListener('change', syncTheme)
  window.removeEventListener('resize', syncWidth)
  window.removeEventListener('keydown', onKeydown)
})
</script>

<template>
  <div class="preview-stage" :class="{ 'desktop-stage': framed }">
    <div ref="shell" class="app-shell" :data-fit-height="measured">
      <div v-if="!desktop || demo || arrowX !== null" class="popover-arrow" :style="arrowX === null ? undefined : { left: `${arrowX - 6}px` }" aria-hidden="true"/>
      <div ref="popover" class="popover">
        <DesignGallery v-if="scenario === 'design'"/>
        <main v-else ref="scroller">
          <div ref="content">
          <p v-if="connectionError" class="connection-error" role="alert">{{ connectionError }}</p>
          <p v-if="view.notice" class="notice" role="status">{{ view.notice }}</p>
          <HistoryView v-if="page === 'history'" :entries="history" :now="now" @back="page = 'overview'" @export="exportHistory"/>
          <SettingsPanel v-else-if="page === 'settings'" :settings="view.settings" :history-count="view.historyCount" @back="page = 'overview'" @save="saveSettings" @export="exportHistory" @clear="clearHistory"/>
          <TestingView v-else-if="view.testing" :progress="view.testing" :samples="samples" @cancel="cancel"/>
          <ResultView v-else :view="view" :now="now" :confirming="confirming" @test="start" @dismiss="confirming = false" @history="page = 'history'" @settings="page = 'settings'"/>
          </div>
        </main>
        <Transition name="toast"><p v-if="message" class="toast" role="status">{{ message }}</p></Transition>
        <div v-if="!desktop || demo" class="preview-ribbon">Interface preview<span>Sample data</span></div>
      </div>
    </div>
  </div>
</template>
