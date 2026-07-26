import type { PropertyCard, SearchResponse } from "./types.ts";

export function isListableProperty(property: Pick<PropertyCard, "price">): boolean {
  return property.price > 0;
}

export function filterListableProperties<T extends Pick<PropertyCard, "price">>(properties: T[]): T[] {
  return properties.filter(isListableProperty);
}

export function filterListableSearchResponse(response: SearchResponse): SearchResponse {
  const results = filterListableProperties(response.results);
  return {
    ...response,
    results,
    total_results: results.length,
  };
}
