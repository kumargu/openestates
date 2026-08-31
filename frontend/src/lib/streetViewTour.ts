import type { MapLayerExperience } from "./types.ts";
import type { CorridorTourWaypoint } from "./arrivalMapProjection.ts";

export type StreetViewLink = {
  heading: number;
  pano: string;
};

export type StreetViewFrame = {
  links: StreetViewLink[];
  pano: string;
  panoramaPosition: { latitude: number; longitude: number };
  waypoint: CorridorTourWaypoint;
};

export type StreetViewResolution = {
  frame: StreetViewFrame | null;
  waypoint: CorridorTourWaypoint;
};

export type StreetViewSequence = {
  endedEarly: boolean;
  frames: StreetViewFrame[];
  skippedShortGap: boolean;
};

export type StreetViewScheduleEntry = {
  dwellMs: number;
  frame: StreetViewFrame;
  lookAtEntrance: boolean;
};

export type StreetViewSchedule = {
  durationMs: number;
  entries: StreetViewScheduleEntry[];
  entranceIndex: number | null;
};

const CURVE_THRESHOLD_DEGREES = 12;

export function normalizeHeading(heading: number): number {
  return (heading % 360 + 360) % 360;
}

export function shortestHeadingDelta(from: number, to: number): number {
  return (normalizeHeading(to) - normalizeHeading(from) + 540) % 360 - 180;
}

export function easedHeadingSteps(from: number, to: number, stepCount = 3): number[] {
  if (stepCount <= 0) return [];
  const delta = shortestHeadingDelta(from, to);
  return Array.from({ length: stepCount }, (_, index) =>
    normalizeHeading(from + delta * ((index + 1) / stepCount)));
}

function headingDistance(left: number, right: number): number {
  return Math.abs(shortestHeadingDelta(left, right));
}

export function shouldReorientStreetView(currentHeading: number, nextHeading: number): boolean {
  return headingDistance(currentHeading, nextHeading) >= CURVE_THRESHOLD_DEGREES;
}

export function streetViewPlayback(frames: StreetViewFrame[]): StreetViewFrame[] {
  if (frames.length === 0) return [];
  const sorted = frames.slice().sort((left, right) =>
    left.waypoint.offsetM - right.waypoint.offsetM);
  return sorted.filter((frame, index) =>
    index === 0 || frame.pano !== sorted[index - 1].pano);
}

export function resolveStreetViewSequence(
  resolutions: StreetViewResolution[],
  maximumGapM: number,
): StreetViewSequence {
  const ordered = resolutions.slice().sort((left, right) =>
    left.waypoint.offsetM - right.waypoint.offsetM);
  const frames: StreetViewFrame[] = [];
  let skippedShortGap = false;
  let endedEarly = false;
  let previousLoadedIndex = -1;

  for (let index = 0; index < ordered.length; index += 1) {
    const resolution = ordered[index];
    if (!resolution.frame) continue;
    if (previousLoadedIndex < 0 && index > 0) {
      const leadingGapM = Math.abs(
        resolution.waypoint.offsetM - ordered[0].waypoint.offsetM,
      );
      if (leadingGapM > maximumGapM) {
        endedEarly = true;
        break;
      }
      skippedShortGap = true;
    }
    if (previousLoadedIndex >= 0 && index > previousLoadedIndex + 1) {
      const gapM = Math.abs(
        resolution.waypoint.offsetM - ordered[previousLoadedIndex].waypoint.offsetM,
      );
      if (gapM > maximumGapM) {
        endedEarly = true;
        break;
      }
      skippedShortGap = true;
    }
    if (frames.at(-1)?.pano !== resolution.frame.pano) frames.push(resolution.frame);
    previousLoadedIndex = index;
  }

  if (previousLoadedIndex >= 0 && previousLoadedIndex < ordered.length - 1) {
    const trailingGapM = Math.abs(
      ordered.at(-1)!.waypoint.offsetM - ordered[previousLoadedIndex].waypoint.offsetM,
    );
    if (trailingGapM > maximumGapM) endedEarly = true;
    else if (trailingGapM > 0) skippedShortGap = true;
  }
  return { endedEarly, frames, skippedShortGap };
}

function distanceSquared(
  position: { latitude: number; longitude: number },
  point: { latitude: number; longitude: number },
): number {
  const longitudeScale = Math.cos((point.latitude * Math.PI) / 180);
  return (position.latitude - point.latitude) ** 2
    + ((position.longitude - point.longitude) * longitudeScale) ** 2;
}

