import type { ArrivalPlaybackState } from "./arrivalPlayback.ts";
import type { ArrivalSearchSociety } from "./types.ts";

const EMPTY_SEARCH_SOCIETIES: ArrivalSearchSociety[] = [];

export type ArrivalView = "society" | "metro" | "approach";

export type ArrivalViewOption = {
  id: ArrivalView;
  label: string;
};

export function arrivalViewOptions({
  approachLabel = "Approach road",
  hasApproachLayer,
  hasMetroEvidence,
  metroLabel = "Metro",
}: {
  approachLabel?: string;
  hasApproachLayer: boolean;
  hasMetroEvidence: boolean;
  metroLabel?: string;
}): ArrivalViewOption[] {
  const options: ArrivalViewOption[] = [{ id: "society", label: "Society" }];
  if (hasMetroEvidence) options.push({ id: "metro", label: metroLabel });
  if (hasApproachLayer) options.push({ id: "approach", label: approachLabel });
  return options;
}

export function arrivalMissingState(
  view: ArrivalView,
  {
    hasApproachRoad,
    hasBoundary,
    hasEntrance,
    missingApproachRoadState,
    missingBoundaryState,
    missingEntranceState,
  }: {
    hasApproachRoad: boolean;
    hasBoundary: boolean;
    hasEntrance: boolean;
    missingApproachRoadState?: string;
    missingBoundaryState?: string;
    missingEntranceState?: string;
  },
): string | null {
  if (view === "approach") return hasApproachRoad ? null : missingApproachRoadState ?? null;
  if (view !== "society") return null;
  if (!hasBoundary) return missingBoundaryState ?? null;
  if (!hasEntrance) return missingEntranceState ?? null;
  return null;
}

export function arrivalSearchSocietiesForView(
  view: ArrivalView,
  societies: ArrivalSearchSociety[],
): ArrivalSearchSociety[] {
  return view === "society" ? societies : EMPTY_SEARCH_SOCIETIES;
}

export type SocietyPlaybackAction = "pause" | "resume" | "play";

export function arrivalGateDistanceLabel(
  distanceFromGateM: number | undefined,
  entranceStatus: "verified" | "inferred" | null,
): string | undefined {
  if (!entranceStatus || distanceFromGateM === undefined) return undefined;
  const entrance = entranceStatus === "inferred" ? "likely entrance" : "entrance";
  if (distanceFromGateM === 0) return `At the ${entrance}`;
  return `${Math.round(distanceFromGateM)} m from ${entrance}`;
}

export function societyPlaybackAction(
  state: ArrivalPlaybackState,
  _autoPlay: boolean,
): SocietyPlaybackAction | null {
  if (state === "preparing" || state === "revealing") return "pause";
  if (state === "paused") return "resume";
  if (state === "settled") return "play";
  return null;
}
