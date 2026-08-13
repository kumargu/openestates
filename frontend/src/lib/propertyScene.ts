import type { PropertyMedia } from "./types.ts";

/** Return only explicit image URLs already present on the property payload. */
export function trustedPropertyMedia(media?: PropertyMedia[]): PropertyMedia[] {
  const idHashes = new Map<string, string>();
  const hashIndexes = new Map<string, number>();
  const trusted: PropertyMedia[] = [];
  const candidates = [...(media ?? [])]
    .filter((asset) =>
      (asset.hero_eligible || asset.gallery_eligible) &&
      asset.media_kind !== "unknown" &&
      /^[a-f0-9]{64}$/i.test(asset.content_sha256 ?? "") &&
      asset.url.startsWith("/media/images/sha256/") &&
      asset.quality_flags.length === 0 &&
      asset.validation_state === "source_identity_matched",
    )
    .sort((left, right) => left.display_order - right.display_order || left.id.localeCompare(right.id));

  for (const asset of candidates) {
    const contentHash = asset.content_sha256;
    if (!contentHash) continue;
    const previousHash = idHashes.get(asset.id);
    if (previousHash && previousHash !== contentHash) {
      const previousIndex = hashIndexes.get(previousHash);
      const previous = previousIndex === undefined ? undefined : trusted[previousIndex];
      if (previous) {
        previous.hero_eligible = false;
        previous.gallery_eligible = false;
        previous.quality_flags = ["conflicting_media_identity"];
      }
      continue;
    }
    idHashes.set(asset.id, contentHash);
    const existingIndex = hashIndexes.get(contentHash);
    if (existingIndex !== undefined) {
      const existing = trusted[existingIndex];
      if (!existing) continue;
      const sceneConflict = Boolean(
        existing.scene_category &&
        asset.scene_category &&
        existing.scene_category !== asset.scene_category,
      );
      if (existing.media_kind !== asset.media_kind || sceneConflict) {
        existing.hero_eligible = false;
        existing.gallery_eligible = false;
        existing.quality_flags = ["conflicting_media_classification"];
        continue;
      }
      existing.hero_eligible ||= asset.hero_eligible;
      existing.gallery_eligible ||= asset.gallery_eligible;
      existing.scene_category ??= asset.scene_category;
      existing.source_entity_label ??= asset.source_entity_label;
      existing.identity_proof_method ??= asset.identity_proof_method;
      existing.media_classification_method ??= asset.media_classification_method;
      existing.fetched_at ??= asset.fetched_at;
      existing.alt_text ??= asset.alt_text;
      continue;
    }
    hashIndexes.set(contentHash, trusted.length);
    trusted.push({ ...asset, quality_flags: [...asset.quality_flags] });
  }

  return trusted
    .filter((asset) =>
      (asset.hero_eligible || asset.gallery_eligible) && asset.quality_flags.length === 0,
    )
    .sort((left, right) =>
      Number(right.hero_eligible) - Number(left.hero_eligible) ||
      left.display_order - right.display_order ||
      left.id.localeCompare(right.id),
    );
}

/** Pick a scene by stable list position, wrapping only when scenes are exhausted. */
export function propertySceneMediaAt(
  media: PropertyMedia[],
  index: number,
): PropertyMedia | null {
  if (media.length === 0) return null;
  const safeIndex = Math.max(0, Math.floor(index)) % media.length;
  return media[safeIndex] ?? null;
}

export function propertyMediaLabel(asset: PropertyMedia): string | null {
  const scene = asset.scene_category?.replaceAll("_", " ");
  const sceneLabel = scene ? scene.charAt(0).toUpperCase() + scene.slice(1) : null;
  if (asset.media_kind === "render") return sceneLabel ? `${sceneLabel} render` : "Render";
  return sceneLabel;
}

export function propertyMediaAlt(asset: PropertyMedia, title: string): string {
  return asset.alt_text?.trim() || [title, propertyMediaLabel(asset)].filter(Boolean).join(" — ");
}
