import assert from "node:assert/strict";
import test from "node:test";
import {
  queryWithoutBhkClause,
  searchResultsAnnouncement,
  searchResultReasonLabels,
  splitLabelParts,
} from "../src/lib/search.ts";

test("result announcements distinguish named alternatives from relaxed matches", () => {
  assert.equal(
    searchResultsAnnouncement(
      "Godrej Splendour under 1.4Cr",
      0,
      3,
      "named_society_alternatives",
    ),
    'No exact matches for "Godrej Splendour under 1.4Cr". Showing 3 alternatives.',
  );
  assert.equal(
    searchResultsAnnouncement("3BHK under 1Cr", 0, 3, null),
    'No exact matches for "3BHK under 1Cr". Showing 3 broader matches.',
  );
});

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

test("a 2 or 3 BHK match reason does not become a tile chip", () => {
  const labels = searchResultReasonLabels({
    title: "3 BHK in Prestige Lakeside Habitat",
    area: "Whitefield",
    society_name: "Prestige Lakeside Habitat",
    builder_name: "Prestige",
    match_reason: "Matches Whitefield · 2 or 3 BHK",
  });
  assert.equal(labels.some((label) => /bhk/i.test(label)), false);
});

test("queryWithoutBhkClause uses UTF-8 source spans", () => {
  const query = "₹2Cr, two or three BHK in Whitefield";
  const clause = "two or three BHK";
  const start = Buffer.byteLength(query.slice(0, query.indexOf(clause)));
  const end = start + Buffer.byteLength(clause);
  assert.equal(
    queryWithoutBhkClause(query, [{ start, end, raw_text: clause }]),
    "₹2Cr, in Whitefield",
  );
});

test("queryWithoutBhkClause removes every repeated source span", () => {
  const query = "2 BHK in Whitefield or 2 BHK in Bellandur";
  const clause = "2 BHK";
  const secondStart = Buffer.byteLength(query.slice(0, query.lastIndexOf(clause)));
  assert.equal(
    queryWithoutBhkClause(query, [
      { start: 0, end: clause.length, raw_text: clause },
      {
        start: secondStart,
        end: secondStart + clause.length,
        raw_text: clause,
      },
    ]),
    "in Whitefield or in Bellandur",
  );
});

test("queryWithoutBhkClause removes source spans for exclusions", () => {
  const query = "2 BHK in Whitefield, outside a 4 BHK";
  const include = "2 BHK";
  const exclude = "outside a 4 BHK";
  const excludeStart = Buffer.byteLength(query.slice(0, query.indexOf(exclude)));
  assert.equal(
    queryWithoutBhkClause(query, [
      { start: 0, end: include.length, raw_text: include },
      {
        start: excludeStart,
        end: excludeStart + exclude.length,
        raw_text: exclude,
      },
    ]),
    "in Whitefield",
  );
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
