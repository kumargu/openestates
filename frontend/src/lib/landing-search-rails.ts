import type { SearchResponse, SearchResultItem } from "./types.ts";

/** Canonical backend traversal order carried into property detail; rendering caps do not apply. */
export function orderedLandingSearchResults(
  response: SearchResponse,
): SearchResultItem[] {
  const allResults = response.resultSets.flatMap((set) => set.results);
  const resultById = new Map(allResults.map((result) => [result.id, result] as const));
  return [...new Set(response.orderedResultIds)].flatMap((id) => {
    const result = resultById.get(id);
    return result ? [result] : [];
  });
}

export function partitionLandingResultSet(results: SearchResultItem[]) {
  return {
    exact: results.filter((result) => result.matchTier !== "supported"),
    siblings: results.filter((result) => result.matchTier === "supported"),
  };
}

/** Contextual homes carry the accepted journey but never acquire exact-match proof. */
export function journeyNavigationResults(response: SearchResponse): SearchResultItem[] {
  const exact = orderedLandingSearchResults(response);
  const seen = new Set(exact.map((card) => card.id));
  return [...exact, ...(response.journey?.active.collections ?? []).flatMap((collection) =>
    collection.cards.filter((card) => !seen.has(card.id) && seen.add(card.id)).map((card) => ({
      ...card, matchTier: "contextual" as const, reasons: [], collectionTitle: collection.title,
    })))];
}
