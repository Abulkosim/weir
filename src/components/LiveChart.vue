<script setup lang="ts">
import { computed } from 'vue'

const props = defineProps<{ samples: number[]; spanMs: number; tickMs?: number }>()
const width = 300
const height = 64

const points = computed(() => {
  const tick = props.tickMs ?? 100
  const top = Math.max(1, ...props.samples) * 1.15
  return props.samples.map((value, index) => [
    Math.min(width, (index * tick / props.spanMs) * width),
    height - (value / top) * (height - 4),
  ] as const)
})
const line = computed(() => points.value.map(([x, y], index) => `${index ? 'L' : 'M'}${x.toFixed(1)} ${y.toFixed(1)}`).join(' '))
const area = computed(() => {
  const last = points.value.at(-1)
  return last ? `${line.value} L${last[0].toFixed(1)} ${height} L0 ${height} Z` : ''
})
const head = computed(() => points.value.at(-1) ?? null)
</script>

<template>
  <svg class="live-chart" :viewBox="`0 0 ${width} ${height}`" preserveAspectRatio="none" aria-hidden="true">
    <defs>
      <linearGradient id="live-fill" x1="0" x2="0" y1="0" y2="1">
        <stop offset="0" stop-color="var(--accent)" stop-opacity=".22"/>
        <stop offset="1" stop-color="var(--accent)" stop-opacity="0"/>
      </linearGradient>
      <linearGradient id="live-stroke" x1="0" x2="1" y1="0" y2="0">
        <stop offset="0" stop-color="var(--accent-2)"/>
        <stop offset="1" stop-color="var(--accent)"/>
      </linearGradient>
    </defs>
    <line x1="0" :x2="width" :y1="height - .5" :y2="height - .5" stroke="var(--line)" vector-effect="non-scaling-stroke"/>
    <path v-if="area" :d="area" fill="url(#live-fill)"/>
    <path v-if="line" :d="line" fill="none" stroke="url(#live-stroke)" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" vector-effect="non-scaling-stroke"/>
  </svg>
  <span v-if="head" class="live-head" :style="{ left: `${head[0] / width * 100}%`, top: `${head[1] / height * 100}%` }" aria-hidden="true"/>
</template>

<style scoped>
.live-chart { display: block; width: 100%; height: 100%; overflow: visible; }
.live-head { position: absolute; width: 7px; height: 7px; margin: -3.5px 0 0 -3.5px; border-radius: 50%; background: var(--accent); box-shadow: 0 0 0 5px color-mix(in srgb, var(--accent) 18%, transparent); transition: left .1s linear, top .1s linear; }
</style>
