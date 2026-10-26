<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue'
import AppIcon from './AppIcon.vue'
import AppTooltip from './ui/AppTooltip.vue'
import MetaActions from './MetaActions.vue'
import { clock, errorCopy, estimateText, formatMb, shortcut, summaryText, formatMbps, formatMs, meteredCopy, meteredLabel, relativeAge, server } from '../lib/format'
import { loadedMs, type View } from '../lib/model'
import { shareText } from '../lib/plan'

const props = defineProps<{ view: View; now: number; busy?: boolean; confirming?: boolean; updateAvailable?: boolean }>()
defineEmits<{ test: [confirmed: boolean]; dismiss: []; history: []; settings: []; check: []; copy: [text: string] }>()
const notNow = ref<HTMLButtonElement | null>(null)
const details = ref(false)
const stale = computed(() => props.view.last ? props.now - props.view.last.startedAtMs > 24 * 3_600_000 : false)
const metered = computed(() => props.view.network.metered)
const offline = computed(() => !props.view.network.online || props.view.outageSinceMs !== null)
const offlineSince = computed(() => props.view.network.offlineSinceMs ?? props.view.outageSinceMs)
const note = computed(() => metered.value ? `${meteredLabel[metered.value]} · ${estimateText(props.view.estimate)}` : estimateText(props.view.estimate))
watch(() => props.confirming, async open => {
  if (!open) return
  await nextTick()
  notNow.value?.focus()
})

const report = computed(() => props.view.last)
const failure = computed(() => props.view.failure)
const loaded = computed(() => report.value ? loadedMs(report.value) : null)
const downloadLabel = computed(() => report.value ? `Download ${formatMbps(report.value.download.mbps)} megabits per second` : '')
const varied = computed(() => report.value?.download.varied ? `Varied ${formatMbps(report.value.download.p10)}–${formatMbps(report.value.download.p90)} Mbps` : null)
</script>

