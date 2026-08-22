import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
} from "react";
import { ImageWithFallback } from "../ImageWithFallback.tsx";
import {
  nextStoryFrameIndex,
  selectStoryMotionTheme,
  shouldAutoAdvanceStory,
  stableStoryHash,
  STORY_MOTION_REGISTRY,
  type StoryArrivalFrame,
  type StoryMediaFrame,
} from "../../lib/propertyStory.ts";
import type { StoryScenePlayback } from "./PropertySceneCard.tsx";
import "../../styles/property-arrival.css";

type Props = {
  propertyId: string;
  title: string;
  frames: StoryArrivalFrame[];
  playback?: StoryScenePlayback;
};

function motionFrames(frames: StoryArrivalFrame[]): StoryMediaFrame[] {
  return frames.map((frame) => ({
    id: frame.id,
    url: frame.url,
    role: "neighbourhood",
    sourceType: frame.sourceType,
    lifecycle: frame.lifecycle,
    capturedAt: frame.capturedAt,
    sourceUrl: frame.sourceUrl,
  }));
}

function frameLabel(frame: StoryArrivalFrame, index: number): string {
  return frame.label || `Approach view ${index + 1}`;
}

function distanceLabel(distanceFromGateM?: number): string | undefined {
  if (distanceFromGateM === undefined) return undefined;
  if (distanceFromGateM === 0) return "At the gate";
  return `${Math.round(distanceFromGateM)} m from gate`;
}

export function PropertyArrivalFilm({
  propertyId,
  title,
  frames: projectedFrames,
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
    useState<StoryArrivalFrame | null>(null);

  const frames = useMemo(
    () => projectedFrames.filter((frame) => !failedFrameIds.has(frame.id)),
    [failedFrameIds, projectedFrames],
  );
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
  const motionTheme = selectStoryMotionTheme({
    frames: motionFrames(frames),
    motionSeed: stableStoryHash(`${propertyId}:arrival`),
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
    "--arrival-motion-duration": `${motionDurationMs}ms`,
    "--arrival-transition-duration": `${transitionMs}ms`,
  } as CSSProperties;
  const isReady = Boolean(
    activeFrame && readyFrameIds.has(activeFrame.id),
  );
  const motionPaused =
    !playing || reducedMotion || !viewportVisible || !pageVisible;

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

  if (!activeFrame) return null;

  const label = frameLabel(activeFrame, safeActive);
  const distance = distanceLabel(activeFrame.distanceFromGateM);

  return (
    <section
      id="remote-arrival"
      ref={rootRef}
      className={`property-arrival ${motion.className}${
        isReady ? " is-ready" : ""
      }${
        motionPaused ? " is-paused" : ""
      }`}
      style={motionStyle}
      aria-labelledby="property-arrival-title"
    >
      <header className="property-arrival__intro">
        <span>Remote arrival</span>
        <h2 id="property-arrival-title">The way in.</h2>
      </header>

      <article
        className="property-arrival__stage"
        aria-label={`Approach to ${title}`}
      >
        {previousFrame && previousFrame.id !== activeFrame.id && (
          <div
            key={`previous-${previousFrame.id}`}
            className="property-arrival__layer is-previous"
          >
            <ImageWithFallback
              src={previousFrame.url}
              alt=""
              className="property-arrival__image"
              loading="lazy"
              fetchPriority="low"
            />
          </div>
        )}
        <div
          key={activeFrame.id}
          className="property-arrival__layer is-active"
        >
          <ImageWithFallback
            src={activeFrame.url}
            alt={`${title}: ${label}`}
            className="property-arrival__image"
            loading="lazy"
            fetchPriority="low"
            onReady={() => {
              setReadyFrameIds((current) => {
                if (current.has(activeFrame.id)) return current;
                const next = new Set(current);
                next.add(activeFrame.id);
                return next;
              });
            }}
            onError={() => {
              setFailedFrameIds((current) => {
                const next = new Set(current);
                next.add(activeFrame.id);
                return next;
              });
            }}
          />
        </div>
        {nextFrame && nextFrame.id !== activeFrame.id && (
          <img
            className="property-arrival__preload"
            src={nextFrame.url}
            alt=""
            aria-hidden="true"
            loading="lazy"
          />
        )}

        <div className="property-arrival__loading" aria-hidden="true" />
        <div className="property-arrival__vignette" aria-hidden="true" />

        <div className="property-arrival__topline">
          <span>{title} · arrival film</span>
          {activeFrame.sourceUrl && (
            <a
              href={activeFrame.sourceUrl}
              target="_blank"
              rel="noreferrer"
            >
              {activeFrame.stripKind === "street_view_strip"
                ? "Street View"
                : "Source"}{" "}
              ↗
            </a>
          )}
        </div>

        <div className="property-arrival__copy" aria-live="polite">
          <span>
            {String(safeActive + 1).padStart(2, "0")} /{" "}
            {String(frames.length).padStart(2, "0")}
          </span>
          <h3>{label}</h3>
          <p>
            {activeFrame.lifecycle === "current" ? "Street-level view" : ""}
            {activeFrame.lifecycle === "current" && distance ? " · " : ""}
            {distance}
          </p>
        </div>

        <div className="property-arrival__controls">
          <div
            className="property-arrival__sequence"
            aria-label="Arrival sequence"
          >
            {frames.map((frame, index) => (
              <button
                key={frame.id}
                type="button"
                className={index === safeActive ? "is-active" : ""}
                aria-label={`Show ${frameLabel(frame, index)}`}
                aria-pressed={index === safeActive}
                onClick={() => selectFrame(index)}
              >
                <span>{String(index + 1).padStart(2, "0")}</span>
                <strong>{frameLabel(frame, index)}</strong>
              </button>
            ))}
          </div>
          {frames.length > 1 && !reducedMotion && (
            <button
              type="button"
              className="property-arrival__play"
              aria-label={playing ? "Pause arrival film" : "Play arrival film"}
              aria-pressed={!playing}
              onClick={() => writePlaying(!playing)}
            >
              {playing ? "Pause film" : "Play film"}
            </button>
          )}
        </div>
      </article>
    </section>
  );
}
