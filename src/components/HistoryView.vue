<script setup lang="ts">
import { computed } from 'vue'
import AppIcon from './AppIcon.vue'
import AppTooltip from './ui/AppTooltip.vue'
import { errorCopy, formatMbps, formatMs } from '../lib/format'
import { entryTime, type Entry } from '../lib/model'
import { hourLabel, slowHours } from '../lib/hours'

const props = defineProps<{ entries: Entry[]; now: number; planMbps?: number | null }>()
defineEmits<{ back: []; export: []; report: [] }>()

const results = computed(() => props.entries.flatMap(entry => entry.kind === 'result' ? [entry.report] : []))
const recent = computed(() => results.value.slice(0, 30).reverse())
const sorted = computed(() => results.value.map(report => report.download.mbps).sort((a, b) => a - b))
const median = computed(() => {
  const values = sorted.value
  if (!values.length) return null
  const middle = Math.floor(values.length / 2)
  return values.length % 2 ? values[middle]! : (values[middle - 1]! + values[middle]!) / 2
})
const hours = computed(() => slowHours(props.entries))
const tallest = computed(() => Math.max(1, ...(hours.value?.buckets.map(bucket => bucket.median ?? 0) ?? [1])))
const isSlow = (hour: number) => {
  const slow = hours.value?.slow
  if (!slow) return false
  return slow.from <= slow.to ? hour >= slow.from && hour < slow.to : hour >= slow.from || hour < slow.to
}
const range = computed(() => sorted.value.length > 1 ? [sorted.value[0]!, sorted.value.at(-1)!] as const : null)

const width = 300
const height = 52
const planShare = computed(() => props.planMbps && median.value !== null ? Math.round(median.value / props.planMbps * 100) : null)
const spark = computed(() => {
  const values = recent.value.map(report => report.download.mbps)
  if (values.length < 2) return null
  const top = Math.max(...values, props.planMbps ?? 0) * 1.08
  const step = width / (values.length - 1)
  const y = (value: number) => height - 3 - (value / top) * (height - 8)
  const points = values.map((value, index) => [index * step, y(value)] as const)
  return {
    line: points.map(([x, y], index) => `${index ? 'L' : 'M'}${x.toFixed(1)} ${y.toFixed(1)}`).join(' '),
    dots: points,
    plan: props.planMbps ? y(props.planMbps) : null,
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
      <template v-if="entries.length">
        <AppTooltip text="A PDF of the last 30 days for your provider"><button class="button button-small" @click="$emit('report')"><AppIcon name="report" :size="13"/>Report</button></AppTooltip>
        <AppTooltip text="Export CSV"><button class="icon-button" aria-label="Export CSV" @click="$emit('export')"><AppIcon name="export" :size="16"/></button></AppTooltip>
      </template>
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
          <p v-if="planShare !== null" class="sub num">{{ planShare }}% of your {{ planMbps }} Mbps plan</p>
        </div>
        <svg v-if="spark" class="history-spark" :viewBox="`0 0 ${width} ${height}`" preserveAspectRatio="none" aria-hidden="true">
          <line v-if="spark.plan !== null" class="spark-plan" x1="0" :x2="width" :y1="spark.plan" :y2="spark.plan" vector-effect="non-scaling-stroke"/>
          <path :d="spark.line" fill="none" stroke="var(--accent)" stroke-width="1.6" stroke-linejoin="round" stroke-linecap="round" vector-effect="non-scaling-stroke"/>
        </svg>
      </section>

      <section v-if="hours" class="card hours-card" aria-labelledby="hours-title">
        <p id="hours-title" class="label">Download by hour of day</p>
        <div class="hour-bars" aria-hidden="true">
          <span v-for="bucket in hours.buckets" :key="bucket.hour" class="hour-bar" :class="{ empty: bucket.median === null, slow: isSlow(bucket.hour) }" :style="{ height: bucket.median === null ? undefined : `${Math.max(8, bucket.median / tallest * 100)}%` }"/>
        </div>
        <div class="hour-axis num" aria-hidden="true"><span>00</span><span>06</span><span>12</span><span>18</span><span>24</span></div>
        <p class="sub num">{{ hours.slow ? `Slowest around ${hourLabel(hours.slow.from)}–${hourLabel(hours.slow.to)}: ${formatMbps(hours.slow.median)} Mbps, usually ${formatMbps(hours.usual)}` : `No hour stands out. Usually ${formatMbps(hours.usual)} Mbps.` }}</p>
      </section>

      <div class="history-list">
        <section v-for="group in groups" :key="group.label" class="history-day">
          <h2>{{ group.label }}</h2>
          <ul>
            <li v-for="entry in group.entries" :key="entryTime(entry)" class="history-row num">
              <span class="history-time">{{ time(entryTime(entry)) }}<template v-if="entry.kind === 'result' && entry.report.scheduled"><AppIcon class="history-scheduled" name="clock" :size="11"/><span class="sr-only">, scheduled</span></template></span>
              <template v-if="entry.kind === 'result'">
                <span class="history-metric"><span class="sr-only">{{ `Download ${formatMbps(entry.report.download.mbps)} Mbps` }}</span><span aria-hidden="true" class="history-metric-shown"><AppIcon name="download" :size="12"/>{{ formatMbps(entry.report.download.mbps) }}</span></span>
                <span class="history-metric"><span class="sr-only">{{ entry.report.upload ? `Upload ${formatMbps(entry.report.upload.mbps)} Mbps` : 'Upload not measured' }}</span><span aria-hidden="true" class="history-metric-shown"><AppIcon name="upload" :size="12"/>{{ entry.report.upload ? formatMbps(entry.report.upload.mbps) : '–' }}</span></span>
                <span class="history-latency">{{ entry.report.latency ? `${formatMs(entry.report.latency.medianMs)} ms` : '–' }}</span>
              </template>
              <span v-else-if="entry.kind === 'failed'" class="history-failed">{{ errorCopy[entry.error].title }}</span>
              <span v-else class="history-failed">No internet until {{ time(entry.endMs) }} · {{ Math.max(1, Math.ceil((entry.endMs - entry.startMs) / 60_000)) }} min</span>
            </li>
          </ul>
        </section>
      </div>
    </template>
  </div>
</template>
