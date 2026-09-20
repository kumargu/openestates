import type { SearchResultItem } from "./types.ts";

/** The API owns which evidence is suitable for the card. Never infer it from copy. */
export function searchResultReasonLabels(result: Pick<SearchResultItem, "reasons">): string[] {
  return [...new Set(result.reasons.filter((reason) => reason.showOnCard).map((reason) => reason.explanation))].slice(0, 2);
}
