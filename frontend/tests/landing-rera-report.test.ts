import assert from "node:assert/strict";
import test from "node:test";
import { getFixtureResponse } from "../src/lib/dev-fixtures.ts";
import type { PropertyDetailResponse, ReraEvidenceReportResponse } from "../src/lib/types.ts";

const PROPERTY_ID = "fixture-samadhura-capitol-3bhk";
const REGISTRATION = "PRM/KA/RERA/1251/446/PR/051024/007125";

test("landing review uses the RERA response without synthetic plan previews", () => {
  const detail = getFixtureResponse(`/api/properties/${PROPERTY_ID}`) as PropertyDetailResponse;
  const report = getFixtureResponse(`/api/properties/${PROPERTY_ID}/rera`) as ReraEvidenceReportResponse;

  assert.equal(detail.rera?.registration_number, REGISTRATION);
  assert.equal(detail.rera?.total_units, 405);
  assert.equal(detail.plans, undefined);
  assert.deepEqual(report.evidence.registration_ids, [REGISTRATION]);
  assert.deepEqual(report.evidence.series, []);
  assert.equal(report.buyer_report?.registry_url, "https://rera.karnataka.gov.in/");
  assert.equal(detail.map_context?.home.name, "Samadhura Capitol Residences");
  assert.equal(detail.map_context?.places[0]?.name, "Kadugodi Tree Park Metro");
  assert.equal(detail.map_context?.proof_focus?.entityId, "place:kadugodi-tree-park-metro");
});
