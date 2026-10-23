declare global {
  interface Window {
    __TAURI__?: {
      core: { invoke: (command: string, args?: Record<string, unknown>) => Promise<unknown> }
      event: { listen: (event: string, handler: (event: { payload: unknown }) => void) => Promise<() => void> }
    }
  }
}

export const query = new URLSearchParams(window.location.search)
export const demo = query.get('demo') === '1'
export const desktop = typeof window.__TAURI__?.core.invoke === 'function'
export const isWindows = navigator.userAgent.includes('Windows')

export function invoke(command: string, args?: Record<string, unknown>): Promise<unknown> {
  const tauri = window.__TAURI__
  if (!tauri || demo) return Promise.reject(new Error('Desktop bridge unavailable'))
  return tauri.core.invoke(command, args)
}

export async function listen(event: string, handler: (payload: unknown) => void): Promise<() => void> {
  const tauri = window.__TAURI__
  if (!tauri || demo) return () => {}
  return tauri.event.listen(event, message => handler(message.payload))
}
