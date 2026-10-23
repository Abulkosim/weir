import { z } from 'zod'

const ms = z.number().finite().nonnegative()
const count = z.number().int().nonnegative()

export const testErrorSchema = z.enum(['offline', 'refused', 'serverError', 'stalled', 'tooShort', 'dropped', 'networkChanged', 'cancelled'])
export const phaseSchema = z.enum(['connecting', 'latency', 'download', 'upload'])

export const progressSchema = z.object({
  phase: phaseSchema,
  phaseMs: count,
  liveMbps: ms.nullable(),
  downloadMbps: ms.nullable(),
  latencyMs: ms.nullable(),
  loadedMs: ms.nullable(),
  bytesUsed: count,
  colo: z.string().regex(/^[A-Z]{3}$/).nullable(),
})

export const speedSchema = z.object({
  mbps: ms,
  p10: ms,
  p90: ms,
  varied: z.boolean(),
  capped: z.boolean(),
  rampedSlowly: z.boolean(),
  rough: z.boolean(),
  loadedMs: ms.nullable(),
})

export const reportSchema = z.object({
  startedAtMs: count,
  durationMs: count,
  provider: z.string().max(40),
  colo: z.string().regex(/^[A-Z]{3}$/).nullable(),
  latency: z.object({ medianMs: ms, jitterMs: ms.nullable(), answered: count, sent: count }).nullable(),
  download: speedSchema,
  upload: speedSchema.nullable(),
  uploadError: testErrorSchema.nullable(),
  bytesUsed: count,
})

export const meteredSchema = z.enum(['hotspot', 'lowData', 'metered', 'roaming', 'overLimit'])
export const startSchema = z.enum(['started', 'running', 'needsConfirmation', 'offline'])

export const healthSchema = z.enum(['good', 'fair', 'poor'])
export const meaningSchema = z.object({
  health: healthSchema,
  headline: z.string().max(120),
  lines: z.array(z.object({ topic: z.string().max(20), text: z.string().max(120), health: healthSchema })).max(6),
})

export const settingsSchema = z.object({
  theme: z.enum(['system', 'light', 'dark']),
  traySpeed: z.boolean(),
  autostart: z.boolean(),
})
export const defaultSettings: z.infer<typeof settingsSchema> = { theme: 'system', traySpeed: false, autostart: false }

export const viewSchema = z.object({
  revision: count,
  testing: progressSchema.nullable(),
  last: reportSchema.nullable(),
  failure: z.object({ error: testErrorSchema, atMs: count }).nullable(),
  estimate: z.object({ upTo: z.boolean(), mb: count }),
  network: z.object({ online: z.boolean(), metered: meteredSchema.nullable(), offlineSinceMs: count.nullable() }),
  meaning: meaningSchema.nullable(),
  historyCount: count,
  notice: z.string().max(300).nullable(),
  settings: settingsSchema,
})

export const entrySchema = z.discriminatedUnion('kind', [
  z.object({ kind: z.literal('result'), report: reportSchema }),
  z.object({ kind: z.literal('failed'), atMs: count, error: testErrorSchema }),
])
export const historySchema = z.array(entrySchema).max(1_000)

export type TestError = z.infer<typeof testErrorSchema>
export type Phase = z.infer<typeof phaseSchema>
export type Progress = z.infer<typeof progressSchema>
export type Speed = z.infer<typeof speedSchema>
export type Report = z.infer<typeof reportSchema>
export type View = z.infer<typeof viewSchema>
export type Metered = z.infer<typeof meteredSchema>
export type Meaning = z.infer<typeof meaningSchema>
export type Entry = z.infer<typeof entrySchema>
export type Settings = z.infer<typeof settingsSchema>

export const emptyView: View = { revision: 0, testing: null, last: null, failure: null, estimate: { upTo: true, mb: 600 }, network: { online: true, metered: null, offlineSinceMs: null }, meaning: null, historyCount: 0, notice: null, settings: { ...defaultSettings } }

/** Under-load latency: the worse of the download and upload medians. */
export function loadedMs(report: Report): number | null {
  const values = [report.download.loadedMs, report.upload?.loadedMs ?? null].filter((value): value is number => value !== null)
  return values.length ? Math.max(...values) : null
}

export function entryTime(entry: Entry): number {
  return entry.kind === 'result' ? entry.report.startedAtMs : entry.atMs
}
