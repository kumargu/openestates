import assert from "node:assert/strict";
import test from "node:test";
import {
  buyerProofCoverageLabel,
  buyerProofReceiptLabel,
} from "../src/lib/buyerProof.ts";
import type { BuyerProofReceipt } from "../src/lib/types.ts";

const receipt: BuyerProofReceipt = {
  label: "Configured buyer receipt",
  matched_value: "Configured buyer receipt (0.6 km)",
  distance_m: 600,
  requested_preference: "configured preference",
  match_status: "matched",
  source_type: "Configured source",
  evidence_confidence: 0.82,
  focus: {
    surfaceId: "configured_surface",
    layerId: "configured_layer",
    factKey: "configured_fact",
    reason: "matches configured preference",
  },
};

test("buyer proof keeps the concrete receipt separate from confidence", () => {
  assert.equal(
    buyerProofReceiptLabel(receipt),
    "Configured buyer receipt · 600 m",
  );
  assert.equal(buyerProofReceiptLabel({ ...receipt, distance_m: 2700 }), "Configured buyer receipt · 2.7 km");
  assert.equal(receipt.evidence_confidence, 0.82);
});

test("missing and conflicted coverage use calm buyer copy", () => {
  assert.equal(
    buyerProofCoverageLabel({ preference: "quiet neighborhood", status: "no_data" }),
    "Quiet neighborhood not yet verified",
  );
  assert.equal(
    buyerProofCoverageLabel({ preference: "road access", status: "conflicted" }),
    "Road access has conflicting evidence",
  );
});
