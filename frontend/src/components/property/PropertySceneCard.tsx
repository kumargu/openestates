import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type ReactNode,
} from "react";
import { ImageWithFallback } from "../ImageWithFallback.tsx";
import {
  nextStoryFrameIndex,
  selectStoryMotionTheme,
  shouldAutoAdvanceStory,
  STORY_MOTION_REGISTRY,
  type PropertyStoryModel,
  type StoryMediaFrame,
  type StoryMotionTheme,
} from "../../lib/propertyStory.ts";
import { PropertyPhotoWalker } from "./PropertyPhotoWalker.tsx";

type Props = {
  story: PropertyStoryModel;
  actions?: ReactNode;
  playback?: StoryScenePlayback;
};

export type StoryPlaybackSpeed = 0.5 | 1 | 2;

export type StoryScenePlayback = {
  activeIndex?: number;
  playing?: boolean;
  speed?: StoryPlaybackSpeed;
  reducedMotion?: boolean;
  visibility?: "auto" | "visible" | "hidden" | "offscreen";
  onActiveIndexChange?: (index: number) => void;
  onPlayingChange?: (playing: boolean) => void;
};

export function PropertySceneCard({
  story,
  actions,
  playback,
}: Props) {
  const rootRef = useRef<HTMLElement>(null);
  const [internalActive, setInternalActive] = useState(0);
  const [internalPlaying, setInternalPlaying] = useState(true);
  const [prefersReducedMotion, setPrefersReducedMotion] = useState(false);
  const [isIntersecting, setIsIntersecting] = useState(true);
  const [documentVisible, setDocumentVisible] = useState(
    () => document.visibilityState !== "hidden",
  );
  const [failedFrameIds, setFailedFrameIds] = useState<Set<string>>(
    () => new Set(),
  );
  const [readyFrameIds, setReadyFrameIds] = useState<Set<string>>(
    () => new Set(),
  );
  const [previousFrame, setPreviousFrame] =
    useState<StoryMediaFrame | null>(null);
  const [walkerIndex, setWalkerIndex] = useState<number | null>(null);

  const frames = useMemo(
    () =>
      story.media.frames.filter((frame) => !failedFrameIds.has(frame.id)),
    [failedFrameIds, story.media.frames],
  );
  const usableGalleryUrls = useMemo(() => {
    const failedUrls = new Set(
      story.media.frames
        .filter((frame) => failedFrameIds.has(frame.id))
        .map((frame) => frame.url),
    );
    return story.media.galleryUrls.filter((url) => !failedUrls.has(url));
  }, [failedFrameIds, story.media.frames, story.media.galleryUrls]);
  const active = playback?.activeIndex ?? internalActive;
  const safeActive =
    frames.length > 0 ? Math.max(0, Math.floor(active)) % frames.length : 0;
  const activeFrame = frames[safeActive];
  const nextFrame = frames[nextStoryFrameIndex(safeActive, frames.length)];
  const playing = playback?.playing ?? internalPlaying;
  const speed = playback?.speed ?? 1;
  const reducedMotion =
    playback?.reducedMotion ?? prefersReducedMotion;
  const viewportVisible = playback?.visibility === "visible"
    ? true
    : playback?.visibility === "offscreen"
      ? false
      : isIntersecting;
  const pageVisible = playback?.visibility === "visible"
    ? true
    : playback?.visibility === "hidden"
      ? false
      : documentVisible;
  const motionTheme: StoryMotionTheme = selectStoryMotionTheme({
    frames,
    motionSeed: story.motionSeed,
    explicitTheme: story.motionTheme,
    reducedMotion,
  });
  const motion = STORY_MOTION_REGISTRY[motionTheme];
  const motionDurationMs = motion.durationMs > 0
    ? Math.max(1_000, motion.durationMs / speed)
    : 0;
  const transitionMs = motion.transitionMs > 0
    ? Math.max(120, motion.transitionMs / speed)
    : 0;
  const motionStyle = {
    "--story-motion-duration": `${motionDurationMs}ms`,
    "--story-transition-duration": `${transitionMs}ms`,
  } as CSSProperties;
  const hasImages = frames.length > 0;
  const isReady = Boolean(
    activeFrame && readyFrameIds.has(activeFrame.id),
  );

  const writeActive = useCallback(
    (index: number) => {
      if (playback?.activeIndex === undefined) setInternalActive(index);
      playback?.onActiveIndexChange?.(index);
    },
    [playback],
  );

  const writePlaying = useCallback(
    (next: boolean) => {
      if (playback?.playing === undefined) setInternalPlaying(next);
      playback?.onPlayingChange?.(next);
    },
    [playback],
  );

  const selectFrame = useCallback(
    (index: number) => {
      if (activeFrame) setPreviousFrame(activeFrame);
      const total = frames.length;
      writeActive(total > 0 ? ((index % total) + total) % total : 0);
    },
    [activeFrame, frames.length, writeActive],
  );

  useEffect(() => {
    const mq = window.matchMedia("(prefers-reduced-motion: reduce)");
    const update = () => setPrefersReducedMotion(mq.matches);
    update();
    mq.addEventListener("change", update);
    return () => mq.removeEventListener("change", update);
  }, []);

  useEffect(() => {
    const sync = () => setDocumentVisible(document.visibilityState !== "hidden");
    document.addEventListener("visibilitychange", sync);
    return () => document.removeEventListener("visibilitychange", sync);
  }, []);

  useEffect(() => {
    const root = rootRef.current;
    if (!root || typeof IntersectionObserver === "undefined") return undefined;
    const observer = new IntersectionObserver(
      ([entry]) => setIsIntersecting(Boolean(entry?.isIntersecting)),
      { threshold: 0.2 },
    );
    observer.observe(root);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    if (!previousFrame) return undefined;
    const timer = window.setTimeout(
      () => setPreviousFrame(null),
      transitionMs,
    );
    return () => window.clearTimeout(timer);
  }, [previousFrame, transitionMs]);

  useEffect(() => {
    if (
      !shouldAutoAdvanceStory({
        playing,
        frameCount: frames.length,
        reducedMotion,
        isVisible: viewportVisible,
        documentVisible: pageVisible,
      })
    ) {
      return undefined;
    }
    const timer = window.setTimeout(
      () => selectFrame(nextStoryFrameIndex(safeActive, frames.length)),
      motionDurationMs,
    );
    return () => window.clearTimeout(timer);
  }, [
    frames.length,
    motionDurationMs,
    pageVisible,
    playing,
    reducedMotion,
    safeActive,
    selectFrame,
    viewportVisible,
  ]);

  function markReady(frameId: string) {
    setReadyFrameIds((current) => {
      if (current.has(frameId)) return current;
      const next = new Set(current);
      next.add(frameId);
      return next;
    });
  }

  function markFailed(frameId: string) {
    setFailedFrameIds((current) => {
      if (current.has(frameId)) return current;
      const next = new Set(current);
      next.add(frameId);
      return next;
    });
  }

  function openGallery() {
    if (usableGalleryUrls.length === 0) return;
    const index = activeFrame
      ? usableGalleryUrls.indexOf(activeFrame.url)
      : 0;
    setWalkerIndex(index >= 0 ? index : 0);
  }

  return (
    <section
      ref={rootRef}
      className={`property-scene ${motion.className}${
        hasImages ? " property-scene--live" : " property-scene--empty"
      }${isReady ? " is-ready" : ""}`}
      data-motion-theme={motionTheme}
      style={motionStyle}
      aria-labelledby="property-scene-title"
    >
      <div className="property-scene__stage">
        {previousFrame && previousFrame.id !== activeFrame?.id && (
          <div
            key={`previous-${previousFrame.id}`}
            className={`property-scene__layer is-previous ${focalPointClass(previousFrame)}`}
          >
            <ImageWithFallback
              src={previousFrame.url}
              alt=""
              className="property-scene__image"
              loading="eager"
              fetchPriority="low"
            />
          </div>
        )}
        {activeFrame ? (
          <div
            key={activeFrame.id}
            className={`property-scene__layer is-active ${focalPointClass(activeFrame)}`}
          >
            <ImageWithFallback
              src={activeFrame.url}
              alt={`${story.identity.title}, view ${safeActive + 1}`}
              className="property-scene__image"
              loading="eager"
              decoding="auto"
              fetchPriority={safeActive === 0 ? "high" : "auto"}
              onReady={() => markReady(activeFrame.id)}
              onError={() => markFailed(activeFrame.id)}
            />
          </div>
        ) : (
          <div
            className="property-scene__placeholder"
            role="img"
            aria-label="Photos unavailable"
          />
        )}

        {nextFrame && nextFrame.id !== activeFrame?.id && (
          <img
            className="property-scene__preload"
            src={nextFrame.url}
            alt=""
            aria-hidden="true"
            loading="eager"
            fetchPriority="low"
          />
        )}

        <div className="property-scene__loading" aria-hidden="true" />
        <div className="property-scene__vignette" aria-hidden="true" />
        <div className="property-scene__grain" aria-hidden="true" />

        <div className="property-scene__topline">
          <div className="property-scene__provenance">
            <span className="property-scene__chapter-count">
              Property story · {story.decks.length} chapters
            </span>
            {activeFrame?.lifecycle === "current" && (
              <span>Current image</span>
            )}
            {activeFrame?.lifecycle === "proposed" && (
              <span>Proposed render</span>
            )}
            {activeFrame?.sourceUrl && (
              <a
                href={activeFrame.sourceUrl}
                target="_blank"
                rel="noreferrer"
              >
                Source
              </a>
            )}
          </div>
          {actions && (
            <div className="property-scene__actions" aria-label="Property actions">
              {actions}
            </div>
          )}
        </div>

        <div className="property-scene__identity">
          <p>{story.identity.location}</p>
          <h1 id="property-scene-title">{story.identity.title}</h1>
          <div className="property-scene__facts" aria-label="Home summary">
            {story.identity.facts.map((fact) => (
              <span key={fact.key}>{fact.value}</span>
            ))}
          </div>
        </div>

        <div className="property-scene__controls">
          <div className="property-scene__sequence" aria-label="Property views">
            {frames.map((frame, index) => (
              <button
                key={frame.id}
                type="button"
                className={index === safeActive ? "is-active" : ""}
                aria-label={`Show view ${index + 1}`}
                aria-pressed={index === safeActive}
                onClick={() => selectFrame(index)}
              >
                {String(index + 1).padStart(2, "0")}
              </button>
            ))}
          </div>
          <div className="property-scene__transport">
            {frames.length > 1 && (
              <>
                <button
                  type="button"
                  aria-label="Previous image"
                  onClick={() => selectFrame(safeActive - 1)}
                >
                  ←
                </button>
                <button
                  type="button"
                  aria-label={playing ? "Pause images" : "Play images"}
                  aria-pressed={!playing}
                  onClick={() => writePlaying(!playing)}
                >
                  {playing ? "Pause" : "Play"}
                </button>
                <button
                  type="button"
                  aria-label="Next image"
                  onClick={() => selectFrame(safeActive + 1)}
                >
                  →
                </button>
              </>
            )}
            {usableGalleryUrls.length > 0 && (
              <button
                type="button"
                className="property-scene__gallery"
                onClick={openGallery}
              >
                All photos
                <span>{usableGalleryUrls.length}</span>
              </button>
            )}
          </div>
        </div>
      </div>

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

function focalPointClass(frame: StoryMediaFrame): string {
  const x = frame.focalPoint?.x ?? 0.5;
  const y = frame.focalPoint?.y ?? 0.5;
  const horizontal = x < 0.34 ? "left" : x > 0.66 ? "right" : "center";
  const vertical = y < 0.34 ? "top" : y > 0.66 ? "bottom" : "middle";
  return `story-focal--${horizontal} story-focal--${vertical}`;
}
