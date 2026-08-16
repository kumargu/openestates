export const RAIL_PAGE_COMPACT_MAX_WIDTH = 760;
export const LANDING_RAIL_MIN_CARD_PX = 188;
export const LANDING_RAIL_GAP_PX = 14.4;

/** How many full cards fit in a rail without a trailing peek. */
export function fittedRailPageSize(
  width: number,
  options: {
    compact?: boolean;
    minCardWidth?: number;
    gap?: number;
  } = {},
): number {
  if (options.compact) return 1;
  const minCardWidth = options.minCardWidth ?? LANDING_RAIL_MIN_CARD_PX;
  const gap = options.gap ?? LANDING_RAIL_GAP_PX;
  if (width <= 0) return 1;
  return Math.max(1, Math.floor((width + gap) / (minCardWidth + gap)));
}
