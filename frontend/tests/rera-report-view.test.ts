import assert from "node:assert/strict";
import test from "node:test";
import {
  buildReraReportViewModel,
  claimValueText,
  displayFactsForSection,
  httpUrl,
  orderReraDocuments,
  orderReraRegulatoryEvents,
  previewReraRegulatoryEvents,
  projectReraInventoryChart,
  regulatoryCoverageNote,
  regulatoryEventPresentation,
  selectReraPlanPreviews,
  sectionHasEvidence,
  selectorMatches,
} from "../src/lib/reraReportView.ts";
import type {
  ReraBuyerDocument,
  ReraEvidenceClaim,
  ReraEvidenceProjection,
  ReraEvidenceReportResponse,
  ReraReportSurfaceSection,
} from "../src/lib/types.ts";

const DOCUMENTS: ReraBuyerDocument[] = [
  { id: "page-10", label: "Approved plan page 10", group: "plans", group_label: "Plans", url: "https://rera.example/10" },
  { id: "page-2", label: "Approved plan page 2", group: "plans", group_label: "Plans", url: "https://rera.example/2" },
  { id: "noc", label: "Fire NOC", group: "approvals", group_label: "Approvals", url: "https://rera.example/noc" },
];

const CLAIM: ReraEvidenceClaim = {
  claim_id: "claim:one",
  subject: { entity_id: "registration:one", entity_type: "registration" },
  predicate: "official_registration_number",
  value: { type: "text", data: "PRM/KA/RERA/TEST" },
  assertion_mode: "registry_record",
  source_trust: "primary_authority",
  evidence: [{
    source_record_id: "record:one",
    receipt_id: "receipt:one",
    capture_id: "capture:one",
    locator: "listing[0]",
    page: 4,
    supporting_quote: "The Authority records the project-specific finding.",
  }],
};

const EVIDENCE: ReraEvidenceProjection = {
  schema_version: "rera_evidence_projection.v2",
  property_id: "property:one",
  bundle_id: "bundle:one",
  generated_at: "2026-08-10T00:00:00Z",
  registration_ids: ["registration:one"],
  entities: [],
  claims: [CLAIM],
  events: [],
  series: [],
  discrepancies: [],
  regulatory_coverage: [],
  source_index: [],
};

const SECTION: ReraReportSurfaceSection = {
  id: "registration",
  title: "Official registration record",
  renderer: "fact_list",
  selectors: [{ key: "claim:official_registration_number", label: "Registration number" }],
  preview_kinds: [],
  empty_behavior: "omit",
};

test("RERA selectors match exact and configured wildcard predicates", () => {
  assert.equal(selectorMatches("claim:official_registration_number", "claim:official_registration_number"), true);
  assert.equal(selectorMatches("claim:declared_inventory_*", "claim:declared_inventory_unit_count"), true);
  assert.equal(selectorMatches("claim:complaint_*", "claim:declared_inventory_unit_count"), false);
});

test("RERA sections omit themselves when the evidence product has no matching data", () => {
  assert.equal(sectionHasEvidence(SECTION, EVIDENCE), true);
  assert.equal(sectionHasEvidence({
    ...SECTION,
    selectors: [{ key: "claim:declared_water_source", label: "Water source" }],
  }, EVIDENCE), false);
});

test("RERA display facts use config labels and coalesce repeated source assertions", () => {
  const facts = displayFactsForSection(SECTION, {
    ...EVIDENCE,
    claims: [CLAIM, { ...CLAIM, claim_id: "claim:two" }],
  });
  assert.equal(facts.length, 1);
  assert.equal(facts[0].label, "Registration number");
  assert.equal(facts[0].value, "PRM/KA/RERA/TEST");
  assert.equal(facts[0].claims.length, 2);
});

test("RERA display facts omit incomplete label-only values", () => {
  const facts = displayFactsForSection({
    ...SECTION,
    selectors: [{ key: "claim:declared_water_local_authority", label: "Local authority" }],
  }, {
    ...EVIDENCE,
    claims: [{
      ...CLAIM,
      predicate: "declared_water_local_authority",
      value: { type: "text", data: "Water Supply :" },
    }],
  });
  assert.deepEqual(facts, []);
});

test("RERA values retain units without deriving per-home carpet area", () => {
  assert.equal(claimValueText({ type: "number", data: 53728 }, "square_metres"), "53,728 m²");
  assert.equal(
    claimValueText({ type: "number", data: 53728 }, "square_feet_from_square_metres"),
    "5,78,323 sq ft",
  );
  assert.equal(claimValueText({ type: "boolean", data: false }), "No");
  assert.equal(httpUrl("javascript:alert(1)"), null);
});

