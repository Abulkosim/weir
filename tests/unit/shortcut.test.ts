import assert from 'node:assert/strict'
import { test } from 'node:test'
import { formatShortcut, recordShortcut } from '../../src/lib/shortcut.ts'

const key = (code: string, mods: Partial<{ ctrlKey: boolean; altKey: boolean; shiftKey: boolean; metaKey: boolean }> = {}) => ({ code, ctrlKey: false, altKey: false, shiftKey: false, metaKey: false, ...mods })

test('records modifiers in a fixed order with the physical key', () => {
  assert.deepEqual(recordShortcut(key('KeyW', { metaKey: true, altKey: true })), { kind: 'shortcut', value: 'Alt+Super+KeyW' })
  assert.deepEqual(recordShortcut(key('Digit1', { ctrlKey: true, shiftKey: true })), { kind: 'shortcut', value: 'Control+Shift+Digit1' })
  assert.deepEqual(recordShortcut(key('Space', { ctrlKey: true, altKey: true })), { kind: 'shortcut', value: 'Control+Alt+Space' })
})

test('waits on modifiers and rejects unusable combinations', () => {
  assert.deepEqual(recordShortcut(key('MetaLeft', { metaKey: true })), { kind: 'pending' })
  assert.deepEqual(recordShortcut(key('KeyA', { shiftKey: true }), false), { kind: 'error', message: 'Include Command, Control or Option.' })
  assert.deepEqual(recordShortcut(key('KeyA'), true), { kind: 'error', message: 'Include Ctrl, Alt or Win.' })
  assert.equal(recordShortcut(key('IntlRo', { metaKey: true })).kind, 'error')
})

test('formats with Mac symbols or Windows names', () => {
  assert.equal(formatShortcut('Control+Alt+KeyW', false), '⌃⌥W')
  assert.equal(formatShortcut('Alt+Super+Space', false), '⌥⌘Space')
  assert.equal(formatShortcut('Control+Alt+KeyW', true), 'Ctrl+Alt+W')
  assert.equal(formatShortcut('Control+Shift+Digit1', true), 'Ctrl+Shift+1')
})
