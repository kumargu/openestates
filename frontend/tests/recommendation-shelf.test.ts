import assert from "node:assert/strict";
import test from "node:test";
import { recommendationShelfItems } from "../src/lib/recommendations.ts";
import type { PropertyCard, RecommendationBranch } from "../src/lib/types.ts";

function card(
  id: string,
  society: string,
  societyId = `society:${society.toLocaleLowerCase("en-IN").replaceAll(" ", "-")}`,
): PropertyCard {
  return {
    id,
    kg_entity_refs: {
      property_entity_id: `property:${id}`,
      society_entity_id: societyId,
      area_entity_id: "area:test",
    },
    title: society,
    area: "Test Area",
    price: 20_000_000,
    price_per_sqft: 12_000,
    bhk: 3,
    sqft: 1_500,
    society_name: society,
    builder_name: "Test Builder",
    images: [],
    hero_image: `/images/${id}.jpg`,
    transparency_tags: [],
    description_summary: "",
    possession_status: "Ready",
    metro_distance_mins: 10,
    floor: 5,
    total_floors: 20,
    facing: "East",
  };
}

function branch(id: string, property: PropertyCard): RecommendationBranch {
  return {
    branch_id: id,
    lens: "proof",
    headline: "Alternative",
    property,
    contrast: "Configured contrast",
    evidence_delta: {
      fact_count: 1,
      gap_count: 0,
      fact_delta: 1,
      gap_delta: 0,
    },
    channels: [{ channel: "spatial_nearby", score: 0.8 }],
    magnitude: 0.8,
  };
}

test("recommendation shelf preserves backend order without global inventory fallback", () => {
  const current = card("anchor", "Anchor Court");
  const branches = [
    branch("first", card("first-home", "First Home")),
    branch("second", card("second-home", "Second Home")),
  ];

  const items = recommendationShelfItems(branches, current, new Set());

  assert.deepEqual(items.map((item) => item.property.id), ["first-home", "second-home"]);
});

test("recommendation shelf keeps one qualified home per society", () => {
  const current = card("anchor", "Anchor Court");
  const sharedSociety = "society:shared";
  const branches = [
    branch("first", card("shared-a", "Shared A", sharedSociety)),
    branch("duplicate", card("shared-b", "Shared B", sharedSociety)),
    branch("other", card("other", "Other Home")),
  ];

  const items = recommendationShelfItems(branches, current, new Set());

  assert.deepEqual(items.map((item) => item.property.id), ["shared-a", "other"]);
});