const INVENTORY_SECTION: ReraReportSurfaceSection = {
  id: "inventory",
  title: "Homes and carpet area",
  renderer: "dual_bar_chart",
  selectors: [
    { key: "entity:inventory_configuration", label: "Configuration" },
    { key: "claim:inventory_homes", label: "Homes", format: "integer" },
    { key: "claim:inventory_total_area", label: "Total carpet area", format: "square_feet_from_square_metres" },
    { key: "claim:inventory_filed_area", label: "Filed carpet area", format: "square_feet_from_square_metres" },
  ],
  preview_kinds: [],
  empty_behavior: "omit",
};

function inventoryClaim(
  entityId: string,
  predicate: string,
  value: number,
  id = `${entityId}:${predicate}`,
): ReraEvidenceClaim {
  return {
    ...CLAIM,
    claim_id: id,
    subject: { entity_id: entityId, entity_type: "inventory_configuration" },
    predicate,
    value: { type: "number", data: value },
  };
}

test("RERA inventory chart scales homes and carpet area independently", () => {
  const entities = Array.from({ length: 18 }, (_, index) => ({
    entity_id: `inventory:${index + 1}`,
    entity_type: "inventory_configuration",
    label: index === 0 ? "1 BHK" : `${index + 1} BHK type`,
  }));
  const claims = entities.flatMap((entity, index) => [
    inventoryClaim(entity.entity_id, "inventory_homes", index === 0 ? 8 : (index + 1) * 20),
    inventoryClaim(entity.entity_id, "inventory_total_area", index === 0 ? 271 : (index + 1) * 1000),
  ]);
  const rows = projectReraInventoryChart(INVENTORY_SECTION, { ...EVIDENCE, entities, claims });

  assert.equal(rows.length, 18);
  assert.equal(rows.at(-1)?.homesPercent, 100);
  assert.equal(rows.at(-1)?.carpetAreaPerHomePercent, 100);
  assert.equal(rows[0].homesDisplay, "8");
  assert.equal(rows[0].carpetAreaPerHomeDisplay, "365 sq ft");
  assert.ok(rows[0].homesPercent < rows[0].carpetAreaPerHomePercent);
});

test("RERA inventory chart falls back to filed area and retains missing values", () => {
  const entities = [
    { entity_id: "inventory:filed", entity_type: "inventory_configuration", label: "4 BHK" },
    { entity_id: "inventory:missing", entity_type: "inventory_configuration", label: "Studio" },
  ];
  const rows = projectReraInventoryChart(INVENTORY_SECTION, {
    ...EVIDENCE,
    entities,
    claims: [
      inventoryClaim("inventory:filed", "inventory_homes", 31),
      inventoryClaim("inventory:filed", "inventory_filed_area", 4661),
      inventoryClaim("inventory:missing", "inventory_homes", 0),
    ],
  });

  assert.equal(rows[0].carpetAreaLabel, "Filed carpet area");
  assert.equal(rows[0].carpetAreaPerHomeDisplay, "1,618 sq ft");
  assert.equal(rows[1].homesDisplay, "0");
  assert.equal(rows[1].homesPercent, 0);
  assert.equal(rows[1].carpetAreaPerHomeDisplay, "—");
});

test("RERA inventory chart omits area-only rows that cannot produce a per-home value", () => {
  const entity = { entity_id: "inventory:area-only", entity_type: "inventory_configuration", label: "3 BHK" };
  const rows = projectReraInventoryChart(INVENTORY_SECTION, {
    ...EVIDENCE,
    entities: [entity],
    claims: [inventoryClaim(entity.entity_id, "inventory_total_area", 1200)],
  });

  assert.deepEqual(rows, []);
});

test("RERA inventory chart preserves distinct repeated filings and coalesces exact duplicates", () => {
  const entities = [
    { entity_id: "inventory:first", entity_type: "inventory_configuration", label: "2 BHK" },
    { entity_id: "inventory:duplicate", entity_type: "inventory_configuration", label: "2-bhk" },
    { entity_id: "inventory:distinct", entity_type: "inventory_configuration", label: "2 BHK" },
  ];
  const claims = [
    inventoryClaim("inventory:first", "inventory_homes", 20),
    inventoryClaim("inventory:first", "inventory_total_area", 1000),
    inventoryClaim("inventory:duplicate", "inventory_homes", 20),
    inventoryClaim("inventory:duplicate", "inventory_total_area", 1000),
    inventoryClaim("inventory:distinct", "inventory_homes", 30),
    inventoryClaim("inventory:distinct", "inventory_total_area", 1400),
  ];
  const rows = projectReraInventoryChart(INVENTORY_SECTION, { ...EVIDENCE, entities, claims });

  assert.equal(rows.length, 2);
  assert.deepEqual(rows.map(({ homes }) => homes), [20, 30]);
});

