import assert from "node:assert/strict";
import test from "node:test";
import {
  LANDING_SEARCH_RAIL_CAP,
  composeLandingSearchRails,
  formatBudgetInr,
  landingSearchRailHomeCount,
  landingSearchRailTooLong,
} from "../src/lib/landing-search-rails.ts";
import type {
  PropertyCard,
  SearchIntent,
  SearchResponse,
  SearchResultFocus,
  SearchResultItem,
} from "../src/lib/types.ts";

function card(
  overrides: Partial<PropertyCard> & Pick<PropertyCard, "id" | "bhk">,
): PropertyCard {
  return {
    kg_entity_refs: {
      property_entity_id: `property:${overrides.id}`,
      society_entity_id: overrides.kg_entity_refs?.society_entity_id ?? `society:${overrides.id}`,
      area_entity_id: "area:whitefield",
    },
    title: overrides.title ?? `${overrides.bhk} BHK`,
    area: overrides.area ?? "Whitefield",
    price: overrides.price ?? 18_000_000,
    price_per_sqft: 10_000,
    bhk: overrides.bhk,
    sqft: 1200,
    society_name: overrides.society_name ?? "Prestige Waterford",
    builder_name: "Prestige",
    hero_image: "/img.jpg",
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

function result(
  overrides: Partial<SearchResultItem> & Pick<PropertyCard, "id" | "bhk">,
): SearchResultItem {
  return {
    ...card(overrides),
    match_score: overrides.match_score ?? 0.8,
    match_label: overrides.match_label ?? "Strong match",
    match_reason: overrides.match_reason ?? "Matches 3 BHK",
    match_explanation: overrides.match_explanation,
  };
}

function response(input: {
  intent?: Partial<SearchIntent>;
  focus?: SearchResultFocus;
  results: SearchResultItem[];
}): SearchResponse {
  return {
    query: "quiet 3BHK near schools under 2.5Cr",
    intent: {
      area: null,
      bhk: null,
      budget_max: null,
      preferences: [],
      ...input.intent,
    },
    results: input.results,
    area_context: null,
    total_results: input.results.length,
    focus: input.focus,
    knowledge_context: null,
  };
}

test("named society stays first and nearby homes follow", () => {
  const waterford = result({
    id: "waterford-3",
    bhk: 3,
    society_name: "Prestige Waterford",
    kg_entity_refs: {
      property_entity_id: "property:waterford-3",
      society_entity_id: "society:prestige-waterford",
      area_entity_id: "area:whitefield",
    },
  });
  const sibling = result({
    id: "waterford-2",
    bhk: 2,
    society_name: "Prestige Waterford",
    kg_entity_refs: {
      property_entity_id: "property:waterford-2",
      society_entity_id: "society:prestige-waterford",
      area_entity_id: "area:whitefield",
    },
  });
  const nearby = [
    result({ id: "lakeside", bhk: 3, society_name: "Prestige Lakeside", area: "Whitefield" }),
    result({ id: "oakwood", bhk: 3, society_name: "Brigade Oakwood", area: "Whitefield" }),
    result({ id: "sarjapur", bhk: 3, society_name: "Godrej Air", area: "Sarjapur" }),
  ];

  const rails = composeLandingSearchRails(response({
    intent: { bhk: 3, area: "Whitefield" },
    focus: {
      mode: "named_society",
      society_id: "society:prestige-waterford",
      society_name: "Prestige Waterford",
      focus_results: [waterford],
      sibling_configs: [sibling],
      more_homes: nearby,
    },
    results: [waterford, ...nearby],
  }));

  assert.equal(rails[0]?.id, "exact");
  assert.equal(rails[0]?.label, "Prestige Waterford");
  assert.deepEqual(rails[0]?.results.map((item) => item.id), ["waterford-3"]);
  assert.deepEqual(rails[0]?.siblings?.map((item) => item.id), ["waterford-2"]);
  assert.equal(rails[1]?.id, "nearby");
  assert.equal(rails[1]?.label, "Near Prestige Waterford");
  assert.deepEqual(rails[1]?.results.map((item) => item.id), ["lakeside", "oakwood"]);
  assert.equal(rails.at(-1)?.id, "more");
  assert.deepEqual(rails.at(-1)?.results.map((item) => item.id), ["sarjapur"]);
  assert.ok(!rails.flatMap((rail) => rail.results).some((item) => item.id === "waterford-2"));
});

test("soft query splits leftover homes by budget and preference", () => {
  const best = [
    result({
      id: "best-1",
      bhk: 3,
      price: 20_000_000,
      society_name: "Home One",
      match_reason: "Matches quiet, schools",
      match_explanation: {
        reasons: [{
          preference: "quiet",
          fact_key: "noise_level",
          display: "Quiet street",
          score: 0.8,
          confidence: 0.7,
          source_type: "reviews",
          scoring_method: "text",
        }],
        preference_coverage: [{ preference: "quiet", status: "matched", fact_key: "noise_level" }],
        graph_driven_pct: 1,
        total_facts_consulted: 2,
      },
    }),
    result({
      id: "best-2",
      bhk: 3,
      price: 22_000_000,
      society_name: "Home Two",
      match_reason: "Matches schools",
    }),
  ];
  const underBudget = result({
    id: "budget-1",
    bhk: 3,
    price: 18_000_000,
    society_name: "Value Home",
    match_reason: "Matches 3 BHK",
  });
  const quiet = result({
    id: "quiet-1",
    bhk: 2,
    price: 40_000_000,
    society_name: "Quiet Park",
    match_reason: "Quiet near tech parks",
    match_explanation: {
      reasons: [{
        preference: "quiet near tech parks",
        fact_key: "noise_level",
        display: "Quiet near tech parks",
        score: 0.7,
        confidence: 0.6,
        source_type: "reviews",
        scoring_method: "text",
      }],
      preference_coverage: [{
        preference: "quiet near tech parks",
        status: "matched",
        fact_key: "noise_level",
      }],
      graph_driven_pct: 1,
      total_facts_consulted: 1,
    },
  });
  const leftover = result({
    id: "other-1",
    bhk: 4,
    price: 50_000_000,
    society_name: "Other Home",
    match_reason: "Similar profile",
  });

  const rails = composeLandingSearchRails(response({
    intent: {
      bhk: 3,
      budget_max: 25_000_000,
      preferences: ["quiet near tech parks"],
      positive_preferences: [{
        raw_text: "quiet near tech parks",
        polarity: "positive",
        expanded_keys: [],
        weight: 1,
      }],
    },
    focus: {
      mode: "ranked_matches",
      focus_results: best,
      more_homes: [underBudget, quiet, leftover],
    },
    results: [...best, underBudget, quiet, leftover],
  }));

  assert.equal(rails[0]?.id, "best");
  assert.equal(rails[0]?.label, undefined);
  assert.deepEqual(rails[0]?.results.map((item) => item.id), ["best-1", "best-2"]);
  assert.equal(rails[1]?.label, "Under ₹2.5Cr");
  assert.deepEqual(rails[1]?.results.map((item) => item.id), ["budget-1"]);
  assert.equal(rails[2]?.label, "Quiet Near Tech Parks");
  assert.deepEqual(rails[2]?.results.map((item) => item.id), ["quiet-1"]);
  assert.equal(rails[3]?.label, "More homes");
  assert.deepEqual(rails[3]?.results.map((item) => item.id), ["other-1"]);
});

test("landing search does not drop leftover homes to look like a featured rail", () => {
  const focus = result({ id: "focus", bhk: 3, society_name: "Focus Home" });
  const more = Array.from({ length: 8 }, (_, index) => result({
    id: `more-${index}`,
    bhk: 3,
    society_name: `Society ${index}`,
    match_score: 0.3,
  }));

  const rails = composeLandingSearchRails(response({
    focus: {
      mode: "ranked_matches",
      focus_results: [focus],
      more_homes: more,
    },
    results: [focus, ...more],
  }));

  assert.equal(landingSearchRailHomeCount(rails), 9);
  assert.equal(rails.at(-1)?.id, "more");
  assert.equal(rails.at(-1)?.results.length, 8);
});

test("a home appears in the first rail it qualifies for", () => {
  const primary = result({ id: "primary", bhk: 3, society_name: "Primary" });
  const shared = result({
    id: "shared",
    bhk: 3,
    price: 20_000_000,
    society_name: "Shared",
    match_reason: "Quiet",
    match_explanation: {
      reasons: [{
        preference: "quiet",
        fact_key: "noise_level",
        display: "Quiet",
        score: 0.6,
        confidence: 0.5,
        source_type: "reviews",
        scoring_method: "text",
      }],
      preference_coverage: [{ preference: "quiet", status: "matched", fact_key: "noise_level" }],
      graph_driven_pct: 1,
      total_facts_consulted: 1,
    },
  });
  const onlyQuiet = result({
    id: "quiet-only",
    bhk: 2,
    price: 40_000_000,
    society_name: "Quiet Only",
    match_reason: "Quiet",
    match_explanation: {
      reasons: [{
        preference: "quiet",
        fact_key: "noise_level",
        display: "Quiet",
        score: 0.6,
        confidence: 0.5,
        source_type: "reviews",
        scoring_method: "text",
      }],
      preference_coverage: [{ preference: "quiet", status: "matched", fact_key: "noise_level" }],
      graph_driven_pct: 1,
      total_facts_consulted: 1,
    },
  });

  const rails = composeLandingSearchRails(response({
    intent: {
      budget_max: 25_000_000,
      positive_preferences: [{
        raw_text: "quiet",
        polarity: "positive",
        expanded_keys: [],
        weight: 1,
      }],
    },
    focus: {
      mode: "ranked_matches",
      focus_results: [primary],
      more_homes: [shared, onlyQuiet],
    },
    results: [primary, shared, onlyQuiet],
  }));

  const budgetRail = rails.find((rail) => rail.id === "budget");
  const quietRail = rails.find((rail) => rail.id === "pref-quiet");
  assert.deepEqual(budgetRail?.results.map((item) => item.id), ["shared"]);
  assert.deepEqual(quietRail?.results.map((item) => item.id), ["quiet-only"]);
});

test("budget leftover rails keep homes whose listing band overlaps", () => {
  const primary = result({ id: "primary", bhk: 3, society_name: "Primary" });
  const overlap = result({
    id: "overlap",
    bhk: 3,
    price: 32_250_000,
    price_min: 30_000_000,
    price_max: 48_000_000,
    society_name: "Lakefront",
  });
  const tooHigh = result({
    id: "too-high",
    bhk: 3,
    price: 50_000_000,
    price_min: 49_000_000,
    price_max: 55_000_000,
    society_name: "Premium",
  });

  const rails = composeLandingSearchRails(response({
    intent: { budget_max: 31_000_000 },
    focus: {
      mode: "ranked_matches",
      focus_results: [primary],
      more_homes: [overlap, tooHigh],
    },
    results: [primary, overlap, tooHigh],
  }));

  const budgetRail = rails.find((rail) => rail.id === "budget");
  assert.deepEqual(budgetRail?.results.map((item) => item.id), ["overlap"]);
});

test("formatBudgetInr keeps buyer-facing crore labels short", () => {
  assert.equal(formatBudgetInr(25_000_000), "₹2.5Cr");
  assert.equal(formatBudgetInr(30_000_000), "₹3Cr");
});

test("a large ranked set becomes short area rows instead of one long pager", () => {
  const areas = ["Whitefield", "Sarjapur", "Hebbal", "Electronic City"];
  const results = areas.flatMap((area, areaIndex) => (
    Array.from({ length: 6 }, (_, index) => result({
      id: `${area}-${index}`,
      bhk: 3,
      area,
      society_name: `${area} Society ${index}`,
      match_score: 0.9 - areaIndex * 0.05 - index * 0.01,
    }))
  ));

  const rails = composeLandingSearchRails(response({
    focus: {
      mode: "ranked_matches",
      focus_results: results,
      more_homes: [],
    },
    results,
  }));

  assert.equal(rails[0]?.id, "best");
  assert.ok((rails[0]?.results.length ?? 0) <= LANDING_SEARCH_RAIL_CAP);
  assert.equal(landingSearchRailTooLong(rails), false);
  assert.ok(rails.length >= 3);
  assert.ok(rails.length <= 8);
  const labels = rails.map((rail) => rail.label).filter(Boolean);
  assert.ok(labels.includes("Sarjapur") || labels.includes("Hebbal") || labels.includes("Electronic City"));
  assert.ok(rails.every((rail) => (rail.results.length + (rail.siblings?.length ?? 0)) <= LANDING_SEARCH_RAIL_CAP + 3));
});

test("same-area leftovers split on price instead of paging forever", () => {
  const results = Array.from({ length: 20 }, (_, index) => result({
    id: `home-${index}`,
    bhk: 3,
    area: "Whitefield",
    society_name: `Whitefield Society ${index}`,
    price: (1 + (index % 4)) * 10_000_000,
    match_score: 0.8 - index * 0.01,
  }));

  const rails = composeLandingSearchRails(response({
    focus: {
      mode: "ranked_matches",
      focus_results: results,
      more_homes: [],
    },
    results,
  }));

  assert.equal(landingSearchRailTooLong(rails), false);
  assert.ok(rails.some((rail) => rail.label?.includes("₹") || rail.label === "More homes"));
  assert.ok(rails.every((rail) => rail.results.length <= LANDING_SEARCH_RAIL_CAP));
  assert.ok(landingSearchRailHomeCount(rails) < results.length || rails.length > 1);
});

test("thin area rows merge instead of stacking two-card shelves", () => {
  const best = Array.from({ length: 4 }, (_, index) => result({
    id: `best-${index}`,
    bhk: 3,
    area: "Koramangala",
    society_name: `Best ${index}`,
    match_score: 0.9,
  }));
  const thin = [
    ...["Banashankari", "Begur Road", "Whitefield"].flatMap((area) => (
      [0, 1].map((index) => result({
        id: `${area}-${index}`,
        bhk: 3,
        area,
        society_name: `${area} ${index}`,
        match_score: 0.4,
      }))
    )),
    result({
      id: "itpl-1",
      bhk: 2,
      area: "itpl, Whitefield",
      society_name: "ITPL Home",
      match_score: 0.4,
    }),
    result({
      id: "itpl-2",
      bhk: 3,
      area: "ITPL",
      society_name: "Another ITPL",
      match_score: 0.4,
    }),
  ];

  const rails = composeLandingSearchRails(response({
    focus: {
      mode: "ranked_matches",
      focus_results: best,
      more_homes: thin,
    },
    results: [...best, ...thin],
  }));

  const areaRows = rails.filter((rail) => rail.id.startsWith("area-"));
  assert.equal(areaRows.length, 0);
  const more = rails.find((rail) => rail.id === "more");
  assert.ok((more?.results.length ?? 0) >= 6);
  assert.ok((more?.results.length ?? 0) <= LANDING_SEARCH_RAIL_CAP);
  assert.equal(rails.filter((rail) => rail.label === "Banashankari").length, 0);
  assert.equal(rails.filter((rail) => rail.label === "Begur Road").length, 0);
});

test("ITPL listings join the Whitefield row instead of a second header", () => {
  const best = Array.from({ length: 3 }, (_, index) => result({
    id: `best-${index}`,
    bhk: 3,
    area: "Koramangala",
    society_name: `Best ${index}`,
    match_score: 0.9,
  }));
  const east = [
    ...Array.from({ length: 7 }, (_, index) => result({
      id: `wf-${index}`,
      bhk: 3,
      area: "Whitefield",
      society_name: `Whitefield ${index}`,
      match_score: 0.6,
    })),
    result({ id: "itpl-a", bhk: 2, area: "itpl, Whitefield", society_name: "ITPL A", match_score: 0.6 }),
    result({ id: "itpl-b", bhk: 3, area: "ITPL, Whitefield", society_name: "ITPL B", match_score: 0.6 }),
  ];

  const rails = composeLandingSearchRails(response({
    focus: {
      mode: "ranked_matches",
      focus_results: best,
      more_homes: east,
    },
    results: [...best, ...east],
  }));

  const whitefield = rails.find((rail) => rail.label === "Whitefield");
  assert.ok(whitefield);
  assert.ok((whitefield?.results.length ?? 0) >= 4);
  assert.equal(rails.filter((rail) => /itpl/i.test(rail.label ?? "")).length, 0);
});

test("indistinguishable leftovers stay one short More homes row", () => {
  const results = Array.from({ length: 24 }, (_, index) => result({
    id: `clone-${index}`,
    bhk: 3,
    area: "Whitefield",
    society_name: `Clone ${index}`,
    price: 18_000_000,
    match_score: 0.5,
  }));

  const rails = composeLandingSearchRails(response({
    focus: {
      mode: "ranked_matches",
      focus_results: results,
      more_homes: [],
    },
    results,
  }));

  assert.equal(landingSearchRailTooLong(rails), false);
  assert.ok(rails.every((rail) => rail.results.length <= LANDING_SEARCH_RAIL_CAP));
  assert.ok((rails.find((rail) => rail.id === "more")?.results.length ?? 0) <= LANDING_SEARCH_RAIL_CAP);
});
