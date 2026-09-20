import assert from "node:assert/strict";
import test from "node:test";
import {
  formatListingPrice,
  listingSatisfiesBudget,
} from "../src/lib/listing-price.ts";
import {
  initialPropertySurfaceId,
  primaryProofFocus,
  propertyProofMatch,
  propertySceneProofFocus,
} from "../src/lib/proof-focus.ts";


test("formatListingPrice shows a band when min and max differ", () => {
  assert.equal(
    formatListingPrice({
      price: 32_250_000,
      price_min: 30_000_000,
      price_max: 48_000_000,
    }),
    "₹3.0–4.8 Cr",
  );
  assert.equal(
    formatListingPrice({ price: 32_250_000 }),
    "₹3.2 Cr",
  );
  assert.equal(
    formatListingPrice({
      price: 32_250_000,
      price_min: 32_250_000,
      price_max: 32_250_000,
    }),
    "₹3.2 Cr",
  );
});

test("listingSatisfiesBudget uses overlap, not the collapsed midpoint", () => {
  const listing = {
    price: 32_250_000,
    price_min: 30_000_000,
    price_max: 48_000_000,
  };
  assert.equal(listingSatisfiesBudget(listing, null, 33_000_000), true);
  assert.equal(listingSatisfiesBudget(listing, 40_000_000, null), true);
  assert.equal(listingSatisfiesBudget(listing, null, 29_000_000), false);
  assert.equal(listingSatisfiesBudget({ price: 32_250_000 }, 40_000_000, null), false);
});

test("primaryProofFocus follows the backend card reason, not client claims or array position", () => {
  const result = {
    match_reason: "Near metro",
    reasons: [
      { branchId: "branch-1", predicateId: "metro", explanation: "Metro access", proofToken: "signed:metro", showOnCard: false },
      { branchId: "branch-1", predicateId: "hospital", explanation: "Hospital access", proofToken: "signed:hospital", showOnCard: true },
    ],
  };
  assert.equal(primaryProofFocus(result)?.proofToken, "signed:hospital");
});

test("primaryProofFocus is empty when search had no proof overlay", () => {
  assert.equal(primaryProofFocus({ reasons: [] }), undefined);
  assert.equal(primaryProofFocus({}), undefined);
});

test("property detail requests the proof focus's declared surface", () => {
  assert.equal(initialPropertySurfaceId({
    surfaceId: "flooding",
    layerId: "flooding",
    factKey: "waterlogging_risk_score",
    reason: "Matched low flooding risk",
  }), "flooding");
  assert.equal(initialPropertySurfaceId(), "around_this_home");
});

test("section proof handoff keeps the map request on its default surface", () => {
  const focus: ProofFocus = {
    surfaceId: "legal_rera",
    layerId: "legal_rera",
    factKey: "rera_status",
    destinationKind: "section",
    targetId: "official-record",
    matchedValue: "RERA registration found",
    reason: "RERA registration found",
  };
  assert.equal(initialPropertySurfaceId(focus), "around_this_home");
  assert.equal(propertySceneProofFocus(focus), undefined);
  assert.deepEqual(
    propertyProofMatch(focus, "official-record", "https://rera.example/record"),
    {
      value: "RERA registration found",
      sourceUrl: "https://rera.example/record",
    },
  );
});


test("compact exact cards retain proof navigation when facts need no duplicate label", () => {
  assert.equal(primaryProofFocus({ reasons: [{ branchId: "branch-1", predicateId: "budget", explanation: "Under budget", showOnCard: false, proofToken: "signed:budget" }] })?.proofToken, "signed:budget");
});
