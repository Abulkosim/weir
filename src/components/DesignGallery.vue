<script setup lang="ts">
import { ref } from 'vue'
import AppIcon from './AppIcon.vue'
import AppSelect from './ui/AppSelect.vue'
import AppSwitch from './ui/AppSwitch.vue'

const enabled = ref(true)
const theme = ref('system')
const themes = [{ value: 'system', label: 'System' }, { value: 'light', label: 'Light' }, { value: 'dark', label: 'Dark' }]
</script>

<template>
  <div class="gallery">
    <section class="gallery-card">
      <p class="gallery-label">Numbers</p>
      <p class="gallery-hero num">312<span>Mbps</span></p>
      <p class="gallery-stat num">41<span>Mbps</span> · 14<span>ms</span></p>
      <p class="gallery-copy">Latency 14 ms · 62 ms under load</p>
      <p class="gallery-meta">Tested 12 min ago · Cloudflare, Frankfurt</p>
    </section>
    <section class="gallery-card">
      <p class="gallery-label">Verdicts</p>
      <p class="pill pill-good"><AppIcon name="check" :size="15"/>Good for 4K video and calls</p>
      <p class="pill pill-warning"><AppIcon name="alert" :size="15"/>Calls may stutter: latency jumps to 180 ms under load</p>
      <p class="pill pill-danger"><AppIcon name="offline" :size="15"/>No internet since 14:02</p>
    </section>
    <section class="gallery-card">
      <p class="gallery-label">Controls</p>
      <div class="gallery-row">
        <button class="button button-primary"><AppIcon name="play" :size="14"/>Test</button>
        <button class="button">Cancel</button>
        <button class="button button-small">Export CSV</button>
        <button class="icon-button" aria-label="History"><AppIcon name="history" :size="16"/></button>
        <button class="icon-button" aria-label="Settings"><AppIcon name="settings" :size="17"/></button>
      </div>
      <div class="gallery-row gallery-between">
        <label for="gallery-switch">Launch at login</label>
        <AppSwitch id="gallery-switch" v-model="enabled"/>
      </div>
      <div class="gallery-row gallery-between">
        <span>Appearance</span>
        <AppSelect v-model="theme" :options="themes" label="Appearance"/>
      </div>
      <button class="text-button"><AppIcon name="export" :size="14"/>Export history</button>
    </section>
  </div>
</template>

<style scoped>
.gallery { display: grid; gap: 10px; padding: 14px; }
.gallery-card { background: var(--card); border-radius: 10px; padding: 16px; display: grid; gap: 10px; }
.gallery-card p { margin: 0; }
.gallery-label { font-size: 12px; color: var(--tertiary); font-weight: 500; }
.gallery-hero { font-size: 64px; font-weight: 600; letter-spacing: -.045em; line-height: 1; }
.gallery-hero span, .gallery-stat span { font-size: 14px; letter-spacing: 0; font-weight: 500; color: var(--secondary); margin-left: 4px; }
.gallery-stat { font-size: 22px; font-weight: 600; letter-spacing: -.02em; }
.gallery-copy { color: var(--secondary); font-variant-numeric: tabular-nums; }
.gallery-meta { font-size: 12.5px; color: var(--tertiary); font-variant-numeric: tabular-nums; }
.gallery-row { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
.gallery-between { justify-content: space-between; }
.pill { display: flex; align-items: center; gap: 8px; border-radius: 8px; padding: 9px 11px; font-size: 13px; font-weight: 500; }
.pill svg { flex: none; }
.pill-good { background: var(--accent-soft); color: var(--accent-ink); }
.pill-warning { background: var(--warning-soft); color: var(--warning); }
.pill-danger { background: var(--danger-soft); color: var(--danger); }
</style>