function entranceFrameIndex(
  frames: StreetViewFrame[],
  entrance?: { latitude: number; longitude: number } | null,
): number | null {
  if (!entrance || frames.length === 0) return null;
  let nearestIndex = 0;
  for (let index = 1; index < frames.length; index += 1) {
    if (
      distanceSquared(frames[index].panoramaPosition, entrance)
      < distanceSquared(frames[nearestIndex].panoramaPosition, entrance)
    ) nearestIndex = index;
  }
  return nearestIndex;
}

function meaningfulCurveIndices(frames: StreetViewFrame[]): number[] {
  return frames.flatMap((frame, index) => {
    const previous = frames[index - 1];
    const next = frames[index + 1];
    return (
      (previous && headingDistance(previous.waypoint.heading, frame.waypoint.heading)
        >= CURVE_THRESHOLD_DEGREES)
      || (next && headingDistance(frame.waypoint.heading, next.waypoint.heading)
        >= CURVE_THRESHOLD_DEGREES)
    ) ? [index] : [];
  });
}

function downsampleIndices(frameCount: number, required: Set<number>, maximum: number): number[] {
  const selected = new Set([...required].filter((index) => index >= 0 && index < frameCount));
  while (selected.size < Math.min(maximum, frameCount)) {
    let bestIndex = -1;
    let bestDistance = -1;
    for (let index = 0; index < frameCount; index += 1) {
      if (selected.has(index)) continue;
      const distance = Math.min(...[...selected].map((value) => Math.abs(value - index)));
      if (distance > bestDistance) {
        bestDistance = distance;
        bestIndex = index;
      }
    }
    if (bestIndex < 0) break;
    selected.add(bestIndex);
  }
  return [...selected].sort((left, right) => left - right);
}

export function buildStreetViewSchedule(
  frames: StreetViewFrame[],
  experience: MapLayerExperience,
  entrance?: { latitude: number; longitude: number } | null,
): StreetViewSchedule {
  const playback = streetViewPlayback(frames);
  if (playback.length === 0) return { durationMs: 0, entries: [], entranceIndex: null };
  const configuredTargetDurationMs = experience.targetDurationMs ?? 28_000;
  const targetDurationMs = Math.min(
    experience.maximumDurationMs ?? Number.POSITIVE_INFINITY,
    Math.max(experience.minimumDurationMs ?? 0, configuredTargetDurationMs),
  );
  const overheadMs = (experience.overviewDwellMs ?? 0) + experience.transitionMs;
  const entranceDwellMs = entrance ? experience.entranceDwellMs ?? 0 : 0;
  const availableMs = Math.max(0, targetDurationMs - overheadMs - entranceDwellMs);
  const minimumFrameDwellMs = experience.minimumFrameDwellMs ?? experience.dwellMs;
  const maximumFrames = Math.max(2, Math.floor(availableMs / minimumFrameDwellMs));
  const rawEntranceIndex = entranceFrameIndex(playback, entrance);
  const required = new Set<number>([0, playback.length - 1, ...meaningfulCurveIndices(playback)]);
  if (rawEntranceIndex !== null) {
    required.add(rawEntranceIndex);
    if (rawEntranceIndex + 1 < playback.length) required.add(rawEntranceIndex + 1);
  }
  const selectedIndices = downsampleIndices(
    playback.length,
    required,
    Math.max(maximumFrames, required.size),
  );
  const ordinaryDwellMs = Math.floor(availableMs / selectedIndices.length);
  let remainderMs = availableMs - ordinaryDwellMs * selectedIndices.length;
  const selectedEntranceIndex = rawEntranceIndex === null
    ? null
    : selectedIndices.indexOf(rawEntranceIndex);
  const entries = selectedIndices.map((frameIndex, index) => {
    const extraMs = remainderMs > 0 ? 1 : 0;
    remainderMs -= extraMs;
    return {
      dwellMs: ordinaryDwellMs + extraMs
        + (index === selectedEntranceIndex ? entranceDwellMs : 0),
      frame: playback[frameIndex],
      lookAtEntrance: index === selectedEntranceIndex,
    };
  });
  return {
    durationMs: overheadMs + entries.reduce((total, entry) => total + entry.dwellMs, 0),
    entries,
    entranceIndex: selectedEntranceIndex,
  };
}

export function streetViewAnchorHeading(
  from: { latitude: number; longitude: number },
  to: { latitude: number; longitude: number },
): number {
  const latitudeScale = 110_570;
  const longitudeScale = 111_320 * Math.cos((from.latitude * Math.PI) / 180);
  const east = (to.longitude - from.longitude) * longitudeScale;
  const north = (to.latitude - from.latitude) * latitudeScale;
  return normalizeHeading(Math.atan2(east, north) * 180 / Math.PI);
}
