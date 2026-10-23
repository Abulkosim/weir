<script setup lang="ts">
import { computed } from 'vue'
import LiveChart from './LiveChart.vue'
import { formatMb, formatMbps, formatMs, server } from '../lib/format'
import type { Progress } from '../lib/model'
import { useTween } from '../lib/tween'

const props = defineProps<{ progress: Progress; samples: number[] }>()
defineEmits<{ cancel: [] }>()

const steps = [
  { phase: 'latency', label: 'Latency', spanMs: 1_500 },
  { phase: 'download', label: 'Download', spanMs: 10_000 },
  { phase: 'upload', label: 'Upload', spanMs: 8_000 },
] as const
const order = { connecting: 0, latency: 0, download: 1, upload: 2 } as const
const current = computed(() => order[props.progress.phase])
const span = computed(() => steps[current.value].spanMs)
const fill = (index: number) => index < current.value ? 1 : index > current.value ? 0 : Math.min(0.95, props.progress.phaseMs / span.value)
const transferring = computed(() => props.progress.phase === 'download' || props.progress.phase === 'upload')
const live = useTween(() => transferring.value ? props.progress.liveMbps : null)
const heroLabel = computed(() => props.progress.phase === 'upload' ? 'Upload' : props.progress.phase === 'download' ? 'Download' : 'Latency')
const status = computed(() => ({ connecting: 'Connecting to Cloudflare', latency: 'Measuring latency', download: 'Measuring download', upload: 'Measuring upload' })[props.progress.phase])
</script>

<template>
  <div class="view testing-view">
    <div class="meta">
      <span class="where"><span class="dot pulsing"/>{{ progress.colo ? server('Cloudflare', progress.colo) : 'Cloudflare' }}</span>
      <span class="sr-only" role="status">{{ status }}</span>
    </div>
    <section class="card" aria-label="Test in progress">
      <ol class="steps">
        <li v-for="(step, index) in steps" :key="step.phase" class="step" :class="{ on: index === current, done: index < current }">
          <i><b :style="{ transform: `scaleX(${fill(index)})` }"/></i>{{ step.label }}
        </li>
      </ol>
      <p class="label">{{ heroLabel }}</p>
      <p v-if="transferring" class="hero num" aria-hidden="true"><span class="big">{{ live === null ? '–' : formatMbps(live) }}</span><span class="unit">Mbps</span></p>
      <p v-else class="hero num" aria-hidden="true"><span class="big">{{ progress.latencyMs === null ? '–' : formatMs(progress.latencyMs) }}</span><span class="unit">ms</span></p>
      <div class="chart">
        <LiveChart v-if="transferring" :samples="samples" :span-ms="span"/>
      </div>
      <div class="stats">
        <div class="stat">
          <template v-if="progress.phase === 'upload'">
            <p class="label">Download</p>
            <p class="value num">{{ formatMbps(progress.downloadMbps ?? 0) }}<small>Mbps</small></p>
            <p class="sub">Done</p>
          </template>
          <template v-else>
            <p class="label">Upload</p>
            <p class="value num wait">–</p>
            <p class="sub">Next</p>
          </template>
        </div>
        <div class="stat">
          <template v-if="transferring">
            <p class="label">Latency</p>
            <p class="value num">{{ progress.latencyMs === null ? '–' : formatMs(progress.latencyMs) }}<small v-if="progress.latencyMs !== null">ms</small></p>
            <p class="sub num">{{ progress.loadedMs === null ? 'Under load next' : `${formatMs(progress.loadedMs)} ms under load` }}</p>
          </template>
          <template v-else>
            <p class="label">Download</p>
            <p class="value num wait">–</p>
            <p class="sub">Next</p>
          </template>
        </div>
      </div>
    </section>
    <footer class="foot">
      <span class="note num">Used {{ formatMb(progress.bytesUsed) }} so far</span>
      <button class="button" @click="$emit('cancel')">Cancel</button>
    </footer>
  </div>
</template>