test("RERA plan previews reject brochures carried by an older bundle", () => {
  const previews = [
    { artifact_id: "plan", kind: "sanction_plan", label: "Approved plan", preview_url: "/plan.png", confidence: 1 },
    { artifact_id: "brochure", kind: "brochure", label: "Brochure", preview_url: "/brochure.png", confidence: 1 },
  ];
  assert.deepEqual(
    selectReraPlanPreviews(previews, ["site_plan", "sanction_plan"])
      .map((preview) => preview.artifact_id),
    ["plan"],
  );
});

test("RERA documents retain every filing and sort numbered pages naturally", () => {
  assert.deepEqual(
    orderReraDocuments(DOCUMENTS).map((document) => document.id),
    ["noc", "page-2", "page-10"],
  );
  assert.equal(orderReraDocuments(DOCUMENTS).length, DOCUMENTS.length);
});

test("RERA regulatory preview follows configured priority and caps the initial chronology", () => {
  const events = [
    ["historical", "2026-08-01"],
    ["promoter_disclosure", "2026-08-02"],
    ["enforcement", "2026-07-01"],
    ["registration_restriction", "2026-06-01"],
  ].map(([eventClass, occurredAt], index) => ({
    event_id: `event:${index}`,
    registration_id: "registration:one",
    event_class: eventClass,
    event_type: "authority_order",
    occurred_at: occurredAt,
    issuer: "K-RERA",
    decision_stage: "final_authority_order",
    current_effect: `Effect ${index}`,
    claim_ids: [],
    source_ids: [],
  }));
  const eventOrder = ["registration_restriction", "enforcement", "promoter_disclosure", "historical"];

  assert.deepEqual(
    orderReraRegulatoryEvents(events, eventOrder).map((event) => event.event_class),
    eventOrder,
  );
  assert.deepEqual(
    previewReraRegulatoryEvents(events, eventOrder).map((event) => event.event_class),
    eventOrder.slice(0, 3),
  );
  assert.deepEqual(previewReraRegulatoryEvents([], eventOrder), []);
});

test("RERA coverage note names only sources actually checked", () => {
  assert.equal(
    regulatoryCoverageNote(
      [{ source: "K-RERA", checked_at: "2026-08-11T00:00:00Z", status: "checked" }],
      "High Court proceedings are outside this release.",
    ),
    "K-RERA checked; High Court proceedings are outside this release.",
  );
  assert.equal(
    regulatoryCoverageNote(
      [{ source: "K-RERA archive", checked_at: "2026-08-11T00:00:00Z", status: "unavailable" }],
      "High Court proceedings are outside this release.",
    ),
    null,
  );
});

test("RERA regulatory event exposes one contextual action with exact quote support", () => {
  const event = {
    event_id: "event:one",
    registration_id: "registration:one",
    event_class: "final_finding",
    event_type: "authority_order",
    occurred_at: "2026-08-01",
    issuer: "K-RERA",
    decision_stage: "final_authority_order",
    current_effect: "The Authority recorded a project-specific finding.",
    claim_ids: [CLAIM.claim_id],
    source_ids: ["receipt:one"],
  };
  const presentation = regulatoryEventPresentation(event, {
    ...EVIDENCE,
    claims: [{ ...CLAIM, assertion_mode: "authority_order" }],
    events: [event],
    source_index: [{
      receipt_id: "receipt:one",
      capture_id: "capture:one",
      source_url: "https://rera.example/order.pdf",
      captured_at: "2026-08-01T00:00:00Z",
      content_type: "application/pdf",
    }],
  });

  assert.equal(presentation.actionLabel, "Open order");
  assert.equal(presentation.source?.source_url, "https://rera.example/order.pdf");
  assert.equal(presentation.supportingEvidence?.page, 4);
  assert.equal(
    presentation.supportingEvidence?.supporting_quote,
    "The Authority records the project-specific finding.",
  );
});

