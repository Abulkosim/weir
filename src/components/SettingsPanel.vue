<script setup lang="ts">
import { ref } from 'vue'
import { AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogOverlay, AlertDialogPortal, AlertDialogRoot, AlertDialogTitle } from 'reka-ui'
import type { Settings } from '../lib/model'
import AppIcon from './AppIcon.vue'
import AppSwitch from './ui/AppSwitch.vue'
import UpdateSettings from './settings/UpdateSettings.vue'
import type { UpdateState } from '../lib/updater'

const props = defineProps<{ settings: Settings; historyCount: number; busy?: boolean; updateState: UpdateState; updateBusy: boolean; demo: boolean }>()
const emit = defineEmits<{ back: []; save: [settings: Settings]; export: []; clear: []; checkUpdate: []; installUpdate: [] }>()
const confirmClear = ref(false)
const isMac = navigator.userAgent.includes('Mac')
const themes: { id: Settings['theme']; label: string; icon: 'system' | 'sun' | 'moon' }[] = [
  { id: 'system', label: 'System', icon: 'system' },
  { id: 'light', label: 'Light', icon: 'sun' },
  { id: 'dark', label: 'Dark', icon: 'moon' },
]

function patch(change: Partial<Settings>): void {
  emit('save', { ...props.settings, ...change })
}
</script>

<template>
  <div class="view settings-view">
    <header class="page-header">
      <button class="icon-button" aria-label="Back" @click="$emit('back')"><AppIcon name="back" :size="17"/></button>
      <h1>Settings</h1>
      <span class="settings-autosave">Saves automatically</span>
    </header>

    <div class="settings-content">
      <section class="settings-section">
        <h2>Appearance</h2>
        <div class="theme-options" role="group" aria-label="Appearance">
          <button v-for="theme in themes" :key="theme.id" class="theme-option" :class="{ selected: settings.theme === theme.id }" :aria-pressed="settings.theme === theme.id" :disabled="busy" @click="patch({ theme: theme.id })">
            <span class="theme-thumbnail" :class="`thumbnail-${theme.id}`"><span/><span/><span/></span>
            <span><AppIcon :name="theme.icon" :size="13"/>{{ theme.label }}</span>
            <span v-if="settings.theme === theme.id" class="theme-check"><AppIcon name="tick" :size="10"/></span>
          </button>
        </div>
      </section>

      <section class="settings-section">
        <h2>Tray &amp; startup</h2>
        <div v-if="isMac" class="setting-row">
          <div><span id="tray-speed-label" class="setting-label">Show speed next to icon</span><p>The last download speed, in Mbps.</p></div>
          <AppSwitch :model-value="settings.traySpeed" aria-labelledby="tray-speed-label" :disabled="busy" @update:model-value="patch({ traySpeed: $event })"/>
        </div>
        <div class="setting-row">
          <span id="autostart-label" class="setting-label">Launch at login</span>
          <AppSwitch :model-value="settings.autostart" aria-labelledby="autostart-label" :disabled="busy" @update:model-value="patch({ autostart: $event })"/>
        </div>
      </section>

      <section class="settings-section">
        <h2>Outages</h2>
        <div class="setting-row">
          <div><span id="watch-label" class="setting-label">Watch for outages</span><p>A tiny check about once a minute, under 2 kB. Outages go into History.</p></div>
          <AppSwitch :model-value="settings.watchOutages" aria-labelledby="watch-label" :disabled="busy" @update:model-value="patch({ watchOutages: $event })"/>
        </div>
        <div class="setting-row">
          <span id="notify-label" class="setting-label">Notify when it drops and returns</span>
          <AppSwitch :model-value="settings.notifyOutages" aria-labelledby="notify-label" :disabled="busy || !settings.watchOutages" @update:model-value="patch({ notifyOutages: $event })"/>
        </div>
      </section>

      <section class="settings-section">
        <h2>History</h2>
        <div class="setting-row">
          <div><span class="setting-label num">{{ historyCount === 1 ? '1 test' : `${historyCount} tests` }}</span><p>Kept on this computer for a year.</p></div>
          <button class="button button-small" :disabled="busy || !historyCount" @click="$emit('export')"><AppIcon name="export" :size="13"/>Export CSV</button>
        </div>
        <div class="setting-row">
          <span class="setting-label">Clear history</span>
          <button class="button button-small" :disabled="busy || !historyCount" @click="confirmClear = true">Clear…</button>
        </div>
      </section>

      <UpdateSettings :state="updateState" :busy="updateBusy" :demo="demo" :auto-check="settings.autoCheckUpdates" :settings-busy="Boolean(busy)" @check="$emit('checkUpdate')" @install="$emit('installUpdate')" @auto-check="patch({ autoCheckUpdates: $event })"/>

      <section class="settings-section about">
        <h2>About</h2>
        <p>Tests run against Cloudflare’s speed test servers. Weir has no accounts, ads or tracking, and history never leaves this computer.</p>
        <p class="settings-footnote num">github.com/Abulkosim/weir</p>
      </section>
    </div>

    <AlertDialogRoot v-model:open="confirmClear">
      <AlertDialogPortal to=".popover">
        <AlertDialogOverlay class="modal-backdrop">
          <AlertDialogContent class="confirm-dialog">
            <AlertDialogTitle as="h2">Clear history?</AlertDialogTitle>
            <AlertDialogDescription as="p">Deletes all {{ historyCount }} saved tests. Export a CSV first if you want to keep them.</AlertDialogDescription>
            <div class="confirm-actions">
              <AlertDialogCancel class="button">Keep history</AlertDialogCancel>
              <AlertDialogAction class="button button-danger" @click="$emit('clear')">Clear history</AlertDialogAction>
            </div>
          </AlertDialogContent>
        </AlertDialogOverlay>
      </AlertDialogPortal>
    </AlertDialogRoot>
  </div>
</template>
