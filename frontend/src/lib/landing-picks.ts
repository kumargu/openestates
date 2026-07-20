import { filterListableProperties } from "./property-filters.ts";
import type { AreaTrackerResponse, PropertyCard } from "./types.ts";

export type LandingPick = {
  area: string;
  property: PropertyCard;
};

function hasGoogleRating(
  property: PropertyCard,
): property is PropertyCard & { google_rating: number } {
  return typeof property.google_rating === "number" && property.google_rating > 0;
}

function compareGoogleRank(a: PropertyCard, b: PropertyCard): number {
  const ratingDelta = (b.google_rating ?? 0) - (a.google_rating ?? 0);
  if (ratingDelta !== 0) return ratingDelta;

  const reviewDelta = (b.google_review_count ?? 0) - (a.google_review_count ?? 0);
  if (reviewDelta !== 0) return reviewDelta;

  return a.price - b.price;
}

export function areaNamesForLandingPicks(
  areaTracker: AreaTrackerResponse | null,
  properties: PropertyCard[],
): string[] {
  if (areaTracker?.markets?.length) {
    return areaTracker.markets
      .filter((market) => market.listing_count >= 2)
      .sort((a, b) => b.listing_count - a.listing_count)
      .map((market) => market.name);
  }

  const byArea: Record<string, number> = {};
  for (const property of filterListableProperties(properties)) {
    byArea[property.area] = (byArea[property.area] ?? 0) + 1;
  }

  return Object.entries(byArea)
    .filter(([, count]) => count >= 2)
    .sort((a, b) => b[1] - a[1])
    .map(([area]) => area);
}

/** One listable home per area — highest Google rating, then review count. */
export function topGoogleRatedPerArea(
  properties: PropertyCard[],
  areaNames: string[],
): LandingPick[] {
  const listable = filterListableProperties(properties);
  const picks: LandingPick[] = [];

  for (const area of areaNames) {
    const ranked = listable
      .filter((property) => property.area === area)
      .filter(hasGoogleRating)
      .sort(compareGoogleRank);

    const best = ranked[0];
    if (best) {
      picks.push({ area, property: best });
    }
  }

  return picks;
}
