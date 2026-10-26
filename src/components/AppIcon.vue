<script setup lang="ts">
export type IconName = 'weir' | 'history' | 'settings' | 'play' | 'stop' | 'back' | 'chevron' | 'check' | 'tick' | 'close' | 'alert' | 'sun' | 'moon' | 'system' | 'external' | 'download' | 'upload' | 'wifi' | 'offline' | 'export' | 'router' | 'globe' | 'dns' | 'pulse' | 'copy' | 'clock' | 'report'
defineProps<{ name: IconName; size?: number }>()
</script>

<template>
  <svg :width="size ?? 18" :height="size ?? 18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.65" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
    <template v-if="name === 'weir'"><path d="M6.3 17.7a8 8 0 1 1 11.4 0" stroke-width="2.4"/><circle cx="12" cy="12.6" r="1.6" fill="currentColor" stroke="none"/></template>
    <path v-else-if="name === 'history'" d="M3 17.5 8.5 11l4 3.5L21 6"/>
    <template v-else-if="name === 'settings'"><path d="m9 3-.7 2.2-2 .9-2.1-.5L2.7 8.2l1.5 1.7v2.3l-1.5 1.7 1.5 2.6 2.1-.5 2 .9L9 19h3l.7-2.1 2-.9 2.1.5 1.5-2.6-1.5-1.7V9.9l1.5-1.7-1.5-2.6-2.1.5-2-.9L12 3z" transform="translate(1.5 1)"/><circle cx="12" cy="12" r="3"/></template>
    <path v-else-if="name === 'play'" d="M7 5.3v13.4c0 .7.8 1.1 1.4.7l10.3-6.7a.8.8 0 0 0 0-1.4L8.4 4.6C7.8 4.2 7 4.6 7 5.3Z" fill="currentColor" stroke="none"/>
    <rect v-else-if="name === 'stop'" x="6.5" y="6.5" width="11" height="11" rx="2" fill="currentColor" stroke="none"/>
    <path v-else-if="name === 'back'" d="m14 6-6 6 6 6M8 12h12"/>
    <path v-else-if="name === 'chevron'" d="m8 10 4 4 4-4"/>
    <template v-else-if="name === 'check'"><circle cx="12" cy="12" r="9"/><path d="m8.2 12.3 2.6 2.6 5-5.3"/></template>
    <path v-else-if="name === 'tick'" d="m5 12 4 4L19 6"/>
    <path v-else-if="name === 'close'" d="m6 6 12 12M6 18 18 6"/>
    <template v-else-if="name === 'alert'"><circle cx="12" cy="12" r="9"/><path d="M12 7.5v5.5m0 3.5h.01"/></template>
    <template v-else-if="name === 'sun'"><circle cx="12" cy="12" r="4"/><path d="M12 2v2m0 16v2M2 12h2m16 0h2M5 5l1 1m12 12 1 1M5 19l1-1M18 6l1-1"/></template>
    <path v-else-if="name === 'moon'" d="M20 14a8 8 0 0 1-10-10A9 9 0 1 0 20 14Z"/>
    <template v-else-if="name === 'system'"><rect x="3" y="4" width="18" height="13" rx="2"/><path d="M8 21h8m-4-4v4"/></template>
    <template v-else-if="name === 'external'"><path d="M14 3h7v7M21 3l-11 11M10 3H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-5"/></template>
    <path v-else-if="name === 'download'" d="M12 4v13m-5.5-5.5L12 17l5.5-5.5M5 20.5h14"/>
    <path v-else-if="name === 'upload'" d="M12 20V7M6.5 12.5 12 7l5.5 5.5M5 3.5h14"/>
    <template v-else-if="name === 'wifi'"><path d="M2.5 9a14 14 0 0 1 19 0M5.5 12.5a9.5 9.5 0 0 1 13 0M8.6 15.9a5 5 0 0 1 6.8 0"/><circle cx="12" cy="19.2" r="1.2" fill="currentColor" stroke="none"/></template>
    <template v-else-if="name === 'offline'"><path d="M2.5 9a14 14 0 0 1 19 0M5.5 12.5a9.5 9.5 0 0 1 13 0M8.6 15.9a5 5 0 0 1 6.8 0M4 4l16 16"/><circle cx="12" cy="19.2" r="1.2" fill="currentColor" stroke="none"/></template>
    <template v-else-if="name === 'router'"><rect x="3" y="13" width="18" height="7" rx="2"/><path d="M7 16.5h.01M11 16.5h.01M8.5 9.5a5 5 0 0 1 7 0M6 7a8.5 8.5 0 0 1 12 0"/></template>
    <template v-else-if="name === 'globe'"><circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3a14 14 0 0 1 0 18M12 3a14 14 0 0 0 0 18"/></template>
    <template v-else-if="name === 'dns'"><path d="M4 6h10M4 12h16M4 18h7"/><circle cx="18" cy="6" r="2"/><circle cx="15" cy="18" r="2"/></template>
    <path v-else-if="name === 'pulse'" d="M3 12h4l2.5-6 4 12 2.5-6h5"/>
    <template v-else-if="name === 'copy'"><rect x="8" y="8" width="12" height="12" rx="2"/><path d="M16 8V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h2"/></template>
    <template v-else-if="name === 'clock'"><circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/></template>
    <template v-else-if="name === 'report'"><path d="M7 3h7l4 4v13a1 1 0 0 1-1 1H7a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1Z"/><path d="M14 3v4h4M9 13h6M9 17h4"/></template>
    <path v-else-if="name === 'export'" d="M12 15V3.5M7.5 8 12 3.5 16.5 8M5 12v7a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2v-7"/>
  </svg>
</template>
