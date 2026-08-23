import assert from "node:assert/strict";
import test from "node:test";
import {
  societyKey,
  uniqueSocietiesForDiscovery,
} from "../src/lib/property-filters.ts";
import type { PropertyCard } from "../src/lib/types.ts";

function card(overrides: Partial<PropertyCard> & Pick<PropertyCard, "id" | "bhk">): PropertyCard {
  return {
    kg_entity_refs: {
      property_entity_id: `property:${overrides.id}`,
      society_entity_id: overrides.kg_entity_refs?.society_entity_id ?? "society:godrej-air",
      area_entity_id: "area:sarjapur",
    },
    title: overrides.title ?? `${overrides.bhk} BHK`,
    area: "Sarjapur",
    price: overrides.price ?? 15_000_000,
    price_per_sqft: 10_000,
    bhk: overrides.bhk,
    sqft: 1200,
    society_name: overrides.society_name ?? "Godrej Air",
    builder_name: "Godrej",
    hero_image: overrides.hero_image ?? null,
    transparency_tags: [],
    description_summary: "",
    possession_status: "Ready",
    metro_distance_mins: 20,
    floor: 5,
    total_floors: 20,
    facing: "East",
    ...overrides,
  };
}

test("uniqueSocietiesForDiscovery keeps one card per society", () => {
  const properties = [
    card({ id: "godrej-1", bhk: 1, hero_image: null }),
    card({ id: "godrej-2", bhk: 2, hero_image: "/img/2.jpg" }),
    card({ id: "godrej-3", bhk: 3, hero_image: "/img/3.jpg", google_rating: 4.2 }),
    card({
      id: "prestige-3",
      bhk: 3,
      society_name: "Prestige Lakeside",
      kg_entity_refs: {
        property_entity_id: "property:prestige-3",
        society_entity_id: "society:prestige-lakeside",
        area_entity_id: "area:varthur",
      },
      hero_image: "/img/p.jpg",
    }),
    card({ id: "zero-price", bhk: 2, price: 0 }),
  ];

  const unique = uniqueSocietiesForDiscovery(properties);
  assert.equal(unique.length, 2);
  assert.deepEqual(
    unique.map((p) => p.id).sort(),
    ["godrej-3", "prestige-3"],
  );
  assert.equal(societyKey(unique[0]!), "society:godrej-air");
});
