import assert from "node:assert/strict";
import test from "node:test";
import {
  claimValueText,
  displayFactsForSection,
  httpUrl,
  orderReraDocuments,
  orderReraRegulatoryEvents,
  previewReraRegulatoryEvents,
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
  assert.equal(claimValueText({ type: "boolean", data: false }), "No");
  assert.equal(httpUrl("javascript:alert(1)"), null);
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
