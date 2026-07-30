import assert from "node:assert/strict";
import test from "node:test";
import {
  decisionLabelFacets,
  mapContextFacets,
  notebookNoteFacets,
  propertyBaselineFacets,
  savedFinancialPlanFacets,
  type SavedFinancialPlan,
} from "../src/lib/decisionFacets.ts";
import type { NotebookNote } from "../src/lib/notebook.ts";
import type { PropertyCard, PropertyMapContext } from "../src/lib/types.ts";

const property: PropertyCard = {
  id: "home-1",
  kg_entity_refs: {
    property_entity_id: "property:home-1",
    society_entity_id: "society:one",
    area_entity_id: "area:one",
  },
  title: "3 BHK Home",
  area: "Whitefield",
  price: 20_000_000,
  price_per_sqft: 12_500,
  bhk: 3,
  sqft: 1600,
  society_name: "Society One",
  builder_name: "Builder One",
  hero_image: null,
  transparency_tags: [],
  description_summary: "",
  possession_status: "Ready",
  metro_distance_mins: 12,
  floor: 4,
  total_floors: 18,
  facing: "East",
  decision_labels: [{
    key: "project_complaints",
    label: "Project complaints",
    severity: "risk",
    scope: "project",
    visualId: "risk",
    valueText: "3 open",
    surfaces: ["compare"],
    priority: 7,
    sourceFactKeys: ["rera.project_complaints"],
    confidence: 0.82,
    compareGroup: "legal_project",
    groupId: "attention",
    placement: "primary",
  }, {
    key: "builder_delivery",
    label: "Builder delivery",
    severity: "positive",
    scope: "builder",
    visualId: "builder",
    valueText: "On-time record",
    surfaces: ["compare"],
    priority: 8,
    sourceFactKeys: ["builder.delivery_score"],
    confidence: 0.76,
    compareGroup: "legal_project",
    groupId: "project_facts",
    placement: "more",
  }, {
    key: "area_water",
    label: "Area water",
    severity: "caution",
    scope: "area",
    visualId: "water",
    valueText: "Seasonal stress",
    surfaces: ["compare"],
    priority: 9,
    sourceFactKeys: ["area.water_stress"],
    confidence: 0.7,
    compareGroup: "map_water",
    groupId: "attention",
    placement: "more",
  }],
};

test("property facets preserve stable identity, origin, scope, and source refs", () => {
  const facets = propertyBaselineFacets(property);
  const byTopic = new Map(facets.map((item) => [item.topic, item]));

  assert.equal(byTopic.get("price")?.id, "canonical:home-1:price");
  assert.equal(byTopic.get("price")?.origin, "canonical_fact");
  assert.equal(byTopic.get("price")?.scope, "property");
  assert.equal(byTopic.get("price")?.value, 20_000_000);
  assert.equal(byTopic.get("home_state")?.scope, "society");
  assert.match(
    byTopic.get("project_complaints")?.id ?? "",
    /^canonical:home-1:project-complaints:rera-project-complaints-[a-z0-9]+$/,
  );
  assert.equal(byTopic.get("project_complaints")?.scope, "project");
  assert.equal(byTopic.get("project_complaints")?.sourceRef?.recordId, "rera.project_complaints");
  assert.deepEqual(byTopic.get("project_complaints")?.compare, { group: "legal_project", rank: 7 });
  assert.equal(byTopic.get("builder_delivery")?.scope, "builder");
  assert.match(
    byTopic.get("builder_delivery")?.id ?? "",
    /^canonical:home-1:builder-delivery:builder-delivery-score-[a-z0-9]+$/,
  );
  assert.equal(byTopic.get("area_water")?.scope, "area");
});

test("canonical label facets avoid source and fallback handle collisions", () => {
  const duplicateFallbacks = decisionLabelFacets({
    propertyId: "home-1",
    labels: [{
      key: "same_key",
      label: "Same key",
      severity: "risk",
      scope: "project",
      visualId: "risk",
      valueText: "First",
      priority: 1,
      confidence: 0.7,
      groupId: "attention",
      placement: "primary",
    }, {
      key: "same_key",
      label: "Same key",
      severity: "risk",
      scope: "project",
      visualId: "risk",
      valueText: "Second",
      priority: 2,
      confidence: 0.7,
      groupId: "documents",
      placement: "primary",
    }],
  });
  const opaqueSources = decisionLabelFacets({
    propertyId: "home-1",
    labels: [{
      key: "opaque",
      label: "Opaque",
      severity: "info",
      scope: "project",
      visualId: "info",
      priority: 1,
      sourceFactKeys: ["source:a/b"],
      confidence: 0.7,
      groupId: "project_facts",
      placement: "more",
    }, {
      key: "opaque",
      label: "Opaque",
      severity: "info",
      scope: "project",
      visualId: "info",
      priority: 1,
      sourceFactKeys: ["source:a:b"],
      confidence: 0.7,
      groupId: "project_facts",
      placement: "more",
    }],
  });

  assert.notEqual(duplicateFallbacks[0].id, duplicateFallbacks[1].id);
  assert.notEqual(opaqueSources[0].id, opaqueSources[1].id);
  assert.match(opaqueSources[0].id, /^canonical:home-1:opaque:source-a-b-[a-z0-9]+$/);
  assert.match(opaqueSources[1].id, /^canonical:home-1:opaque:source-a-b-[a-z0-9]+$/);
});

