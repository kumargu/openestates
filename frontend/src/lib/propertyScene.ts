function unique(values: string[]): string[] {
  return [...new Set(values.filter(Boolean))];
}

/** Return only explicit image URLs already present on the property payload. */
export function initialPropertySceneUrls(input: {
  heroImage?: string | null;
  images?: string[];
}): string[] {
  return unique([
    ...(input.heroImage ? [input.heroImage] : []),
    ...(input.images ?? []),
  ]);
}

/** Wrap a photo index through a gallery of `total` frames. */
export function wrapPhotoIndex(index: number, total: number): number {
  if (total <= 0) return 0;
  const floor = Math.floor(index);
  return ((floor % total) + total) % total;
}

/** Pick a scene by stable list position, wrapping only when scenes are exhausted. */
export function propertySceneImageAt(
  images: string[],
  index: number,
  fallback?: string | null,
): string | null {
  if (images.length === 0) return fallback ?? null;
  const safeIndex = Math.max(0, Math.floor(index)) % images.length;
  return images[safeIndex] ?? fallback ?? null;
}

/** Mosaic lead and Show all open photo 0. A side tile at slice position n opens n + 1. */
export function photoIndexFromMosaicSlot(
  slot: "lead" | "all" | { tile: number },
): number {
  if (slot === "lead" || slot === "all") return 0;
  return Math.max(0, Math.floor(slot.tile) + 1);
}

const SCENE_LABELS = ["Exterior", "Building", "Amenities", "Neighbourhood", "Gallery"];

export function sceneLabelForIndex(index: number): string {
  return SCENE_LABELS[index % SCENE_LABELS.length];
}