function reportFixture(
  overrides: Partial<ReraEvidenceReportResponse> = {},
): ReraEvidenceReportResponse {
  return {
    availability: "available",
    evidence: EVIDENCE,
    surface: {
      version: 1,
      coverage_note: "Archive not checked.",
      regulatory_event_order: [],
      sections: [],
    },
    buyer_report: {
      registry_url: "https://rera.example/project",
      fact_sections: [
        {
          id: "registration",
          title: "Registration",
          facts: [
            { key: "rera_number", label: "Registration number", value: "PRM/KA/ONE", learned_at: "2026-08-10" },
            { key: "rera_status", label: "Status", value: "APPROVED", learned_at: "2026-08-10" },
          ],
        },
        {
          id: "schedule",
          title: "Schedule",
          facts: [
            { key: "rera_original_completion_date", label: "Original completion", value: "2025-01-01", learned_at: "2026-08-10" },
            { key: "rera_completion_date", label: "Current completion", value: "2027-01-01", learned_at: "2026-08-10" },
            { key: "rera_delay_months", label: "Schedule movement", value: "24 months", learned_at: "2026-08-10" },
          ],
        },
      ],
      complaints: [{
        scope: "promoter",
        total: 8,
        open: 1,
        disposed: 7,
        rows_parsed: 8,
        status_counts_complete: true,
        theme_counts: {},
      }],
      documents: [],
    },
    ...overrides,
  };
}

test("RERA adaptive model keeps registrations phase-specific", () => {
  const secondClaim = {
    ...CLAIM,
    claim_id: "claim:two",
    subject: { entity_id: "registration:two", entity_type: "registration" },
    value: { type: "text" as const, data: "PRM/KA/RERA/TWO" },
  };
  const report = reportFixture({
    evidence: {
      ...EVIDENCE,
      registration_ids: ["registration:one", "registration:two"],
      entities: [
        { entity_id: "registration:one", entity_type: "registration", label: "Phase 1" },
        { entity_id: "registration:two", entity_type: "registration", label: "Tower B" },
      ],
      claims: [
        CLAIM,
        { ...CLAIM, claim_id: "completion:one", predicate: "proposed_completion_date", value: { type: "date", data: "2026-01-01" } },
        secondClaim,
        { ...secondClaim, claim_id: "completion:two", predicate: "proposed_completion_date", value: { type: "date", data: "2028-01-01" } },
      ],
    },
  });

  const model = buildReraReportViewModel(report, new Date("2026-08-24"));
  assert.deepEqual(model.registrations.map(({ scope, number, completion }) => ({ scope, number, completion })), [
    { scope: "Phase 1", number: "PRM/KA/RERA/TEST", completion: "2026-01-01" },
    { scope: "Tower B", number: "PRM/KA/RERA/TWO", completion: "2028-01-01" },
  ]);
  assert.equal(model.summary[1].value, "Not in record");
  assert.deepEqual(model.delivery, []);
});

test("RERA adaptive model retains useful fallback data without claiming a canonical match", () => {
  const report = reportFixture({
    availability: "partial",
    evidence: { ...EVIDENCE, registration_ids: [], claims: [], generated_at: "2026-08-10" },
  });
  const model = buildReraReportViewModel(report, new Date("2026-08-24"));

  assert.equal(model.state, "partial");
  assert.equal(model.registrations.length, 1);
  assert.equal(model.registrations[0].number, "PRM/KA/ONE");
  assert.equal(model.registrations[0].state, "partial");
  assert.equal(model.summary[0].state, "partial");
  assert.equal(model.coverage.find((item) => item.id === "completion_certificate")?.state, "not_applicable");
});

test("RERA adaptive model exposes missing, stale, and conflicting states", () => {
  const missing = buildReraReportViewModel(reportFixture({
    availability: "unavailable",
    evidence: { ...EVIDENCE, registration_ids: [], claims: [] },
    buyer_report: undefined,
  }));
  assert.equal(missing.state, "missing");
  assert.equal(missing.summary.find((item) => item.id === "registrations")?.state, "missing");

  const stale = buildReraReportViewModel(reportFixture({
    evidence: {
      ...EVIDENCE,
      regulatory_coverage: [{ source: "K-RERA", checked_at: "2025-01-01", status: "stale" }],
    },
  }));
  assert.equal(stale.state, "stale");

  const conflicting = buildReraReportViewModel(reportFixture({
    evidence: {
      ...EVIDENCE,
      regulatory_coverage: [{ source: "K-RERA", checked_at: "2026-08-10", status: "conflicting" }],
    },
  }));
  assert.equal(conflicting.state, "conflicting");
  assert.equal(conflicting.summary.find((item) => item.id === "completion")?.state, "available");
});
