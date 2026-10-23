<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watchEffect } from 'vue'
import { demo, desktop, invoke, isWindows, listen, query } from './lib/bridge'
import DesignGallery from './components/DesignGallery.vue'
import HistoryView from './components/HistoryView.vue'
import ResultView from './components/ResultView.vue'
import TestingView from './components/TestingView.vue'
import { useWeir } from './lib/weir'
import { arrowOffset, fitHeight, nextPlacement, type PopoverPlacement } from './lib/popover'

const { view, connectionError, now, samples, confirming, history, start, cancel } = useWeir()
const page = ref<'overview' | 'history'>(demo && query.get('page') === 'history' ? 'history' : 'overview')
const media = window.matchMedia('(prefers-color-scheme: dark)')
const systemDark = ref(media.matches)
const forcedTheme = demo ? query.get('theme') : null
const scenario = demo ? query.get('state') : null
const dark = computed(() => forcedTheme ? forcedTheme === 'dark' : systemDark.value)
const placement = ref<PopoverPlacement>({ revision: 0, arrowX: null })
const viewportWidth = ref(window.innerWidth)
const arrowX = computed(() => !desktop || demo || isWindows ? null : arrowOffset(placement.value.arrowX, viewportWidth.value))
const shell = ref<HTMLElement | null>(null)
const popover = ref<HTMLElement | null>(null)
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
  const height = fitHeight(parseFloat(style.paddingTop), parseFloat(style.paddingBottom), popover.value.offsetHeight)
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
  <div class="preview-stage" :class="{ 'desktop-stage': desktop && !demo }">
    <div ref="shell" class="app-shell" :data-fit-height="measured">
      <div v-if="!desktop || demo || arrowX !== null" class="popover-arrow" :style="arrowX === null ? undefined : { left: `${arrowX - 6}px` }" aria-hidden="true"/>
      <div ref="popover" class="popover">
        <DesignGallery v-if="scenario === 'design'"/>
        <main v-else>
          <p v-if="connectionError" class="connection-error" role="alert">{{ connectionError }}</p>
          <p v-if="view.notice" class="notice" role="status">{{ view.notice }}</p>
          <HistoryView v-if="page === 'history'" :entries="history" :now="now" @back="page = 'overview'"/>
          <TestingView v-else-if="view.testing" :progress="view.testing" :samples="samples" @cancel="cancel"/>
          <ResultView v-else :view="view" :now="now" :confirming="confirming" @test="start" @dismiss="confirming = false" @history="page = 'history'"/>
        </main>
        <div v-if="!desktop || demo" class="preview-ribbon">Interface preview<span>Sample data</span></div>
      </div>
    </div>
  </div>
</template>
