import { z } from 'zod'

export const POPOVER_HEIGHT = 600
export const ARROW_MARGIN = 26

export const popoverPlacementSchema = z.object({
  revision: z.number().int().nonnegative(),
  arrowX: z.number().finite().nullable(),
})
export type PopoverPlacement = z.infer<typeof popoverPlacementSchema>

export function nextPlacement(current: PopoverPlacement, payload: unknown): PopoverPlacement {
  const result = popoverPlacementSchema.safeParse(payload)
  if (!result.success) return { ...current, arrowX: null }
  return result.data.revision >= current.revision ? result.data : current
}

export function arrowOffset(arrowX: number | null, viewportWidth: number): number | null {
  if (arrowX === null) return null
  return arrowX >= ARROW_MARGIN && arrowX <= viewportWidth - ARROW_MARGIN ? arrowX : null
}

export function fitHeight(paddingTop: number, paddingBottom: number, content: number): number {
  return Math.min(Math.ceil(paddingTop + paddingBottom + content), POPOVER_HEIGHT)
}
