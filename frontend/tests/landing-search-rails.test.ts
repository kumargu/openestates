import assert from "node:assert/strict";
import test from "node:test";

import {
  composeLandingSearchRails,
  landingSearchRailHomeCount,
  orderedLandingSearchResults,
} from "../src/lib/landing-search-rails.ts";
import type { SearchResponse, SearchResultItem } from "../src/lib/types.ts";

const runtimeVersion = {
  servingBundleVersion: "test-bundle",
  scoringPolicyVersion: 1,
  searchEngineVersion: "test-search",
};

function result(id: string, tier: SearchResultItem["match_tier"] = "exact"): SearchResultItem {
  return {
    id,
    title: id,
    area: "Whitefield",
    city: "Bengaluru",
    society_name: id,
    builder_name: "Builder",
    price: 10_000_000,
    price_per_sqft: 10_000,
    bhk: 3,
    sqft: 1_200,
    carpet_area_sqft: 1_200,
    super_builtup_sqft: 1_500,
    possession_status: "Ready",
    metro_distance_mins: 10,
    floor: 1,
    total_floors: 10,
    facing: "East",
    images: [],
    hero_image: "",
    transparency_tags: [],
    description_summary: "",
    kg_entity_refs: {
      property_entity_id: `property:${id}`,
      society_entity_id: `society:${id}`,
      area_entity_id: "area:whitefield",
      source_entity_ids: [],
    },
    match_score: 1,
    match_label: "Strong match",
    match_reason: "Matched",
    match_tier: tier,
  };
}

test("renders backend branches and order without regrouping", () => {
  const response: SearchResponse = {
    query: "2BHK or 3BHK in Whitefield",
    resultSets: [
      { branchId: "branch-1", label: "2 BHK", results: [result("a"), result("b")] },
      { branchId: "branch-2", label: "3 BHK", results: [result("c"), result("d")] },
    ],
    orderedResultIds: ["a", "c", "b", "d"],
    totalMatches: 4,
    runtimeVersion,
    state: "results",
  };

  const rails = composeLandingSearchRails(response);
  assert.deepEqual(rails.map((rail) => rail.results.map((item) => item.id)), [["a", "b"], ["c", "d"]]);
  assert.deepEqual(
    orderedLandingSearchResults(response).map((item) => item.id),
    ["a", "c", "b", "d"],
  );
  assert.equal(landingSearchRailHomeCount(rails), 4);
});

test("falls back to deduplicated branch order during a rolling backend deploy", () => {
  const response = {
    query: "3BHK in Whitefield",
    resultSets: [
      { branchId: "one", label: "Matches", results: [result("a"), result("b")] },
      { branchId: "two", label: "More", results: [result("b"), result("c")] },
    ],
    totalMatches: 3,
    state: "results",
  } as SearchResponse;

  assert.deepEqual(
    orderedLandingSearchResults(response).map((item) => item.id),
    ["a", "b", "c"],
  );
});

test("keeps same-project sibling configurations as a quiet plus group", () => {
  const response: SearchResponse = {
    query: "3BHK in Waterford",
    resultSets: [{
      branchId: "branch-1",
      label: "Prestige Waterford",
      results: [result("asked"), result("sibling", "supported")],
    }],
    orderedResultIds: ["asked", "sibling"],
    totalMatches: 2,
    runtimeVersion,
    state: "results",
  };

  const [rail] = composeLandingSearchRails(response);
  assert.deepEqual(rail?.results.map((item) => item.id), ["asked"]);
  assert.deepEqual(rail?.siblings?.map((item) => item.id), ["sibling"]);
});

test("keeps every backend result available for landing pagination", () => {
  const backendResults = Array.from(
    { length: 29 },
    (_, index) => result(`home-${index}`),
  );
  const response: SearchResponse = {
    query: "3BHK in Whitefield",
    resultSets: [{ branchId: "branch-1", label: "Matches", results: backendResults }],
    orderedResultIds: backendResults.map((result) => result.id),
    totalMatches: backendResults.length,
    runtimeVersion,
    state: "results",
  };

  const rails = composeLandingSearchRails(response);
  assert.deepEqual(
    rails[0]?.results.map((item) => item.id),
    backendResults.map((item) => item.id),
  );
  assert.deepEqual(orderedLandingSearchResults(response), backendResults);
  assert.equal(landingSearchRailHomeCount(rails), 29);
});
