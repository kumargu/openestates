import { getFixtureResponse } from "../../src/lib/dev-fixtures.ts";
import type { SearchJourneyEnvelope } from "../../src/lib/types.ts";

/** Mock transport data only: backend semantics are tested against serving fixtures in Rust. */
export function journeyFixture(id = "revision-1", brief = "3 BHK, under ₹2.4Cr"): SearchJourneyEnvelope {
  const envelope = structuredClone(getFixtureResponse("/api/search?q=3BHK")) as SearchJourneyEnvelope;
  envelope.active.buyerBrief = brief;
  envelope.active.revision = {
    id,
    parentId: id === "revision-1" ? undefined : "revision-1",
    stateToken: `signed:${id}`,
    resultFingerprint: "same-homes",
    depth: id === "revision-1" ? 0 : 1,
  };
  envelope.active.intent.branches = [{
    id: "branch-1", preferences: [], constraints: { kind: "predicate", predicate: {
      id: "budget", dimension: "price", label: "Price", polarity: "positive", operator: "atMost", value: { max: 24000000 }, unit: "INR", required: true,
    } },
  }];
  if (envelope.active.results.kind === "current") {
    envelope.active.results.resultSets[0].label = "Whitefield";
    envelope.active.results.resultSets[0].results[0].reasons = [{
      branchId: "branch-1", predicateId: "school", explanation: "Near Fixture School", proofToken: "signed:exact-receipt", showOnCard: true,
    }];
    const browseCards = envelope.active.results.resultSets[0].results.slice(0, 2).map((result, index) => ({
      id: `contextual-${index + 1}`,
      society_id: `society:contextual-${index + 1}`,
      title: `Contextual home ${index + 1}`,
      society_name: `Contextual home ${index + 1}`,
      area: index === 0 ? "Sarjapur" : "Yelahanka",
      image: result.image ?? "/landing/tiles/03-map-evidence-960.webp",
      bhk: result.bhk,
      price: 11_000_000 + index * 3_000_000,
      sqft: result.sqft,
      google_rating: result.google_rating,
      detail_href: `/property/contextual-${index + 1}`,
      save_id: `contextual-${index + 1}`,
    }));
    envelope.active.collections = [{
      id: "other-areas",
      strategy: "other_areas",
      title: "More to explore — Other areas",
      note: "Other areas",
      priceBand: { min: 11_000_000, max: 14_000_000, currency: "INR", label: "₹1.1–1.4 Cr" },
      cards: browseCards,
    }];
  }
  return envelope;
}

export function retainedFixture(parent = journeyFixture()): SearchJourneyEnvelope {
  return {
    ...parent,
    active: { ...parent.active, results: {
      kind: "retained", orderedResultIds: parent.active.results.orderedResultIds,
      resultFingerprint: parent.active.revision.resultFingerprint,
    } },
    attempt: { kind: "revision", outcome: "preservedParent", clarification: {
      code: "no_matches", message: "No homes match that change. Your previous search is unchanged.",
    } },
  };
}
