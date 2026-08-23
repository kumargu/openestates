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
import "../../styles/property-arrival.css";

type Props = {
  propertyId: string;
  title: string;
  frames: StoryArrivalFrame[];
  playback?: StoryScenePlayback;
};

function distanceLabel(distanceFromGateM?: number): string | undefined {
  if (distanceFromGateM === undefined) return undefined;
  if (distanceFromGateM === 0) return "At the gate";
  return `${Math.round(distanceFromGateM)} m from gate`;
}

function frameMeta(frame: StoryArrivalFrame): string | undefined {
  const lifecycle = frame.lifecycle === "current"
    ? "Current street view"
    : frame.lifecycle === "proposed"
      ? "Proposed render"
      : undefined;
  return [lifecycle, distanceLabel(frame.distanceFromGateM)]
    .filter(Boolean)
    .join(" · ") || undefined;
}

export function PropertyArrivalFilm({
  propertyId,
  title,
  frames,
  playback,
}: Props) {
  const frameKey = frames.map((frame) => frame.id).join("|");
  const [availability, setAvailability] = useState({
    frameKey,
    count: frames.length,
  });
  const usableFrameCount = availability.frameKey === frameKey
    ? availability.count
    : frames.length;
  const syncAvailability = useCallback((frameIds: string[]) => {
    setAvailability((current) =>
      current.frameKey === frameKey && current.count === frameIds.length
        ? current
        : { frameKey, count: frameIds.length });
  }, [frameKey]);
  const filmstripFrames = useMemo<PropertyFilmstripFrame[]>(
    () =>
      frames.map((frame, index) => ({
        id: frame.id,
        url: frame.url,
        label: frame.label || `Approach view ${index + 1}`,
        meta: frameMeta(frame),
        lifecycle: frame.lifecycle,
        sourceUrl: frame.sourceUrl,
      })),
    [frames],
  );

  if (filmstripFrames.length === 0 || usableFrameCount === 0) return null;

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
      <PropertyFilmstrip
        ariaLabel={`Approach to ${title}`}
        frames={filmstripFrames}
        motionSeed={stableStoryHash(`${propertyId}:arrival`)}
        playback={playback}
          showPlaybackControl
        onUsableFramesChange={syncAvailability}
      />
    </section>
  );
}
