import assert from "node:assert/strict";
import test from "node:test";
import {
  claimValueText,
  displayFactsForSection,
  httpUrl,
  sectionHasEvidence,
  selectorMatches,
} from "../src/lib/reraReportView.ts";
import type {
  ReraEvidenceClaim,
  ReraEvidenceProjection,
  ReraReportSurfaceSection,
} from "../src/lib/types.ts";

const CLAIM: ReraEvidenceClaim = {
  claim_id: "claim:one",
  subject: { entity_id: "registration:one", entity_type: "registration" },
  predicate: "official_registration_number",
  value: { type: "text", data: "PRM/KA/RERA/TEST" },
  assertion_mode: "registry_record",
  source_trust: "primary_authority",
  extraction_confidence: 0.95,
  validation_state: "accepted",
  visibility: "public",
  evidence: [{
    source_record_id: "record:one",
    receipt_id: "receipt:one",
    capture_id: "capture:one",
    locator: "listing[0]",
    parser_version: "fixture.v1",
  }],
};

const EVIDENCE: ReraEvidenceProjection = {
  schema_version: "rera_evidence_projection.v1",
  property_id: "property:one",
  bundle_id: "bundle:one",
  generated_at: "2026-08-10T00:00:00Z",
  registration_ids: ["registration:one"],
  entities: [],
  claims: [CLAIM],
  events: [],
  series: [],
  discrepancies: [],
  coverage: [],
  source_index: [],
};

const SECTION: ReraReportSurfaceSection = {
  id: "registration",
  title: "Official registration record",
  renderer: "fact_list",
  selectors: [{ key: "claim:official_registration_number", label: "Registration number" }],
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

test("RERA values retain units without deriving per-home carpet area", () => {
  assert.equal(claimValueText({ type: "number", data: 53728 }, "square_metres"), "53,728 m²");
  assert.equal(claimValueText({ type: "boolean", data: false }), "No");
  assert.equal(httpUrl("javascript:alert(1)"), null);
});