test("map context facets expose map facts without notebook label semantics", () => {
  const context: PropertyMapContext = {
    home: { entity_id: "society:one", name: "Society One" },
    places: [{
      feature_id: "school:one",
      layer: "schools",
      name: "Green School",
      distance_km: 0.8,
      rating: 4.3,
      review_count: 210,
      source_type: "Google",
      source_url: "https://example.com/school",
    }, {
      layer: "schools",
      name: "School Without ID",
      latitude: 12.93,
      longitude: 77.62,
      distance_km: 1.2,
      source_type: "Google",
      source_url: "https://example.com/unindexed-school",
    }],
    water: {
      groundwater_class: "safe",
      summary: "No severe groundwater stress in the mapped radius.",
      source_type: "public_layer",
      illustrative_zone: false,
    },
  };

  const facets = mapContextFacets("home-1", context);
  const school = facets.find((item) => item.topic === "schools");
  const fallbackSchool = facets.find((item) => item.label === "School Without ID");
  const water = facets.find((item) => item.topic === "water");

  assert.match(school?.id ?? "", /^map:home-1:place:school-one-[a-z0-9]+$/);
  assert.equal(school?.origin, "map_fact");
  assert.equal(school?.scope, "society");
  assert.equal(school?.value, 0.8);
  assert.match(school?.detail ?? "", /0.8 km/);
  assert.equal(school?.sourceRef?.url, "https://example.com/school");
  assert.match(
    fallbackSchool?.id ?? "",
    /^map:home-1:place:schools-school-without-id-https-example-com-unindexed-school-google-12-93-77-62-1-2-[a-z0-9]+$/,
  );
  assert.equal(water?.compare?.group, "map_water");

  const reordered = mapContextFacets("home-1", {
    ...context,
    places: [...context.places].reverse(),
  }).find((item) => item.label === "School Without ID");
  assert.equal(reordered?.id, fallbackSchool?.id);
});

test("map fallback identities survive missing coordinates and URLs", () => {
  const context: PropertyMapContext = {
    home: { entity_id: "society:one", name: "Society One" },
    places: [{
      layer: "schools",
      name: "Duplicate Name",
      distance_km: 1.1,
      source_type: "Google",
    }, {
      layer: "schools",
      name: "Duplicate Name",
      distance_km: 1.4,
      source_type: "Google",
    }],
  };

  const facets = mapContextFacets("home-1", context);
  const reorderedFacets = mapContextFacets("home-1", {
    ...context,
    places: [...context.places].reverse(),
  });

  assert.notEqual(facets[0].id, facets[1].id);
  assert.deepEqual(
    new Set(reorderedFacets.map((item) => item.id)),
    new Set(facets.map((item) => item.id)),
  );
});

test("notebook v2 adapter stops making visible labels the only semantic contract", () => {
  const notes: NotebookNote[] = [
    {
      id: "note-1",
      propertyId: "home-1",
      title: "School nearby",
      detail: "0.8 km",
      kind: "fact",
      catalogKey: "nearby:school",
      labels: ["schools", "finance"],
      createdAt: 1,
    },
    {
      id: "note-2",
      propertyId: "home-1",
      title: "Visit",
      kind: "handwritten",
      catalogKey: "block:visit",
      labels: [],
      block: { type: "checklist", collapsed: false, items: [] },
      createdAt: 2,
    },
  ];

  const facets = notebookNoteFacets(notes);

  assert.deepEqual(facets.map((item) => item.id), [
    "notebook:note-1:nearby-fact",
    "notebook:note-1:label:schools",
    "notebook:note-1:label:finance",
    "notebook:note-2:checklist",
  ]);
  assert.equal(facets[0].origin, "user_note");
  assert.equal(facets[0].compare?.group, "access_notes");
  assert.equal(facets[1].topic, "label:schools");
  assert.equal(facets[1].compare?.group, "access_notes");
  assert.equal(facets[2].topic, "label:finance");
  assert.equal(facets[2].compare, undefined);
  assert.equal(facets[3].origin, "smart_block");
  assert.equal(facets[3].compare, undefined);
});

test("notebook plan snapshots stay notebook-origin and do not emit label facets", () => {
  const notes: NotebookNote[] = [{
    id: "plan-note-1",
    propertyId: "home-1",
    title: "Saved plan from last visit",
    detail: "EMI looked comfortable.",
    kind: "plan",
    catalogKey: "plan:summary",
    labels: ["finance", "emi", "price"],
    createdAt: 3,
  }];

  const facets = notebookNoteFacets(notes);

  assert.deepEqual(facets.map((item) => item.id), ["notebook:plan-note-1:plan-snapshot"]);
  assert.equal(facets[0].topic, "plan_snapshot");
  assert.equal(facets[0].origin, "user_note");
  assert.equal(facets[0].compare, undefined);
});

