<script setup lang="ts">
import { computed } from 'vue'
import AppSelect from '../ui/AppSelect.vue'
import { monthlyText, nextText, scheduleOptions } from '../../lib/schedule'
import { scheduleSchema, type Schedule } from '../../lib/model'

const props = defineProps<{ schedule: Schedule; estimateMb: number; lastTestMs: number | null; now: number; metered: boolean; disabled?: boolean }>()
const emit = defineEmits<{ save: [schedule: Schedule] }>()
const powerSaving = navigator.userAgent.includes('Windows') ? 'Battery Saver' : 'Low Power Mode'
const status = computed(() => [monthlyText(props.schedule, props.estimateMb), nextText(props.schedule, props.lastTestMs, props.now, props.metered)].filter(Boolean).join(' · '))

function choose(value: string): void {
  const parsed = scheduleSchema.safeParse(value)
  if (parsed.success && parsed.data !== props.schedule) emit('save', parsed.data)
}
</script>

<template>
  <section class="settings-section">
    <h2>Scheduled tests</h2>
    <div class="setting-row">
      <div><span id="schedule-label" class="setting-label">Test automatically</span><p>Never on a hotspot, a metered connection or in {{ powerSaving }}.</p></div>
      <AppSelect :model-value="schedule" :options="scheduleOptions" aria-labelledby="schedule-label" :disabled="disabled" @update:model-value="choose"/>
    </div>
    <p v-if="status" class="settings-note num" role="status">{{ status }}</p>
  </section>
</template>
