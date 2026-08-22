import assert from "node:assert/strict";
import test from "node:test";
import {
  formatListingPrice,
  listingSatisfiesBudget,
} from "../src/lib/listing-price.ts";
import { primaryProofFocus } from "../src/lib/proof-focus.ts";
import type { ProofFocus, SearchResultItem } from "../src/lib/types.ts";

function hospitalFocus(): ProofFocus {
  return {
    surfaceId: "around_this_home",
    layerId: "hospitals",
    factKey: "nearby_hospitals",
    entityId: "place:manipal",
    matchedLabel: "Manipal Hospital Whitefield",
    requestedConstraint: "near Manipal Hospital Whitefield",
    reason: "1.5 km from Manipal Hospital Whitefield",
  };
}

function metroFocus(): ProofFocus {
  return {
    surfaceId: "around_this_home",
    layerId: "metro",
    factKey: "nearby_metro",
    entityId: "place:whitefield-metro",
    requestedConstraint: "near metro",
    reason: "metro access",
  };
}

test("formatListingPrice shows a band when min and max differ", () => {
  assert.equal(
    formatListingPrice({
      price: 32_250_000,
      price_min: 30_000_000,
      price_max: 48_000_000,
    }),
    "₹3.0–4.8 Cr",
  );
  assert.equal(
    formatListingPrice({ price: 32_250_000 }),
    "₹3.2 Cr",
  );
  assert.equal(
    formatListingPrice({
      price: 32_250_000,
      price_min: 32_250_000,
      price_max: 32_250_000,
    }),
    "₹3.2 Cr",
  );
});

test("listingSatisfiesBudget uses overlap, not the collapsed midpoint", () => {
  const listing = {
    price: 32_250_000,
    price_min: 30_000_000,
    price_max: 48_000_000,
  };
  assert.equal(listingSatisfiesBudget(listing, null, 33_000_000), true);
  assert.equal(listingSatisfiesBudget(listing, 40_000_000, null), true);
  assert.equal(listingSatisfiesBudget(listing, null, 29_000_000), false);
  assert.equal(listingSatisfiesBudget({ price: 32_250_000 }, 40_000_000, null), false);
});

test("primaryProofFocus prefers the named place in the query, not array order", () => {
  const result: Pick<SearchResultItem, "proof_focuses" | "match_reason" | "match_explanation"> = {
    match_reason: "Near Aster Hospital Whitefield Bangalore, metro access",
    match_explanation: {
      reasons: [{
        preference: "near metro",
        fact_key: "nearby_metro",
        display: "metro access",
        score: 0.9,
        confidence: 0.8,
        source_type: "Google",
        scoring_method: "geo",
      }],
      preference_coverage: [],
      graph_driven_pct: 1,
      total_facts_consulted: 1,
    },
    proof_focuses: [hospitalFocus(), metroFocus()],
  };
  assert.equal(
    primaryProofFocus(result, "3BHK near metro in Whitefield")?.layerId,
    "metro",
  );
  assert.equal(
    primaryProofFocus(result, "3BHK near Manipal Hospital Whitefield")?.layerId,
    "hospitals",
  );
});

test("primaryProofFocus is empty when search had no proof overlay", () => {
  assert.equal(primaryProofFocus({
    match_reason: "Matches 3 BHK",
    proof_focuses: [],
  }), undefined);
});
