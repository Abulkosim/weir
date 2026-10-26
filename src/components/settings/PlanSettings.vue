<script setup lang="ts">
import { ref, watch } from 'vue'
import { parsePlan } from '../../lib/plan'

const props = defineProps<{ download: number | null; upload: number | null; disabled?: boolean }>()
const emit = defineEmits<{ save: [change: { planDownloadMbps?: number | null; planUploadMbps?: number | null }] }>()
const fields = [
  { key: 'planDownloadMbps', label: 'Download' },
  { key: 'planUploadMbps', label: 'Upload' },
] as const
const text = ref({ planDownloadMbps: '', planUploadMbps: '' })
const invalid = ref({ planDownloadMbps: false, planUploadMbps: false })

watch(() => [props.download, props.upload] as const, ([download, upload]) => {
  text.value = { planDownloadMbps: download === null ? '' : String(download), planUploadMbps: upload === null ? '' : String(upload) }
  invalid.value = { planDownloadMbps: false, planUploadMbps: false }
}, { immediate: true })

function commit(key: (typeof fields)[number]['key']): void {
  const value = parsePlan(text.value[key])
  invalid.value[key] = value === 'invalid'
  if (value === 'invalid') return
  const current = key === 'planDownloadMbps' ? props.download : props.upload
  if (value !== current) emit('save', { [key]: value })
}
</script>

<template>
  <section class="settings-section">
    <h2>Your plan</h2>
    <p class="settings-intro">The speeds your provider sells. Results show how close you get.</p>
    <div v-for="field in fields" :key="field.key" class="setting-row">
      <label :for="field.key" class="setting-label">{{ field.label }}<span class="sr-only"> speed in Mbps</span></label>
      <span class="speed-field">
        <input :id="field.key" v-model="text[field.key]" class="text-input num" inputmode="numeric" autocomplete="off" placeholder="Not set" :aria-invalid="invalid[field.key] || undefined" :aria-describedby="invalid[field.key] ? 'plan-error' : undefined" :disabled="disabled" @change="commit(field.key)" @keydown.enter="commit(field.key)">
        <span class="speed-unit" aria-hidden="true">Mbps</span>
      </span>
    </div>
    <p v-if="invalid.planDownloadMbps || invalid.planUploadMbps" id="plan-error" class="settings-error" role="alert">Enter a whole number of Mbps, or leave it empty.</p>
  </section>
</template>
