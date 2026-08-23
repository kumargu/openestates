import {
  useCallback,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import type {
  PropertyStoryModel,
  StoryMediaFrame,
} from "../../lib/propertyStory.ts";
import {
  PropertyFilmstrip,
  type PropertyFilmstripFrame,
  type StoryPlaybackSpeed,
  type StoryScenePlayback,
} from "./PropertyFilmstrip.tsx";
import { PropertyPhotoWalker } from "./PropertyPhotoWalker.tsx";

export type { StoryPlaybackSpeed, StoryScenePlayback };

type Props = {
  story: PropertyStoryModel;
  actions?: ReactNode;
  playback?: StoryScenePlayback;
  sectionId?: string;
  showIdentity?: boolean;
  cinematicMotion?: boolean;
};

type IdentityProps = {
  story: PropertyStoryModel;
  actions?: ReactNode;
  showFacts?: boolean;
};

export function PropertySceneFacts({
  story,
  pageScoped = false,
}: {
  story: PropertyStoryModel;
  pageScoped?: boolean;
}) {
  const facts = story.identity.facts.map((fact) => (
    <span key={fact.key}>{fact.value}</span>
  ));

  if (pageScoped) {
    return (
      <div
        id="property-summary"
        className="property-story-sticky-facts"
        aria-label="Home summary"
        tabIndex={-1}
      >
        <div className="property-story-sticky-facts__inner">{facts}</div>
      </div>
    );
  }

  return (
    <div className="property-scene__facts" aria-label="Home summary">
      {facts}
    </div>
  );
}

export function PropertySceneIdentity({
  story,
  actions,
  showFacts = true,
}: IdentityProps) {
  return (
    <>
      <div className="property-scene__identity">
        <div className="property-scene__identity-copy">
          <p>{story.identity.location}</p>
          <h1 id="property-scene-title">{story.identity.title}</h1>
        </div>
        {actions && (
          <div className="property-scene__actions" aria-label="Property actions">
            {actions}
          </div>
        )}
      </div>
      {showFacts && <PropertySceneFacts story={story} />}
    </>
  );
}

function frameLabel(frame: StoryMediaFrame, index: number): string {
  if (frame.lifecycle === "current") return "Current image";
  if (frame.lifecycle === "proposed") return "Proposed render";
  if (frame.role === "exterior" || frame.role === "building") return "Exterior";
  if (frame.role === "amenity") return "Amenity";
  if (frame.role === "neighbourhood") return "Neighbourhood";
  return `Property view ${String(index + 1).padStart(2, "0")}`;
}

function sameIds(left: string[], right: string[]): boolean {
  return left.length === right.length
    && left.every((id, index) => id === right[index]);
}

export function PropertySceneCard({
  story,
  actions,
  playback,
  sectionId,
  showIdentity = true,
  cinematicMotion = true,
}: Props) {
  const [walkerIndex, setWalkerIndex] = useState<number | null>(null);
  const [usableFrameIds, setUsableFrameIds] = useState(
    () => story.media.frames.map((frame) => frame.id),
  );
  const filmstripFrames = useMemo<PropertyFilmstripFrame[]>(
    () =>
      story.media.frames.map((frame, index) => ({
        id: frame.id,
        url: frame.url,
        label: frameLabel(frame, index),
        meta: frame.capturedAt,
        lifecycle: frame.lifecycle,
        sourceUrl: frame.sourceUrl,
        focalPoint: frame.focalPoint,
        showCaption:
          frame.lifecycle !== "unknown"
          || ["exterior", "building", "amenity", "neighbourhood"].includes(
            frame.role,
          ),
      })),
    [story.media.frames],
  );
  const usableGalleryUrls = useMemo(() => {
    const usableIds = new Set(usableFrameIds);
    const failedUrls = new Set(
      story.media.frames
        .filter((frame) => !usableIds.has(frame.id))
        .map((frame) => frame.url),
    );
    return story.media.galleryUrls.filter((url) => !failedUrls.has(url));
  }, [story.media.frames, story.media.galleryUrls, usableFrameIds]);

  const syncUsableFrames = useCallback((frameIds: string[]) => {
    setUsableFrameIds((current) =>
      sameIds(current, frameIds) ? current : frameIds);
  }, []);

  function openGallery(activeFrameId: string) {
    const activeUrl = story.media.frames.find(
      (frame) => frame.id === activeFrameId,
    )?.url;
    const activeGalleryIndex = activeUrl
      ? usableGalleryUrls.indexOf(activeUrl)
      : 0;
    setWalkerIndex(activeGalleryIndex >= 0 ? activeGalleryIndex : 0);
  }

  const hasImages = filmstripFrames.length > 0;

  return (
    <section
      id={sectionId}
      className={`property-scene${hasImages ? "" : " property-scene--empty"}`}
      aria-labelledby="property-scene-title"
    >
      {showIdentity && (
        <PropertySceneIdentity story={story} actions={actions} />
      )}

      {hasImages && (
        <PropertyFilmstrip
          ariaLabel={`${story.identity.title} property views`}
          frames={filmstripFrames}
          motionSeed={story.motionSeed}
          motionTheme={story.motionTheme}
          playback={playback}
          priority
          presentation="stage"
          cinematicMotion={cinematicMotion}
          showPlaybackControl
          galleryLabel={`All photos · ${usableGalleryUrls.length}`}
          onOpenGallery={usableGalleryUrls.length > 0 ? openGallery : undefined}
          onUsableFramesChange={syncUsableFrames}
        />
      )}

      {walkerIndex !== null && usableGalleryUrls.length > 0 && (
        <PropertyPhotoWalker
          title={story.identity.title}
          images={usableGalleryUrls}
          index={walkerIndex}
          onIndexChange={setWalkerIndex}
          onClose={() => setWalkerIndex(null)}
        />
      )}
    </section>
  );
}
