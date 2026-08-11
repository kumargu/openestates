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

const SCENE_LABELS = ["Exterior", "Building", "Amenities", "Neighbourhood", "Gallery"];

export function sceneLabelForIndex(index: number): string {
  return SCENE_LABELS[index % SCENE_LABELS.length];
}