<template>
  <div class="view result-view">
    <template v-if="failure">
      <div class="meta"><span class="where"><span class="dot dot-danger"/>Test failed · {{ clock(failure.atMs) }}</span><MetaActions :history="Boolean(view.historyCount)" :update-available="updateAvailable" @check="$emit('check')" @history="$emit('history')" @settings="$emit('settings')"/></div>
      <section class="card failure" role="alert">
        <span class="failure-icon"><AppIcon name="alert" :size="20"/></span>
        <h1>{{ errorCopy[failure.error].title }}</h1>
        <p>{{ errorCopy[failure.error].detail }}</p>
        <button class="text-button failure-check" @click="$emit('check')"><AppIcon name="pulse" :size="13"/>Check connection</button>
        <p v-if="report" class="previous num">Last result {{ formatMbps(report.download.mbps) }} Mbps down · {{ relativeAge(report.startedAtMs, now) }}</p>
      </section>
    </template>

    <template v-else-if="report">
      <div class="meta"><span class="where" :class="{ stale }"><AppIcon v-if="report.scheduled" class="where-icon" name="clock" :size="12"/><span v-else class="dot"/><span v-if="report.scheduled" class="sr-only">Scheduled test, </span>Tested {{ relativeAge(report.startedAtMs, now) }} · {{ server(report.provider, report.colo) }}</span><MetaActions :history="Boolean(view.historyCount)" :update-available="updateAvailable" @check="$emit('check')" @history="$emit('history')" @settings="$emit('settings')"/></div>
      <section class="card" aria-label="Last result">
        <h1 class="sr-only">Last result</h1>
        <p class="label" aria-hidden="true">Download</p>
        <p class="hero num"><span class="sr-only">{{ downloadLabel }}</span><span class="big" aria-hidden="true">{{ formatMbps(report.download.mbps) }}</span><span class="unit" aria-hidden="true">Mbps</span></p>
        <div v-if="view.plan?.download" class="plan-share" :class="`plan-${view.plan.download.health}`">
          <span class="plan-meter" aria-hidden="true"><span :style="{ width: `${Math.min(100, view.plan.download.percent)}%` }"/></span>
          <span class="plan-text num">{{ shareText(view.plan.download) }}</span>
        </div>
        <p v-if="varied" class="hint num">{{ varied }}</p>
        <p v-if="report.download.capped" class="hint">Stopped at the data limit, speed may be higher</p>
        <p v-if="stale" class="hint">Over a day old. Test again for a current number.</p>
        <div class="stats">
          <div class="stat">
            <p class="label">Upload</p>
            <template v-if="report.upload">
              <p class="value num">{{ formatMbps(report.upload.mbps) }}<small>Mbps</small></p>
              <p v-if="report.upload.rough" class="sub">Rough estimate</p>
              <p v-else-if="view.plan?.upload" class="sub num" :class="`plan-sub-${view.plan.upload.health}`">{{ view.plan.upload.atLeast ? 'At least ' : '' }}{{ view.plan.upload.percent }}% of {{ view.plan.upload.paidMbps }} Mbps plan</p>
            </template>
            <template v-else>
              <p class="value num wait">–</p>
              <p class="sub">{{ report.uploadError ? errorCopy[report.uploadError].title : 'Not measured' }}</p>
            </template>
          </div>
          <div class="stat">
            <p class="label">Latency</p>
            <template v-if="report.latency">
              <p class="value num">{{ formatMs(report.latency.medianMs) }}<small>ms</small></p>
              <p class="sub num">{{ loaded === null ? 'Not measured under load' : `${formatMs(loaded)} ms under load` }}</p>
            </template>
            <template v-else>
              <p class="value num wait">–</p>
              <p class="sub">Not measured</p>
            </template>
          </div>
        </div>
        <div v-if="view.meaning" class="verdict" :class="`verdict-${view.meaning.health}`">
          <button class="verdict-toggle" :aria-expanded="details" aria-controls="meaning-lines" @click="details = !details">
            <AppIcon :name="view.meaning.health === 'good' ? 'check' : 'alert'" :size="15"/>
            <span class="verdict-text">{{ view.meaning.headline }}</span>
            <AppIcon class="verdict-chevron" name="chevron" :size="14"/>
          </button>
          <ul v-if="details" id="meaning-lines" class="meaning-lines">
            <li v-for="line in view.meaning.lines" :key="line.topic">
              <span class="meaning-topic">{{ line.topic }}</span>
              <span class="meaning-text num">{{ line.text }}</span>
              <span class="meaning-mark" :class="`mark-${line.health}`"  role="img" :aria-label="line.health"/>
            </li>
            <li class="meaning-data"><span class="meaning-topic">Data</span><span class="meaning-text num">Used {{ formatMb(report.bytesUsed) }}</span><span/></li>
            <li class="meaning-copy"><button class="text-button" @click="$emit('copy', summaryText(report, view.meaning, view.plan))"><AppIcon name="copy" :size="13"/>Copy summary</button></li>
          </ul>
        </div>
      </section>
    </template>

    <template v-else>
      <div class="meta"><span class="where"><span class="dot"/>Cloudflare speed test</span><MetaActions :history="false" :update-available="updateAvailable" @check="$emit('check')" @history="$emit('history')" @settings="$emit('settings')"/></div>
      <section class="card never">
        <span class="never-mark" aria-hidden="true"><AppIcon name="weir" :size="22"/></span>
        <h1>Not tested yet</h1>
        <p>Measures download, upload and latency against Cloudflare. Takes about 20 seconds.</p>
      </section>
    </template>

    <p v-if="offline" class="offline-pill" role="status"><AppIcon name="offline" :size="15"/><span class="num">{{ offlineSince ? `No internet since ${clock(offlineSince)}` : 'No internet' }}</span><button class="text-button offline-check" @click="$emit('check')">Check</button></p>
    <section v-if="confirming && metered && !offline" class="confirm" role="alertdialog" aria-labelledby="confirm-title" aria-describedby="confirm-detail">
      <p id="confirm-title" class="confirm-title"><AppIcon name="alert" :size="15"/>{{ meteredCopy[metered] }}</p>
      <p id="confirm-detail" class="confirm-detail num">{{ estimateText(view.estimate) }} of your data.</p>
      <div class="confirm-actions">
        <button ref="notNow" class="button" @click="$emit('dismiss')">Not now</button>
        <button class="button button-primary" @click="$emit('test', true)">Test anyway</button>
      </div>
    </section>
    <footer v-else class="foot">
      <span class="note num" :class="{ 'note-warning': metered }">{{ note }}</span>
      <AppTooltip :text="`Run a speed test (${shortcut('T')})`"><button class="button button-primary" :disabled="busy || !view.network.online" @click="$emit('test', false)"><AppIcon name="play" :size="13"/>{{ report || failure ? (failure ? 'Try again' : 'Test again') : 'Test' }}</button></AppTooltip>
    </footer>
  </div>
</template>
