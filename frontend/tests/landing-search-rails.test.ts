import assert from "node:assert/strict";
import test from "node:test";

import {
  orderedLandingSearchResults,
  journeyNavigationResults,
  partitionLandingResultSet,
} from "../src/lib/landing-search-rails.ts";
import type { SearchResponse, SearchResultItem } from "../src/lib/types.ts";
import { projectSearchJourney } from "../src/lib/search-journey.ts";
import { journeyFixture } from "./fixtures/search-journey.ts";

const runtimeVersion = {
  servingBundleVersion: "test-bundle",
  scoringPolicyVersion: 1,
  searchEngineVersion: "test-search",
};

function result(id: string, tier: SearchResultItem["matchTier"] = "exact"): SearchResultItem {
  return {
    id, title: id, society_id: `society:${id}`, society_name: id, area: "Whitefield",
    image: null, bhk: 3, price: 10_000_000, sqft: 1_200,
    detail_href: `/property/${id}`, save_id: id, matchTier: tier, reasons: [],
  };
}

test("renders backend order without regrouping", () => {
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

  assert.deepEqual(
    orderedLandingSearchResults(response).map((item) => item.id),
    ["a", "c", "b", "d"],
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

  const partition = partitionLandingResultSet(response.resultSets[0].results);
  assert.deepEqual(partition.exact.map((item) => item.id), ["asked"]);
  assert.deepEqual(partition.siblings.map((item) => item.id), ["sibling"]);
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

  assert.deepEqual(
    partitionLandingResultSet(response.resultSets[0].results).exact.map((item) => item.id),
    backendResults.map((item) => item.id),
  );
  assert.deepEqual(orderedLandingSearchResults(response), backendResults);
});

test("catalog additions remain in backend order without client-side rebucketing", () => {
  const envelope = journeyFixture();
  if (envelope.active.results.kind !== "current") throw new Error("fixture must be current");
  const first = envelope.active.results.resultSets[0].results[0];
  const added = structuredClone(first);
  added.id = "new-home";
  envelope.active.results.resultSets[0].results.push(added);
  envelope.active.results.resultSets.push({ branchId: "branch-2", label: "Alternative", results: [{ ...added, id: "compare-home" }] });
  envelope.active.results.orderedResultIds = [first.id, added.id, "compare-home"];
  envelope.attempt.catalogRebased = true;
  envelope.attempt.catalogDelta = { added: [added.id], removed: [], retained: [first.id, "compare-home"], moved: [] };
  assert.deepEqual(
    orderedLandingSearchResults(projectSearchJourney(envelope)).map((item) => item.id),
    [first.id, added.id, "compare-home"],
  );
});


test("zero exact results retain contextual navigation without exact-match proof", () => {
  const envelope = journeyFixture();
  if (envelope.active.results.kind !== "current") throw new Error("fixture must be current");
  envelope.active.results.resultSets = [];
  envelope.active.results.orderedResultIds = [];
  const response = projectSearchJourney(envelope);
  assert.deepEqual(orderedLandingSearchResults(response), []);
  const navigation = journeyNavigationResults(response);
  assert.ok(navigation.length > 0);
  assert.deepEqual(navigation.map((card) => card.id), envelope.active.collections.flatMap((rail) => rail.cards.map((card) => card.id)));
  for (const card of navigation) {
    assert.equal(card.matchTier, "contextual");
    assert.deepEqual(card.reasons, []);
    assert.equal(card.collectionTitle, envelope.active.collections[0].title);
  }
});
