import type { SearchResponse, SearchResultItem } from "./types.ts";

/** Rendering cap only. Membership and order always come from the backend. */
export const LANDING_SEARCH_RAIL_CAP = 8;

export type LandingSearchRail = {
  id: string;
  label?: string;
  results: SearchResultItem[];
  siblings?: SearchResultItem[];
};

export function composeLandingSearchRails(response: SearchResponse): LandingSearchRail[] {
  return response.resultSets.map((set) => {
    const visible = set.results.slice(0, LANDING_SEARCH_RAIL_CAP);
    const results = visible.filter((result) => result.match_tier !== "supported");
    const siblings = visible.filter((result) => result.match_tier === "supported");
    return {
      id: set.branchId,
      label: set.label === "Matches" ? undefined : set.label,
      results,
      siblings: siblings.length > 0 ? siblings : undefined,
    };
  });
}

export function landingSearchRailHomeCount(rails: LandingSearchRail[]): number {
  return rails.reduce(
    (count, rail) => count + rail.results.length + (rail.siblings?.length ?? 0),
    0,
  );
}

export function landingSearchRailTooLong(rails: LandingSearchRail[]): boolean {
  return rails.some(
    (rail) => rail.results.length + (rail.siblings?.length ?? 0) > LANDING_SEARCH_RAIL_CAP,
  );
}
