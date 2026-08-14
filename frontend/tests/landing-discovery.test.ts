import assert from "node:assert/strict";
import test from "node:test";
import { fixtureProperties } from "../src/lib/dev-fixtures.ts";
import { landingDiscoveryHomes } from "../src/lib/landing-discovery.ts";
import { societyKey } from "../src/lib/property-filters.ts";
import type { SearchResponse, SearchResultItem } from "../src/lib/types.ts";

const proofFocus = {
  surfaceId: "around_this_home",
  layerId: "schools",
  factKey: "nearby_school",
  reason: "Matched your search",
} as const;

function proofResult(index: number): SearchResultItem {
  return {
    ...fixtureProperties[index],
    match_score: 92,
    match_label: "Strong match",
    match_reason: "Nearby school",
    buyer_proof: {
      receipt: {
        label: "Brooklyn NPS",
        distance_m: 600,
        match_status: "matched",
        source_type: "fixture",
        evidence_confidence: 0.82,
        focus: proofFocus,
      },
    },
  };
}

function searchWith(results: SearchResultItem[]): SearchResponse {
  return {
    query: "Quiet 3BHK near schools under 2.5Cr",
    intent: { area: null, bhk: 3, budget_max: 25_000_000, preferences: [] },
    results,
    area_context: null,
    total_results: results.length,
    knowledge_context: null,
  };
}

test("landing uses eligible search order and projects its typed receipt", () => {
  const result = proofResult(1);
  const collection = landingDiscoveryHomes(
    fixtureProperties,
    searchWith([result]),
    6,
  );
  const { homes } = collection;

  assert.equal(collection.source, "search");
  assert.equal(homes[0].property.id, result.id);
  assert.equal(homes.length, 1);
  assert.equal(homes[0].buyerProof?.receipt?.label, "Brooklyn NPS");
  assert.deepEqual(homes[0].proofFocus, proofFocus);
  assert.equal(
    new Set(homes.map(({ property }) => societyKey(property))).size,
    homes.length,
  );
});

test("landing excludes discovery-ineligible search results", () => {
  const ineligible = proofResult(0);
  ineligible.buyer_eligibility = {
    policy_version: 1,
    surfaces: { discovery: { eligible: false, reason_codes: ["missing_price"] } },
  };

  const collection = landingDiscoveryHomes(
    [ineligible, ...fixtureProperties.slice(1)],
    searchWith([ineligible]),
    4,
  );
  const { homes } = collection;

  assert.equal(collection.source, "catalog");
  assert.equal(homes.some(({ property }) => property.id === ineligible.id), false);
  assert.equal(homes.every(({ property }) => property.price > 0), true);
});

test("landing keeps a unique-society fallback when proof is unavailable", () => {
  const duplicate = {
    ...fixtureProperties[0],
    id: `${fixtureProperties[0].id}-2bhk`,
    bhk: 2,
    title: `2 BHK at ${fixtureProperties[0].society_name}`,
  };
  const collection = landingDiscoveryHomes(
    [duplicate, ...fixtureProperties],
    null,
    20,
  );
  const { homes } = collection;

  assert.equal(collection.source, "catalog");
  assert.equal(
    new Set(homes.map(({ property }) => societyKey(property))).size,
    homes.length,
  );
  assert.equal(homes.every(({ buyerProof }) => buyerProof === undefined), true);
});

test("landing never promotes a legacy proof focus without a concrete receipt", () => {
  const result = proofResult(0);
  result.buyer_proof = undefined;
  result.proof_focuses = [{
    surfaceId: "around_this_home",
    layerId: "schools",
    factKey: "legacy_school_match",
    reason: "Legacy result focus",
  }];

  const collection = landingDiscoveryHomes(
    fixtureProperties,
    searchWith([result]),
    1,
  );

  assert.equal(collection.source, "search");
  assert.equal(collection.homes[0].buyerProof, undefined);
  assert.equal(collection.homes[0].proofFocus, undefined);
});
