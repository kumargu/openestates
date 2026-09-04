import type { SearchResponse, SearchResultItem } from "./types.ts";

export type LandingSearchRail = {
  id: string;
  label?: string;
  results: SearchResultItem[];
  siblings?: SearchResultItem[];
};

/** Canonical backend traversal order carried into property detail; rendering caps do not apply. */
export function orderedLandingSearchResults(
  response: SearchResponse,
): SearchResultItem[] {
  const resultSets = Array.isArray(response.resultSets) ? response.resultSets : [];
  const allResults = resultSets.flatMap((set) => set.results);
  const resultById = new Map(allResults.map((result) => [result.id, result] as const));
  const orderedIds = Array.isArray(response.orderedResultIds)
    ? response.orderedResultIds
    : allResults.map((result) => result.id);
  return [...new Set(orderedIds)].flatMap((id) => {
    const result = resultById.get(id);
    return result ? [result] : [];
  });
}

export function composeLandingSearchRails(response: SearchResponse): LandingSearchRail[] {
  return response.resultSets.map((set) => {
    const results = set.results.filter((result) => result.match_tier !== "supported");
    const siblings = set.results.filter((result) => result.match_tier === "supported");
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
