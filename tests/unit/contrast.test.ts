import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'

const css = readFileSync(new URL('../../src/styles.css', import.meta.url), 'utf8')

function tokens(selector: string): Record<string, string> {
  const start = css.indexOf(`${selector} {`)
  assert.ok(start >= 0, `${selector} block missing`)
  const body = css.slice(start, css.indexOf('}', start))
  return Object.fromEntries([...body.matchAll(/--([\w-]+):\s*(#[0-9a-fA-F]{3,6});/g)].map(match => [match[1], match[2]]))
}

function rgb(hex: string): number[] {
  const value = hex.slice(1)
  const full = value.length === 3 ? [...value].map(c => c + c).join('') : value
  return [0, 2, 4].map(i => parseInt(full.slice(i, i + 2), 16))
}

function luminance(hex: string): number {
  const [r, g, b] = rgb(hex).map(channel => {
    const value = channel / 255
    return value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4
  })
  return 0.2126 * r! + 0.7152 * g! + 0.0722 * b!
}

function contrast(a: string, b: string): number {
  const [high, low] = [luminance(a), luminance(b)].sort((x, y) => y - x)
  return (high! + 0.05) / (low! + 0.05)
}

const light = tokens(':root')
const themes = { light, dark: { ...light, ...tokens(':root[data-theme=dark]') } }

for (const [name, theme] of Object.entries(themes)) {
  test(`${name} text tokens meet WCAG AA on surface and card`, () => {
    for (const background of ['surface', 'card']) {
      for (const text of ['ink', 'secondary', 'tertiary', 'accent-ink']) {
        const ratio = contrast(theme[text]!, theme[background]!)
        assert.ok(ratio >= 4.5, `${text} on ${background}: ${ratio.toFixed(2)}`)
      }
    }
    assert.ok(contrast(theme['accent-ink']!, theme['accent-soft']!) >= 4.5, 'accent-ink on accent-soft')
    assert.ok(contrast(theme['button-ink']!, theme.button!) >= 4.5, 'button-ink on button')
    assert.ok(contrast(theme.warning!, theme['warning-soft']!) >= 4.5, 'warning on warning-soft')
    assert.ok(contrast(theme.danger!, theme['danger-soft']!) >= 4.5, 'danger on danger-soft')
  })

  test(`${name} accent and focus ring stand out from the card`, () => {
    assert.ok(contrast(theme.accent!, theme.card!) >= 3, 'accent on card')
    assert.ok(contrast(theme.focus!, theme.surface!) >= 3, 'focus on surface')
  })
}
