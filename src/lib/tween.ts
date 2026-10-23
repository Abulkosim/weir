import { onUnmounted, ref, watch, type Ref } from 'vue'

const reduced = typeof window !== 'undefined' && window.matchMedia('(prefers-reduced-motion: reduce)').matches

export function easeOut(t: number): number {
  return 1 - (1 - t) ** 3
}

/** Follows `source` smoothly so live numbers count toward each new value. */
export function useTween(source: () => number | null, duration = 420): Ref<number | null> {
  const shown = ref<number | null>(source())
  let frame = 0
  watch(source, target => {
    cancelAnimationFrame(frame)
    if (target === null || shown.value === null || reduced) {
      shown.value = target
      return
    }
    const from = shown.value
    const started = performance.now()
    // rAF timestamps can trail performance.now() in throttled windows and iframes, so read one clock and clamp.
    const step = () => {
      const t = Math.min(1, Math.max(0, (performance.now() - started) / duration))
      shown.value = from + (target - from) * easeOut(t)
      if (t < 1) frame = requestAnimationFrame(step)
    }
    frame = requestAnimationFrame(step)
  })
  onUnmounted(() => cancelAnimationFrame(frame))
  return shown
}
