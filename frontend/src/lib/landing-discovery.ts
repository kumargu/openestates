import {
  filterListableProperties,
  uniqueSocietiesForDiscovery,
} from "./property-filters.ts";
import type {
  BuyerProofProjection,
  PropertyCard,
  ProofFocus,
  SearchResponse,
} from "./types.ts";

export const LANDING_PROOF_QUERY = "Quiet 3BHK near schools under 2.5Cr";
export const LANDING_PROOF_QUERY_LABEL = "Quiet 3BHK near schools under ₹2.5Cr";

export type LandingDiscoveryHome = {
  property: PropertyCard;
  buyerProof?: BuyerProofProjection;
  proofFocus?: ProofFocus;
};

export type LandingDiscoveryCollection = {
  source: "search" | "catalog";
  homes: LandingDiscoveryHome[];
};

/**
 * Search owns the ranked, concrete-proof examples. The landing only intersects
 * those results with discovery eligibility and dedupes societies. The catalog
 * is only a calm fallback when search has no eligible results.
 */
export function landingDiscoveryHomes(
  properties: PropertyCard[],
  proofSearch: SearchResponse | null,
  limit: number,
): LandingDiscoveryCollection {
  const eligibleProperties = filterListableProperties(properties);
  const eligibleHomes = uniqueSocietiesForDiscovery(eligibleProperties);
  const eligibleIds = new Set(eligibleProperties.map((property) => property.id));
  const searchHomes = uniqueSocietiesForDiscovery(
    (proofSearch?.results ?? []).filter((result) =>
      eligibleIds.has(result.id)
      && result.buyer_eligibility?.surfaces.discovery?.eligible === true
    ),
  );
  const selected: LandingDiscoveryHome[] = searchHomes.map((property) => ({
    property,
    buyerProof: property.buyer_proof,
    proofFocus: property.buyer_proof?.receipt?.focus,
  }));
  if (selected.length > 0) {
    return {
      source: "search",
      homes: selected.slice(0, Math.max(0, limit)),
    };
  }
  return {
    source: "catalog",
    homes: eligibleHomes
      .map((property) => ({ property }))
      .slice(0, Math.max(0, limit)),
  };
}
