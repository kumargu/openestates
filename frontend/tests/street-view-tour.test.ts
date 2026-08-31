import assert from "node:assert/strict";
import test from "node:test";

import {
  buildStreetViewSchedule,
  easedHeadingSteps,
  resolveStreetViewSequence,
  shortestHeadingDelta,
  type StreetViewFrame,
  type StreetViewResolution,
} from "../src/lib/streetViewTour.ts";
import type { MapLayerExperience } from "../src/lib/types.ts";

const experience: MapLayerExperience = {
  kind: "street_view_tour",
  waypointSpacingM: 50,
  overviewDwellMs: 1_800,
  dwellMs: 2_500,
  anchorPitch: 6,
  cameraAltitudeM: 25,
  cameraRangeM: 145,
  cameraTilt: 80,
  cameraFov: 38,
  streetViewZoom: 0.8,
  transitionMs: 4_200,
  targetDurationMs: 28_000,
  minimumDurationMs: 24_000,
  maximumDurationMs: 32_000,
  minimumFrameDwellMs: 700,
  entranceDwellMs: 3_000,
  maximumPanoramaGapM: 140,
};

function frame(index: number, heading = 15): StreetViewFrame {
  return {
    links: [],
    pano: `pano-${index}`,
    waypoint: {
      latitude: 12.98 + index * 0.0001,
      longitude: 77.74,
      heading,
      offsetM: index * 50,
    },
  };
}

test("road film targets 28 seconds and preserves endpoints, entrance, and continuation", () => {
  const frames = Array.from({ length: 36 }, (_, index) => frame(index));
  const entrance = { latitude: frames[18].waypoint.latitude, longitude: 77.74 };
  const schedule = buildStreetViewSchedule(frames, experience, entrance);

  assert.equal(schedule.durationMs, 28_000);
  assert.ok(schedule.durationMs >= experience.minimumDurationMs!);
  assert.ok(schedule.durationMs <= experience.maximumDurationMs!);
  assert.equal(schedule.entries[0].frame.pano, "pano-0");
  assert.equal(schedule.entries.at(-1)?.frame.pano, "pano-35");
  assert.equal(schedule.entries[schedule.entranceIndex!].frame.pano, "pano-18");
  assert.equal(schedule.entries[schedule.entranceIndex! + 1].frame.pano, "pano-19");
  assert.ok(schedule.entries.length < frames.length);
});

test("road film keeps meaningful curves while downsampling ordinary frames", () => {
  const frames = Array.from({ length: 40 }, (_, index) =>
    frame(index, index >= 20 ? 85 : 15));
  const schedule = buildStreetViewSchedule(frames, experience);

  assert.ok(schedule.entries.some((entry) => entry.frame.pano === "pano-19"));
  assert.ok(schedule.entries.some((entry) => entry.frame.pano === "pano-20"));
  assert.equal(schedule.entries.some((entry) => entry.lookAtEntrance), false);
});

test("road film clamps misconfigured targets to the runtime duration bounds", () => {
  const frames = Array.from({ length: 30 }, (_, index) => frame(index));
  const tooShort = buildStreetViewSchedule(frames, {
    ...experience,
    targetDurationMs: 10_000,
  });
  const tooLong = buildStreetViewSchedule(frames, {
    ...experience,
    targetDurationMs: 40_000,
  });

  assert.equal(tooShort.durationMs, 24_000);
  assert.equal(tooLong.durationMs, 32_000);
});

test("panorama gaps are explicit and material gaps stop the sequence", () => {
  const resolutions = Array.from({ length: 8 }, (_, index) => ({
    waypoint: frame(index).waypoint,
    frame: index === 3 ? null : frame(index),
  } satisfies StreetViewResolution));
  const shortGap = resolveStreetViewSequence(resolutions, 140);
  assert.equal(shortGap.skippedShortGap, true);
  assert.equal(shortGap.endedEarly, false);
  assert.equal(shortGap.frames.at(-1)?.pano, "pano-7");

  const materialResolutions = resolutions.map((resolution, index) =>
    index >= 3 && index <= 5 ? { ...resolution, frame: null } : resolution);
  const materialGap = resolveStreetViewSequence(materialResolutions, 140);
  assert.equal(materialGap.endedEarly, true);
  assert.equal(materialGap.frames.at(-1)?.pano, "pano-2");
});

test("a leading panorama gap never silently teleports into the corridor", () => {
  const frames = Array.from({ length: 5 }, (_, index) => frame(index));
  const shortLeadingGap = frames.map((item, index) => ({
    waypoint: item.waypoint,
    frame: index === 0 ? null : item,
  }));
  const materialLeadingGap = frames.map((item, index) => ({
    waypoint: item.waypoint,
    frame: index < 3 ? null : item,
  }));

  assert.deepEqual(resolveStreetViewSequence(shortLeadingGap, 75), {
    endedEarly: false,
    frames: frames.slice(1),
    skippedShortGap: true,
  });
  assert.deepEqual(resolveStreetViewSequence(materialLeadingGap, 75), {
    endedEarly: true,
    frames: [],
    skippedShortGap: false,
  });
});

test("heading interpolation chooses the shortest turn", () => {
  assert.equal(shortestHeadingDelta(350, 10), 20);
  assert.equal(shortestHeadingDelta(10, 350), -20);
  assert.deepEqual(easedHeadingSteps(350, 10, 2), [0, 10]);
});
