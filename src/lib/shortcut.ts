const modifierOrder = ['Control', 'Alt', 'Shift', 'Super'] as const
type Modifier = (typeof modifierOrder)[number]
const macSymbols: Record<Modifier, string> = { Control: '⌃', Alt: '⌥', Shift: '⇧', Super: '⌘' }
const windowsNames: Record<Modifier, string> = { Control: 'Ctrl', Alt: 'Alt', Shift: 'Shift', Super: 'Win' }
const keyNames: Record<string, string> = {
  Space: 'Space', Enter: '↩', Tab: '⇥', Backspace: '⌫', Delete: '⌦',
  ArrowUp: '↑', ArrowDown: '↓', ArrowLeft: '←', ArrowRight: '→',
  Minus: '-', Equal: '=', BracketLeft: '[', BracketRight: ']', Backslash: '\\', Semicolon: ';', Quote: "'", Comma: ',', Period: '.', Slash: '/', Backquote: '`',
}
const modifierCodes = new Set(['ShiftLeft', 'ShiftRight', 'ControlLeft', 'ControlRight', 'AltLeft', 'AltRight', 'MetaLeft', 'MetaRight', 'OSLeft', 'OSRight', 'CapsLock', 'Fn'])

export type ShortcutEvent = Pick<KeyboardEvent, 'code' | 'ctrlKey' | 'altKey' | 'shiftKey' | 'metaKey'>
export type Recorded = { kind: 'shortcut'; value: string } | { kind: 'pending' } | { kind: 'error'; message: string }

const onWindows = () => typeof navigator !== 'undefined' && navigator.userAgent.includes('Windows')

/** A key press while recording, as the Rust side parses it, for example `Control+Alt+KeyW`. */
export function recordShortcut(event: ShortcutEvent, windows = onWindows()): Recorded {
  if (modifierCodes.has(event.code) || !event.code) return { kind: 'pending' }
  const key = /^(Key[A-Z]|Digit\d|F\d{1,2})$/.test(event.code) || event.code in keyNames ? event.code : null
  if (!key) return { kind: 'error', message: 'That key can’t be used in a shortcut.' }
  if (!event.ctrlKey && !event.altKey && !event.metaKey) return { kind: 'error', message: windows ? 'Include Ctrl, Alt or Win.' : 'Include Command, Control or Option.' }
  const pressed: Record<Modifier, boolean> = { Control: event.ctrlKey, Alt: event.altKey, Shift: event.shiftKey, Super: event.metaKey }
  return { kind: 'shortcut', value: [...modifierOrder.filter(name => pressed[name]), key].join('+') }
}

export function formatShortcut(value: string, windows = onWindows()): string {
  const parts = value.split('+')
  const key = parts.at(-1) ?? ''
  const label = key.startsWith('Key') ? key.slice(3) : key.startsWith('Digit') ? key.slice(5) : keyNames[key] ?? key
  const modifiers = modifierOrder.filter(name => parts.includes(name))
  return windows ? [...modifiers.map(name => windowsNames[name]), label].join('+') : `${modifiers.map(name => macSymbols[name]).join('')}${label}`
}
