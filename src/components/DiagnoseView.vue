<script setup lang="ts">
import { computed } from 'vue'
import AppIcon, { type IconName } from './AppIcon.vue'
import { relativeAge } from '../lib/format'
import type { Diagnosis } from '../lib/model'

const props = defineProps<{ diagnosis: Diagnosis | null; checking: boolean; now: number }>()
defineEmits<{ back: []; check: [] }>()

const icons: Record<Diagnosis['steps'][number]['key'], IconName> = { wifi: 'wifi', router: 'router', dns: 'dns', internet: 'globe' }
const placeholder: { key: keyof typeof icons; label: string }[] = [
  { key: 'wifi', label: 'Wi-Fi' },
  { key: 'router', label: 'Router' },
  { key: 'dns', label: 'DNS' },
  { key: 'internet', label: 'Internet' },
]
const tone = computed(() => props.diagnosis?.status === 'bad' ? 'bad' : props.diagnosis?.status === 'fair' ? 'fair' : 'good')
const statusWord = { good: 'Good', fair: 'Slow', bad: 'Problem', skipped: 'Not checked' } as const
</script>

<template>
  <div class="view diagnose-view">
    <header class="page-header">
      <button class="icon-button" aria-label="Back" @click="$emit('back')"><AppIcon name="back" :size="17"/></button>
      <h1>Connection</h1>
      <span v-if="diagnosis && !checking" class="settings-autosave num">Checked {{ relativeAge(diagnosis.checkedAtMs, now) }}</span>
    </header>

    <section v-if="checking" class="card diagnose-card" aria-busy="true">
      <p class="diagnose-headline checking" role="status"><span class="dot pulsing"/>Checking your connection</p>
      <ul class="check-steps">
        <li v-for="item in placeholder" :key="item.key" class="check-step pending">
          <span class="check-icon"><AppIcon :name="icons[item.key]" :size="16"/></span>
          <span class="check-label">{{ item.label }}</span>
          <span class="check-bar"/>
        </li>
      </ul>
    </section>

    <section v-else-if="diagnosis" class="card diagnose-card">
      <p class="diagnose-headline" :class="`tone-${tone}`" role="status">
        <AppIcon :name="tone === 'good' ? 'check' : 'alert'" :size="17"/>
        <span>{{ diagnosis.headline }}</span>
      </p>
      <ul class="check-steps">
        <li v-for="item in diagnosis.steps" :key="item.key" class="check-step" :class="`status-${item.status}`">
          <span class="check-icon"><AppIcon :name="icons[item.key]" :size="16"/></span>
          <span class="check-label">{{ item.label }}</span>
          <span class="check-text num">{{ item.text }}</span>
          <span class="check-mark" role="img" :aria-label="statusWord[item.status]"/>
        </li>
      </ul>
    </section>

    <section v-else class="card never">
      <span class="never-mark" aria-hidden="true"><AppIcon name="pulse" :size="20"/></span>
      <h1>Is it you or the internet?</h1>
      <p>Checks your Wi-Fi, your router, your DNS server and Cloudflare, one after another. Takes a few seconds.</p>
    </section>

    <footer class="foot">
      <span class="note">Uses about 10 kB. No speed test.</span>
      <button class="button button-primary" :disabled="checking" @click="$emit('check')"><AppIcon name="pulse" :size="14"/>{{ diagnosis ? 'Check again' : 'Check' }}</button>
    </footer>
  </div>
</template>
