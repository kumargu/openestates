import assert from "node:assert/strict";
import test from "node:test";
import {
  buildCompareProjection,
  formatCompareCell,
} from "../src/lib/compareProjection.ts";
import {
  notebookNoteFacets,
  type DecisionFacet,
  type DecisionState,
} from "../src/lib/decisionFacets.ts";
import type { NotebookNote } from "../src/lib/notebook.ts";

function facet(input: {
  propertyId: string;
  topic: string;
  label: string;
  value?: string | number;
  state?: DecisionState;
  rank?: number;
  unit?: string;
}): DecisionFacet {
  return {
    id: `${input.propertyId}:${input.topic}`,
    propertyId: input.propertyId,
    scope: "property",
    topic: input.topic,
    origin: "canonical_fact",
    label: input.label,
    value: input.value,
    unit: input.unit,
    state: input.state ?? "known",
    sourceRef: { surface: "property", recordId: `${input.propertyId}:${input.topic}` },
    compare: { group: "baseline", rank: input.rank ?? 10 },
  };
}

test("compare projection handles zero and one selected home without inventing differences", () => {
  assert.deepEqual(buildCompareProjection([], []), { differences: [], evidence: [] });

  const one = buildCompareProjection(["home-1"], [
    facet({ propertyId: "home-1", topic: "price", label: "Price", value: 20_000_000, unit: "INR" }),
  ]);

  assert.equal(one.evidence.length, 1);
  assert.equal(one.differences.length, 0);
  assert.equal(formatCompareCell(one.evidence[0].cells[0]), "₹2 Cr");
});

test("compare projection begins with at most five material differences for two or four homes", () => {
  const propertyIds = ["home-1", "home-2", "home-3", "home-4"];
  const facets = propertyIds.flatMap((propertyId, index) => [
    facet({ propertyId, topic: "price", label: "Price", value: 10_000_000 + index * 1_000_000, rank: 10 }),
    facet({ propertyId, topic: "bhk", label: "Configuration", value: index < 2 ? 3 : 4, rank: 20 }),
    facet({ propertyId, topic: "area", label: "Area", value: 1400 + index * 100, rank: 30 }),
    facet({ propertyId, topic: "status", label: "Home state", value: index % 2 ? "Delivered" : "Construction", rank: 40 }),
    facet({ propertyId, topic: "water", label: "Water", value: index % 2 ? "Low stress" : "Seasonal stress", rank: 50 }),
    facet({ propertyId, topic: "school", label: "School distance", value: 1 + index, rank: 60 }),
  ]);

  const two = buildCompareProjection(propertyIds.slice(0, 2), facets);
  const four = buildCompareProjection(propertyIds, facets);

  assert.equal(two.differences.length, 5);
  assert.equal(four.differences.length, 5);
  assert.deepEqual(four.differences.map((row) => row.label), [
    "Price",
    "Configuration",
    "Area",
    "Home state",
    "Water",
  ]);
  assert.equal(four.differences[0].numericDelta, 3_000_000);
  assert.equal(four.differences.every((row) => row.cells.length === 4), true);
});

test("compare projection keeps conflicting, unknown, and not-evaluated states distinct", () => {
  const projection = buildCompareProjection(["home-1", "home-2", "home-3", "home-4"], [
    facet({ propertyId: "home-1", topic: "status", label: "Home state", value: "Delivered" }),
    facet({ propertyId: "home-2", topic: "status", label: "Home state", state: "conflicting" }),
    facet({ propertyId: "home-3", topic: "status", label: "Home state", state: "unknown" }),
  ]);
  const row = projection.differences[0];

  assert.equal(row.contrast, "conflicting");
  assert.deepEqual(row.cells.map((cell) => cell.state), [
    "known",
    "conflicting",
    "unknown",
    "not_evaluated",
  ]);
  assert.deepEqual(row.cells.map(formatCompareCell), [
    "Delivered",
    "Conflicting",
    "Unknown",
    "Not evaluated",
  ]);
  assert.equal(row.cells[0].receipts[0].recordId, "home-1:status");
});

test("identical missing coverage stays in evidence without posing as a material difference", () => {
  const projection = buildCompareProjection(["home-1", "home-2"], [
    facet({ propertyId: "home-1", topic: "price", label: "Price", state: "unknown" }),
    facet({ propertyId: "home-2", topic: "price", label: "Price", state: "unknown" }),
  ]);

  assert.equal(projection.evidence[0].contrast, "same");
  assert.equal(projection.differences.length, 0);
});

