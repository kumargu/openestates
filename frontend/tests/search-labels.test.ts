import assert from "node:assert/strict";
import test from "node:test";
import { searchResultReasonLabels, splitLabelParts } from "../src/lib/search.ts";

test("splitLabelParts keeps commas inside parentheses", () => {
  assert.deepEqual(
    splitLabelParts("Near Whitefield (Itpl, Whitefield)"),
    ["Near Whitefield (Itpl, Whitefield)"],
  );
  assert.deepEqual(
    splitLabelParts("Quiet, Near Whitefield (Itpl, Whitefield)"),
    ["Quiet", "Near Whitefield (Itpl, Whitefield)"],
  );
});

test("place chips do not split ITPL out of Whitefield", () => {
  const labels = searchResultReasonLabels({
    title: "2 BHK in Godrej Splendour",
    area: "itpl, Whitefield",
    society_name: "Godrej Splendour",
    builder_name: "Godrej",
    match_reason: "Near Whitefield (Itpl, Whitefield)",
  });
  assert.equal(labels.some((label) => label.includes("Whitefield)") || label.endsWith("(Itpl")), false);
  assert.ok(labels.every((label) => !label.includes("(") || label.includes(")")));
});

test("a near-area chip is omitted when the card already names that place", () => {
  const labels = searchResultReasonLabels({
    title: "2 BHK in Godrej Splendour",
    area: "itpl, Whitefield",
    society_name: "Godrej Splendour",
    builder_name: "Godrej",
    match_reason: "Near Whitefield (Itpl, Whitefield)",
  });
  assert.deepEqual(labels, []);
});
