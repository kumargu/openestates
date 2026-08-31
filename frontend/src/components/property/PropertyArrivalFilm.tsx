import { useCallback, useMemo, useState } from "react";
import {
  stableStoryHash,
  type StoryArrivalFrame,
} from "../../lib/propertyStory.ts";
import {
  PropertyFilmstrip,
  type PropertyFilmstripFrame,
  type StoryScenePlayback,
} from "./PropertyFilmstrip.tsx";
import type { ArrivalSearchSociety, PropertyMapContext } from "../../lib/types.ts";
import {
  hasArrivalMap,
  mappedArrivalEntranceStatus,
} from "../../lib/arrivalMapProjection.ts";
import { arrivalGateDistanceLabel } from "../../lib/arrivalViewState.ts";
import { PropertyArrivalMap } from "./PropertyArrivalMap.tsx";
import "../../styles/property-arrival.css";

type Props = {
  propertyId: string;
  title: string;
  frames: StoryArrivalFrame[];
  playback?: StoryScenePlayback;
  cinematicMotion?: boolean;
  mapContext?: PropertyMapContext | null;
  searchContextSocieties?: ArrivalSearchSociety[];
};

const ARRIVAL_CLOSE_DISTANCE_M = 50;

function viewScale(
  distanceFromGateM?: number,
  entranceStatus: "verified" | "inferred" | null = null,
): PropertyFilmstripFrame["viewScale"] {
  if (!entranceStatus || distanceFromGateM === undefined) return "wide";
  return distanceFromGateM <= ARRIVAL_CLOSE_DISTANCE_M ? "close" : "wide";
}

function captureDateLabel(capturedAt?: string): string | undefined {
  if (!capturedAt) return undefined;
  const match = /^(\d{4})-(\d{2})$/.exec(capturedAt);
  if (!match) return /^\d{4}$/.test(capturedAt) ? capturedAt : undefined;
  const date = new Date(Date.UTC(Number(match[1]), Number(match[2]) - 1));
  return new Intl.DateTimeFormat("en-IN", {
    month: "short",
    year: "numeric",
    timeZone: "UTC",
  }).format(date);
}

function frameMeta(
  frame: StoryArrivalFrame,
  entranceStatus: "verified" | "inferred" | null,
): string | undefined {
  const lifecycle = frame.lifecycle === "current"
    ? frame.stripKind === "street_view_strip"
      ? "Street View"
      : "Current site photo"
    : frame.lifecycle === "proposed"
      ? "Proposed render"
      : undefined;
  return [
    lifecycle,
    captureDateLabel(frame.capturedAt),
    arrivalGateDistanceLabel(frame.distanceFromGateM, entranceStatus),
  ]
    .filter(Boolean)
    .join(" · ") || undefined;
}

function distinctArrivalFrames(frames: StoryArrivalFrame[]): StoryArrivalFrame[] {
  const seenUrls = new Set<string>();
  return frames.filter((frame) => {
    const url = frame.url.trim();
    if (seenUrls.has(url)) return false;
    seenUrls.add(url);
    return true;
  });
}

export function PropertyArrivalFilm({
  propertyId,
  title,
  frames,
  playback,
  cinematicMotion = true,
  mapContext,
  searchContextSocieties,
}: Props) {
  const distinctFrames = useMemo(() => distinctArrivalFrames(frames), [frames]);
  const frameKey = distinctFrames.map((frame) => frame.id).join("|");
  const [availability, setAvailability] = useState({
    frameKey,
    count: distinctFrames.length,
  });
  const usableFrameCount = availability.frameKey === frameKey
    ? availability.count
    : distinctFrames.length;
  const entranceStatus = mappedArrivalEntranceStatus(mapContext);
  const syncAvailability = useCallback((frameIds: string[]) => {
    setAvailability((current) =>
      current.frameKey === frameKey && current.count === frameIds.length
        ? current
        : { frameKey, count: frameIds.length });
  }, [frameKey]);
  const filmstripFrames = useMemo<PropertyFilmstripFrame[]>(
    () =>
      distinctFrames.map((frame, index) => ({
        id: frame.id,
        url: frame.url,
        label: frame.label || `Approach view ${index + 1}`,
        meta: frameMeta(frame, entranceStatus),
        lifecycle: frame.lifecycle,
        sourceUrl: frame.sourceUrl,
        viewScale: viewScale(frame.distanceFromGateM, entranceStatus),
      })),
    [distinctFrames, entranceStatus],
  );

  const mapAvailable = hasArrivalMap(mapContext);
  if (!mapAvailable && (filmstripFrames.length === 0 || usableFrameCount === 0)) return null;

  return (
    <section
      id="remote-arrival"
      className="property-arrival"
      aria-labelledby="property-arrival-title"
    >
      <header className="property-story-heading">
        <span>Arrival</span>
        <h2 id="property-arrival-title">The way in.</h2>
      </header>
      {mapAvailable && mapContext ? (
        <PropertyArrivalMap
          context={mapContext}
          searchContextSocieties={searchContextSocieties}
        />
      ) : (
        <PropertyFilmstrip
          ariaLabel={`Approach to ${title}`}
          frames={filmstripFrames}
          motionSeed={stableStoryHash(`${propertyId}:arrival`)}
          playback={playback}
          presentation="stage"
          cinematicMotion={cinematicMotion}
          cinematicPace="brisk"
          showPlaybackControl
          onUsableFramesChange={syncAvailability}
        />
      )}
    </section>
  );
}
