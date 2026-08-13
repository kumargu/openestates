import type { PropertyCard, SearchResponse } from "./types.ts";

type ListabilityFields = Pick<PropertyCard, "price"> &
  Partial<Pick<PropertyCard, "bhk" | "transparency_tags">>;

export function isListableProperty(property: ListabilityFields): boolean {
  return (
    property.price > 0 ||
    Boolean(
      property.bhk &&
      property.bhk > 0 &&
      property.transparency_tags?.some(
        (tag) => tag.toLowerCase() === "price unavailable",
      ),
    )
  );
}

export function filterListableProperties<T extends ListabilityFields>(
  properties: T[],
): T[] {
  return properties.filter(isListableProperty);
}

export function filterListableSearchResponse(
  response: SearchResponse,
): SearchResponse {
  const results = filterListableProperties(response.results);
  const focus = response.focus
    ? {
        ...response.focus,
        focus_results: filterListableProperties(response.focus.focus_results),
        sibling_configs: filterListableProperties(response.focus.sibling_configs ?? []),
        more_homes: filterListableProperties(response.focus.more_homes ?? []),
      }
    : response.focus;
  return {
    ...response,
    results,
    focus,
    total_results: results.length,
  };
}

/** Stable society key for browse/discovery dedupe. */
export function societyKey(
  property: Pick<PropertyCard, "id" | "society_name" | "kg_entity_refs">,
): string {
  const fromRefs = property.kg_entity_refs?.society_entity_id?.trim();
  if (fromRefs) return fromRefs.toLowerCase();
  const fromName = property.society_name?.trim();
  if (fromName) return fromName.toLowerCase();
  return property.id;
}

function discoverRepresentativeScore(property: PropertyCard): number {
  let score = 0;
  if (property.hero_media) score += 40;
  if (typeof property.google_rating === "number" && property.google_rating > 0) {
    score += property.google_rating * 10;
  }
  if (typeof property.google_review_count === "number" && property.google_review_count > 0) {
    score += Math.min(property.google_review_count, 80) / 10;
  }
  // Prefer a typical family config as the society face on Discover — not every BHK.
  score += Math.max(0, 12 - Math.abs((property.bhk || 0) - 3) * 4);
  return score;
}

/**
 * Discover / landing rails: one card per society.
 * Search results keep all BHK configs when the ask needs them.
 */
export function uniqueSocietiesForDiscovery(properties: PropertyCard[]): PropertyCard[] {
  const bestBySociety = new Map<string, PropertyCard>();
  for (const property of filterListableProperties(properties)) {
    const key = societyKey(property);
    const existing = bestBySociety.get(key);
    if (!existing || discoverRepresentativeScore(property) > discoverRepresentativeScore(existing)) {
      bestBySociety.set(key, property);
    }
  }
  return [...bestBySociety.values()];
}