test("notebook semantic facets keep compare grouping when labels are removed", () => {
  const facets = notebookNoteFacets([{
    id: "note-no-labels",
    propertyId: "home-1",
    title: "School nearby",
    detail: "0.8 km",
    kind: "fact",
    catalogKey: "nearby:school",
    labels: [],
    createdAt: 4,
  }]);

  assert.deepEqual(facets.map((item) => item.id), ["notebook:note-no-labels:nearby-fact"]);
  assert.equal(facets[0].topic, "nearby_fact");
  assert.equal(facets[0].compare?.group, "access_notes");
});

function savedPlan(overrides: Partial<SavedFinancialPlan> = {}): SavedFinancialPlan {
  return {
    id: "plan:home-1",
    propertyId: "home-1",
    modelVersion: "v1",
    shared: {
      propertyPrice: 20_000_000,
    },
    monthlyPath: {
      monthlyEmi: 145_000,
      currentRent: 55_000,
      monthlySip: 40_000,
      loanRate: 8.5,
      sipReturn: 10,
      extraEmisPerYear: 3,
      holdingPeriodYears: 20,
      inspectedYear: 12,
      purchaseYear: 0,
      constructionProfile: {
        state: "ready",
        asOfDate: "2026-01-01",
        dateSource: "not_applicable",
      },
      planAssumptions: {
        homeAppreciationRate: 6,
        rentInflationRate: 10,
      },
    },
    outputs: {
      loanFreeYear: 13,
      breakEvenYear: 9,
      buyNetWorthAtInspectedYear: 31_000_000,
      rentNetWorthAtInspectedYear: 24_000_000,
      totalInterest: 8_500_000,
      loanAmount: 20_000_000,
    },
    updatedAt: 1,
    ...overrides,
  };
}

test("saved financial plan facets expose one structured active plan", () => {
  const plan = savedPlan();

  const facets = savedFinancialPlanFacets(plan);
  const byTopic = new Map(facets.map((item) => [item.topic, item]));

  assert.match(byTopic.get("monthly_emi")?.id ?? "", /^financial-plan:plan-home-1-[a-z0-9]+:monthly_emi$/);
  assert.equal(byTopic.get("property_price")?.value, 20_000_000);
  assert.equal(byTopic.get("loan_free_year")?.value, 13);
  assert.equal(byTopic.get("inspected_year_outcome")?.value, 7_000_000);
  assert.equal(byTopic.get("monthly_emi")?.sourceRef?.recordId, "plan:home-1");
  assert.ok(facets.every((item) => item.origin === "financial_plan"));
  assert.ok(facets.every((item) => item.compare?.group === "financial_plan"));
});

test("saved financial plan facets do not require cash-needed outputs", () => {
  const facets = savedFinancialPlanFacets(savedPlan());
  const byTopic = new Set(facets.map((item) => item.topic));

  assert.equal(byTopic.has("cash_required"), false);
  assert.equal(byTopic.has("funding_gap"), false);
  assert.equal(byTopic.has("upfront_payment"), false);
});

test("saved financial plan rejects invalid or inconsistent monthly plan numbers", () => {
  assert.throws(
    () => savedFinancialPlanFacets(savedPlan({
      outputs: {
        ...savedPlan().outputs,
        totalInterest: Number.NaN,
      },
    })),
    /totalInterest must be a finite number/,
  );
  assert.throws(
    () => savedFinancialPlanFacets(savedPlan({
      monthlyPath: {
        ...savedPlan().monthlyPath,
        planAssumptions: {
          ...savedPlan().monthlyPath.planAssumptions,
          rentInflationRate: Number.POSITIVE_INFINITY,
        },
      },
    })),
    /rentInflationRate must be a finite number/,
  );
  assert.throws(
    () => savedFinancialPlanFacets(savedPlan({
      outputs: {
        ...savedPlan().outputs,
        loanAmount: 19_000_000,
      },
    })),
    /loanAmount must equal 20000000/,
  );
  assert.throws(
    () => savedFinancialPlanFacets(savedPlan({
      monthlyPath: {
        ...savedPlan().monthlyPath,
        constructionProfile: {
          state: "done",
          asOfDate: "2026-01-01",
          dateSource: "not_applicable",
        } as unknown as SavedFinancialPlan["monthlyPath"]["constructionProfile"],
      },
    })),
    /constructionProfile\.state/,
  );
  assert.throws(
    () => savedFinancialPlanFacets(savedPlan({
      outputs: {
        ...savedPlan().outputs,
        loanFreeYear: null,
        totalInterest: 100_000,
      },
    })),
    /totalInterest must be null/,
  );
});
