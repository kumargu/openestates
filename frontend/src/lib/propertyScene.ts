/** Resolve society slug from various ID formats used across the app. */
export function societySlugFromId(societyId: string | undefined): string | null {
  if (!societyId) return null;
  const trimmed = societyId.trim();
  if (!trimmed) return null;
  if (trimmed.startsWith("soc-")) return trimmed.slice(4);
  if (trimmed.startsWith("society:")) return trimmed.slice("society:".length);
  return trimmed;
}

/** Candidate paths produced by pipeline/skills/fetch_images. */
export function societyPhotoCandidates(slug: string, max = 5): string[] {
  const exts = ["jpg", "jpeg", "png", "webp"];
  const paths: string[] = [];
  for (let i = 1; i <= max; i += 1) {
    for (const ext of exts) {
      paths.push(`/societies/${slug}/${i}.${ext}`);
    }
  }
  return paths;
}

function unique(values: string[]): string[] {
  return [...new Set(values.filter(Boolean))];
}

/** Return only explicit image URLs already present on the property payload. */
export function initialPropertySceneUrls(input: {
  heroImage?: string | null;
  images?: string[];
  societyId?: string;
}): string[] {
  const slug = societySlugFromId(input.societyId);
  const localSocietyPhotos = slug ? societyPhotoCandidates(slug, 5) : [];
  return unique([
    ...localSocietyPhotos,
    ...(input.images ?? []),
    ...(input.heroImage ? [input.heroImage] : []),
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

/** Probe which image URLs actually load in the browser. */
export function probeImageUrls(urls: string[]): Promise<string[]> {
  if (urls.length === 0) return Promise.resolve([]);

  return Promise.all(
    urls.map(
      (url) =>
        new Promise<string | null>((resolve) => {
          const img = new Image();
          img.onload = () => resolve(url);
          img.onerror = () => resolve(null);
          img.src = url;
        }),
    ),
  ).then((loaded) => loaded.filter((url): url is string => Boolean(url)));
}

const SCENE_LABELS = ["Exterior", "Building", "Amenities", "Neighbourhood", "Gallery"];

export function sceneLabelForIndex(index: number): string {
  return SCENE_LABELS[index % SCENE_LABELS.length];
}
