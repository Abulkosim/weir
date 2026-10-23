<script setup lang="ts">
import { computed } from 'vue'
import AppIcon from './AppIcon.vue'
import { errorCopy, formatMbps, formatMs } from '../lib/format'
import { entryTime, type Entry } from '../lib/model'

const props = defineProps<{ entries: Entry[]; now: number }>()
defineEmits<{ back: [] }>()

const results = computed(() => props.entries.flatMap(entry => entry.kind === 'result' ? [entry.report] : []))
const recent = computed(() => results.value.slice(0, 30).reverse())
const sorted = computed(() => results.value.map(report => report.download.mbps).sort((a, b) => a - b))
const median = computed(() => {
  const values = sorted.value
  if (!values.length) return null
  const middle = Math.floor(values.length / 2)
  return values.length % 2 ? values[middle]! : (values[middle - 1]! + values[middle]!) / 2
})
const range = computed(() => sorted.value.length > 1 ? [sorted.value[0]!, sorted.value.at(-1)!] as const : null)

const width = 300
const height = 52
const spark = computed(() => {
  const values = recent.value.map(report => report.download.mbps)
  if (values.length < 2) return null
  const top = Math.max(...values) * 1.08
  const step = width / (values.length - 1)
  const points = values.map((value, index) => [index * step, height - 3 - (value / top) * (height - 8)] as const)
  return {
    line: points.map(([x, y], index) => `${index ? 'L' : 'M'}${x.toFixed(1)} ${y.toFixed(1)}`).join(' '),
    dots: points,
  }
})

function dayLabel(at: number): string {
  const day = new Date(at)
  const today = new Date(props.now)
  const start = (date: Date) => new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime()
  const days = Math.round((start(today) - start(day)) / 86_400_000)
  if (days === 0) return 'Today'
  if (days === 1) return 'Yesterday'
  return day.toLocaleDateString([], { weekday: 'short', day: 'numeric', month: 'short' })
}

const groups = computed(() => {
  const list: { label: string; entries: Entry[] }[] = []
  for (const entry of props.entries) {
    const label = dayLabel(entryTime(entry))
    const group = list.at(-1)
    if (group?.label === label) group.entries.push(entry)
    else list.push({ label, entries: [entry] })
  }
  return list
})

function time(at: number): string {
  return new Date(at).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' })
}
</script>

<template>
  <div class="view history-view">
    <header class="page-header">
      <button class="icon-button" aria-label="Back" @click="$emit('back')"><AppIcon name="back" :size="17"/></button>
      <h1>History</h1>
    </header>

    <section v-if="!entries.length" class="card never">
      <span class="never-mark" aria-hidden="true"><AppIcon name="history" :size="20"/></span>
      <h1>No tests yet</h1>
      <p>Each test is kept on this computer for a year. Nothing leaves your device.</p>
    </section>

    <template v-else>
      <section v-if="median !== null" class="card history-summary" aria-label="Download over recent tests">
        <div class="summary-text">
          <p class="label">Download, last {{ recent.length }} tests</p>
          <p class="summary-value num">{{ formatMbps(median) }}<small>Mbps median</small></p>
          <p v-if="range" class="sub num">{{ formatMbps(range[0]) }}–{{ formatMbps(range[1]) }} Mbps</p>
        </div>
        <svg v-if="spark" class="history-spark" :viewBox="`0 0 ${width} ${height}`" preserveAspectRatio="none" aria-hidden="true">
          <path :d="spark.line" fill="none" stroke="var(--accent)" stroke-width="1.6" stroke-linejoin="round" stroke-linecap="round" vector-effect="non-scaling-stroke"/>
        </svg>
      </section>

      <div class="history-list" role="list" aria-label="Tests">
        <section v-for="group in groups" :key="group.label" class="history-day">
          <h2>{{ group.label }}</h2>
          <ul>
            <li v-for="entry in group.entries" :key="entryTime(entry)" class="history-row num" role="listitem">
              <span class="history-time">{{ time(entryTime(entry)) }}</span>
              <template v-if="entry.kind === 'result'">
                <span class="history-metric" :aria-label="`Download ${formatMbps(entry.report.download.mbps)} Mbps`"><AppIcon name="download" :size="12"/>{{ formatMbps(entry.report.download.mbps) }}</span>
                <span class="history-metric" :aria-label="entry.report.upload ? `Upload ${formatMbps(entry.report.upload.mbps)} Mbps` : 'Upload not measured'"><AppIcon name="upload" :size="12"/>{{ entry.report.upload ? formatMbps(entry.report.upload.mbps) : '–' }}</span>
                <span class="history-latency">{{ entry.report.latency ? `${formatMs(entry.report.latency.medianMs)} ms` : '–' }}</span>
              </template>
              <span v-else class="history-failed">{{ errorCopy[entry.error].title }}</span>
            </li>
          </ul>
        </section>
      </div>
    </template>
  </div>
</template>
