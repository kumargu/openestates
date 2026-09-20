import assert from "node:assert/strict";
import test from "node:test";
import { searchResultReasonLabels } from "../src/lib/search.ts";
import type { SearchMatchReason } from "../src/lib/types.ts";

function reason(explanation: string, showOnCard = true): SearchMatchReason {
  return { branchId: "branch-1", predicateId: explanation, explanation, showOnCard, proofToken: "signed:receipt" };
}

test("backend visibility prevents area, budget and BHK copy from repeating on cards", () => {
  assert.deepEqual(searchResultReasonLabels({ reasons: [
    reason("Whitefield", false), reason("3 BHK", false), reason("under ₹2.5Cr", false),
    reason("Nearby schools: Example School (0.4 km)"),
  ] }), ["Nearby schools: Example School (0.4 km)"]);
});

test("structured labels retain parentheses and punctuation without reinterpretation", () => {
  assert.deepEqual(searchResultReasonLabels({ reasons: [reason("Near Whitefield (ITPL, Whitefield)")] }),
    ["Near Whitefield (ITPL, Whitefield)"]);
});

test("result chips retain backend order, deduplicate and stay capped at two", () => {
  assert.deepEqual(searchResultReasonLabels({ reasons: [reason("RERA registration found"),
    reason("RERA registration found"), reason("Google 4.4 · 320 reviews"), reason("0.7 km from Hoodi Metro")] }),
    ["RERA registration found", "Google 4.4 · 320 reviews"]);
});

test("hidden source receipts and absent reasons do not create display labels", () => {
  assert.deepEqual(searchResultReasonLabels({ reasons: [reason("https://maps.google.com/example", false)] }), []);
  assert.deepEqual(searchResultReasonLabels({ reasons: [] }), []);
});
