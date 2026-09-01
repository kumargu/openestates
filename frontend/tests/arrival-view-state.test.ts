import assert from "node:assert/strict";
import test from "node:test";

import {
  arrivalGateDistanceLabel,
  arrivalMissingState,
  arrivalSearchSocietiesForView,
  arrivalViewOptions,
  societyPlaybackAction,
} from "../src/lib/arrivalViewState.ts";
import type { ArrivalSearchSociety } from "../src/lib/types.ts";

test("arrival tabs retain a configured empty road while Metro remains optional", () => {
  assert.deepEqual(arrivalViewOptions({
    hasApproachLayer: true,
    hasMetroEvidence: false,
  }), [
    { id: "society", label: "Society" },
    { id: "approach", label: "Approach road" },
  ]);
});

test("arrival missing states are concise and specific to the active scene", () => {
  const states = {
    hasApproachRoad: true,
    hasBoundary: true,
    hasEntrance: true,
    missingApproachRoadState: "Approach road not mapped",
    missingBoundaryState: "Society boundary not mapped",
    missingEntranceState: "Entrance not mapped",
  };

  assert.equal(arrivalMissingState("society", { ...states, hasBoundary: false }),
    "Society boundary not mapped");
  assert.equal(arrivalMissingState("society", { ...states, hasEntrance: false }),
    "Entrance not mapped");
  assert.equal(arrivalMissingState("approach", { ...states, hasApproachRoad: false }),
    "Approach road not mapped");
  assert.equal(arrivalMissingState("metro", { ...states, hasEntrance: false }), null);
  assert.equal(arrivalMissingState("society", states), null);
});

test("search-derived societies stay isolated to the Society scene", () => {
  const societies = [{
    href: "/property/property%3Atwo",
    propertyId: "property:two",
    societyId: "society:two",
    preview: { area: "Whitefield", bhk: 3, price: 20_000_000, title: "Two" },
    home: { latitude: 12.98, longitude: 77.74, name: "Two" },
  }] satisfies ArrivalSearchSociety[];

  assert.equal(arrivalSearchSocietiesForView("society", societies), societies);
  assert.deepEqual(arrivalSearchSocietiesForView("approach", societies), []);
  assert.deepEqual(arrivalSearchSocietiesForView("metro", societies), []);
});

test("fallback film omits gate language without a mapped entrance", () => {
  assert.equal(arrivalGateDistanceLabel(0, null), undefined);
  assert.equal(arrivalGateDistanceLabel(42, null), undefined);
  assert.equal(arrivalGateDistanceLabel(0, "verified"), "At the entrance");
  assert.equal(arrivalGateDistanceLabel(42, "verified"), "42 m from entrance");
  assert.equal(arrivalGateDistanceLabel(0, "inferred"), "At the likely entrance");
  assert.equal(arrivalGateDistanceLabel(42, "inferred"), "42 m from likely entrance");
});

test("Society playback exposes pause, resume, and explicit replay actions", () => {
  assert.equal(societyPlaybackAction("preparing", true), "pause");
  assert.equal(societyPlaybackAction("revealing", true), "pause");
  assert.equal(societyPlaybackAction("paused", true), "resume");
  assert.equal(societyPlaybackAction("settled", false), "play");
  assert.equal(societyPlaybackAction("settled", true), "play");
});
