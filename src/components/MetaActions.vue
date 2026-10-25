<script setup lang="ts">
import AppIcon from './AppIcon.vue'
import AppTooltip from './ui/AppTooltip.vue'
import { shortcut } from '../lib/format'

defineProps<{ history?: boolean; updateAvailable?: boolean }>()
defineEmits<{ check: []; history: []; settings: [] }>()
</script>

<template>
  <span class="meta-actions">
    <AppTooltip :text="`Check connection (${shortcut('K')})`"><button class="icon-button" aria-label="Check connection" @click="$emit('check')"><AppIcon name="pulse" :size="16"/></button></AppTooltip>
    <AppTooltip v-if="history" text="History"><button class="icon-button" aria-label="History" @click="$emit('history')"><AppIcon name="history" :size="16"/></button></AppTooltip>
    <AppTooltip :text="`Settings (${shortcut(',')})`"><button class="icon-button settings-button" :aria-label="updateAvailable ? 'Settings, update available' : 'Settings'" @click="$emit('settings')"><AppIcon name="settings" :size="16"/><span v-if="updateAvailable" class="update-dot" aria-hidden="true"/></button></AppTooltip>
  </span>
</template>