test("multiple schools remain separate evidence rows without borrowing another entity label", () => {
  const school = (
    propertyId: string,
    recordId: string,
    label: string,
    distanceKm: number,
  ): DecisionFacet => ({
    id: `${propertyId}:${recordId}`,
    propertyId,
    scope: "society",
    topic: "schools",
    origin: "map_fact",
    label,
    value: distanceKm,
    unit: "KM",
    state: "known",
    sourceRef: { surface: "map", recordId },
    compare: { group: "map_schools", rank: 200 },
  });
  const projection = buildCompareProjection(["home-1", "home-2"], [
    school("home-1", "school-alpha", "Alpha School", 0.5),
    school("home-1", "school-gamma", "Gamma School", 0.8),
    school("home-1", "school-common", "Common School", 1.1),
    school("home-2", "school-beta", "Beta School", 2.2),
    school("home-2", "school-delta", "Delta School", 3.1),
    school("home-2", "school-common", "Common School", 1.7),
  ]);
  const expectedLabels = new Map([
    ["school-alpha", "Alpha School"],
    ["school-beta", "Beta School"],
    ["school-common", "Common School"],
    ["school-delta", "Delta School"],
    ["school-gamma", "Gamma School"],
  ]);

  assert.deepEqual(
    projection.evidence.map((row) => row.label).sort(),
    [...expectedLabels.values()].sort(),
  );
  assert.equal(projection.evidence.length, 5);
  for (const row of projection.evidence) {
    const knownCells = row.cells.filter((cell) => cell.state === "known");
    const recordIds = knownCells.map((cell) => cell.receipts[0]?.recordId);
    assert.equal(new Set(recordIds).size, 1);
    assert.equal(row.label, expectedLabels.get(recordIds[0] ?? ""));
    assert.equal(knownCells.length, recordIds[0] === "school-common" ? 2 : 1);
  }
});

test("multiple notebook notes stay independently visible while initial differences remain capped", () => {
  const note = (
    propertyId: string,
    recordId: string,
    label: string,
  ): DecisionFacet => ({
    id: `${propertyId}:${recordId}`,
    propertyId,
    scope: "property",
    topic: "note",
    origin: "user_note",
    label,
    value: label,
    state: "known",
    sourceRef: { surface: "notebook", recordId },
    compare: { group: "access_notes", rank: 100 },
  });
  const notes = [
    note("home-1", "note-1", "Quiet after 8pm"),
    note("home-1", "note-2", "School traffic at pickup"),
    note("home-1", "note-3", "Visitor parking felt tight"),
    note("home-2", "note-4", "Approach road is wider"),
    note("home-2", "note-5", "Metro walk felt exposed"),
    note("home-2", "note-6", "Good evening light"),
  ];
  const projection = buildCompareProjection(["home-1", "home-2"], notes);

  assert.equal(projection.evidence.length, 6);
  assert.deepEqual(
    new Set(projection.evidence.map((row) => row.label)),
    new Set(notes.map((item) => item.label)),
  );
  assert.equal(projection.differences.length, 5);
  assert.equal(projection.evidence.every((row) => (
    row.cells.filter((cell) => cell.state === "known").length === 1
  )), true);
});

test("record-backed projection emits one row per notebook note and aligns the same map record", () => {
  const notes: NotebookNote[] = [
    {
      id: "note-school-a",
      propertyId: "home-1",
      title: "School traffic at pickup",
      detail: "Visit note",
      kind: "handwritten",
      catalogKey: "hand:school-traffic",
      labels: ["schools"],
      createdAt: 1,
    },
    {
      id: "note-school-b",
      propertyId: "home-2",
      title: "Quiet after 8pm",
      detail: "Visit note",
      kind: "handwritten",
      catalogKey: "hand:quiet-evening",
      labels: ["schools"],
      createdAt: 2,
    },
  ];
  const sharedSchool = (propertyId: string, distanceKm: number): DecisionFacet => ({
    id: `${propertyId}:shared-school`,
    propertyId,
    scope: "society",
    topic: "schools",
    origin: "map_fact",
    label: "Common School",
    value: distanceKm,
    unit: "KM",
    state: "known",
    sourceRef: { surface: "map", recordId: "school-common" },
    compare: { group: "map_schools", rank: 200 },
  });
  const projection = buildCompareProjection(["home-1", "home-2"], [
    ...notebookNoteFacets(notes),
    sharedSchool("home-1", 1.1),
    sharedSchool("home-2", 1.7),
  ]);

  assert.equal(projection.evidence.length, 3);
  for (const note of notes) {
    assert.equal(
      projection.evidence.filter((row) => row.cells.some((cell) =>
        cell.receipts.some((receipt) => receipt.recordId === note.id)
      )).length,
      1,
    );
  }
  const schoolRow = projection.evidence.find((row) =>
    row.cells.some((cell) => cell.receipts.some((receipt) => receipt.recordId === "school-common"))
  );
  assert.equal(schoolRow?.cells.filter((cell) => cell.state === "known").length, 2);
  assert.equal(schoolRow?.label, "Common School");
});
