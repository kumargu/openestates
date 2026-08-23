import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
} from "react";
import { ImageWithFallback } from "../ImageWithFallback.tsx";
import {
  nextStoryFrameIndex,
  selectStoryMotionTheme,
  shouldAutoAdvanceStory,
  STORY_MOTION_REGISTRY,
  wrappedFilmstripOffset,
  type StoryFocalPoint,
  type StoryMediaLifecycle,
  type StoryMotionTheme,
} from "../../lib/propertyStory.ts";
import "../../styles/property-filmstrip.css";

const CINEMATIC_STAGE_DURATION_MS = {
  standard: 7_800,
  brisk: 6_400,
} as const;

export type StoryPlaybackSpeed = 0.5 | 1 | 2;
export type StoryCinematicPace = keyof typeof CINEMATIC_STAGE_DURATION_MS;

export type StoryScenePlayback = {
  activeIndex?: number;
  playing?: boolean;
  speed?: StoryPlaybackSpeed;
  reducedMotion?: boolean;
  visibility?: "auto" | "visible" | "hidden" | "offscreen";
  onActiveIndexChange?: (index: number) => void;
  onPlayingChange?: (playing: boolean) => void;
};

export type PropertyFilmstripFrame = {
  id: string;
  url: string;
  label: string;
  meta?: string;
  lifecycle: StoryMediaLifecycle;
  sourceUrl?: string;
  focalPoint?: StoryFocalPoint;
  showCaption?: boolean;
  viewScale?: "wide" | "close";
};

type Props = {
  ariaLabel: string;
  frames: PropertyFilmstripFrame[];
  motionSeed: number;
  motionTheme?: StoryMotionTheme;
  playback?: StoryScenePlayback;
  priority?: boolean;
  showPlaybackControl?: boolean;
  presentation?: "card" | "stage";
  cinematicMotion?: boolean;
  cinematicPace?: StoryCinematicPace;
  galleryLabel?: string;
  onOpenGallery?: (activeFrameId: string) => void;
  onUsableFramesChange?: (frameIds: string[]) => void;
};

function speedClass(speed: StoryPlaybackSpeed): string {
  if (speed === 0.5) return "story-speed--half";
  if (speed === 2) return "story-speed--double";
  return "story-speed--normal";
}

function positionClass(offset: number): string {
  if (offset < 0) return `filmstrip-position--m${Math.abs(offset)}`;
  if (offset > 0) return `filmstrip-position--p${offset}`;
  return "filmstrip-position--active";
}

function focalPointClass(focalPoint?: StoryFocalPoint): string {
  const x = focalPoint?.x ?? 0.5;
  const y = focalPoint?.y ?? 0.5;
  const horizontal = x < 0.34 ? "left" : x > 0.66 ? "right" : "center";
  const vertical = y < 0.34 ? "top" : y > 0.66 ? "bottom" : "middle";
  return `filmstrip-focal--${horizontal} filmstrip-focal--${vertical}`;
}

export function PropertyFilmstrip({
  ariaLabel,
  frames: projectedFrames,
  motionSeed,
  motionTheme,
  playback,
  priority = false,
  showPlaybackControl = false,
  presentation = "card",
  cinematicMotion = false,
  cinematicPace = "standard",
  galleryLabel,
  onOpenGallery,
  onUsableFramesChange,
}: Props) {
  const rootRef = useRef<HTMLDivElement>(null);
  const [internalActive, setInternalActive] = useState(0);
  const [internalPlaying, setInternalPlaying] = useState(true);
  const [prefersReducedMotion, setPrefersReducedMotion] = useState(false);
  const [isIntersecting, setIsIntersecting] = useState(true);
  const [documentVisible, setDocumentVisible] = useState(
    () => document.visibilityState !== "hidden",
  );
  const remainingDurationRef = useRef(0);
  const timerDeadlineRef = useRef<number | null>(null);
  const [failedFrameIds, setFailedFrameIds] = useState<Set<string>>(
    () => new Set(),
  );
  const [readyFrameIds, setReadyFrameIds] = useState<Set<string>>(
    () => new Set(),
  );

  const frames = useMemo(
    () =>
      projectedFrames.filter((frame) => !failedFrameIds.has(frame.id)),
    [failedFrameIds, projectedFrames],
  );
  const active = playback?.activeIndex ?? internalActive;
  const safeActive =
    frames.length > 0 ? Math.max(0, Math.floor(active)) % frames.length : 0;
  const activeFrame = frames[safeActive];
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
  const selectedTheme = selectStoryMotionTheme({
    frames: frames.map((frame) => ({
      id: frame.id,
      url: frame.url,
      role: "gallery",
      sourceType: "story",
      lifecycle: frame.lifecycle,
      sourceUrl: frame.sourceUrl,
      focalPoint: frame.focalPoint,
    })),
    motionSeed,
    explicitTheme: motionTheme,
    reducedMotion,
  });
  const motion = STORY_MOTION_REGISTRY[selectedTheme];
  const sceneDurationMs = cinematicMotion
    ? CINEMATIC_STAGE_DURATION_MS[cinematicPace]
    : motion.durationMs;
  const durationMs = sceneDurationMs > 0
    ? Math.max(1_000, sceneDurationMs / speed)
    : 0;
  const isReady = Boolean(
    activeFrame && readyFrameIds.has(activeFrame.id),
  );
  const paused =
    !playing || reducedMotion || !viewportVisible || !pageVisible || !isReady;

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
      const total = frames.length;
      writeActive(total > 0 ? ((index % total) + total) % total : 0);
    },
    [frames.length, writeActive],
  );

  useEffect(() => {
    const media = window.matchMedia("(prefers-reduced-motion: reduce)");
    const sync = () => setPrefersReducedMotion(media.matches);
    sync();
    media.addEventListener("change", sync);
    return () => media.removeEventListener("change", sync);
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
    onUsableFramesChange?.(frames.map((frame) => frame.id));
  }, [frames, onUsableFramesChange]);

  useEffect(() => {
    remainingDurationRef.current = durationMs;
    timerDeadlineRef.current = null;
  }, [durationMs, safeActive]);

  useEffect(() => {
    if (durationMs <= 0 || !isReady) return undefined;
    if (
      !shouldAutoAdvanceStory({
        playing,
        frameCount: frames.length,
        reducedMotion,
        isVisible: viewportVisible,
        documentVisible: pageVisible,
        durationMs,
      })
    ) {
      return undefined;
    }
    const remainingMs = remainingDurationRef.current > 0
      ? Math.min(remainingDurationRef.current, durationMs)
      : durationMs;
    timerDeadlineRef.current = performance.now() + remainingMs;
    const timer = window.setTimeout(
      () => {
        timerDeadlineRef.current = null;
        remainingDurationRef.current = durationMs;
        selectFrame(nextStoryFrameIndex(safeActive, frames.length));
      },
      remainingMs,
    );
    return () => {
      window.clearTimeout(timer);
      if (timerDeadlineRef.current !== null) {
        remainingDurationRef.current = Math.max(
          0,
          timerDeadlineRef.current - performance.now(),
        );
        timerDeadlineRef.current = null;
      }
    };
  }, [
    durationMs,
    frames.length,
    isReady,
    pageVisible,
    playing,
    reducedMotion,
    safeActive,
    selectFrame,
    viewportVisible,
  ]);

  function handleKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    if (frames.length <= 1) return;
    if (event.key === "ArrowLeft") {
      event.preventDefault();
      selectFrame(safeActive - 1);
    }
    if (event.key === "ArrowRight") {
      event.preventDefault();
      selectFrame(safeActive + 1);
    }
  }

  if (!activeFrame) return null;

  return (
    <div
      ref={rootRef}
      className={`property-filmstrip ${motion.className} ${speedClass(speed)}${
        isReady ? " is-ready" : ""
      }${isReady ? " is-frame-ready" : ""}${
        cinematicMotion ? " property-filmstrip--cinematic" : ""
      }${cinematicMotion ? ` property-filmstrip--cinematic-${cinematicPace}` : ""
      }${paused ? " is-paused" : ""}${
        presentation === "stage" ? " property-filmstrip--stage" : ""
      }`}
      data-motion-theme={selectedTheme}
      tabIndex={0}
      role="region"
      aria-label={ariaLabel}
      onKeyDown={handleKeyDown}
    >
      <div className="property-filmstrip__track">
        {frames.map((frame, index) => {
          const offset = wrappedFilmstripOffset(index, safeActive, frames.length);
          const activeCard = offset === 0;
          return (
            <figure
              key={frame.id}
              className={`property-filmstrip__card ${positionClass(offset)} ${
                activeCard ? "is-active" : ""
              } ${focalPointClass(frame.focalPoint)}${
                frame.viewScale ? ` filmstrip-view--${frame.viewScale}` : ""
              }`}
              data-lifecycle={frame.lifecycle}
              aria-hidden={!activeCard}
            >
              <ImageWithFallback
                src={frame.url}
                alt={activeCard ? `${ariaLabel}: ${frame.label}` : ""}
                loading={priority && activeCard ? "eager" : "lazy"}
                decoding="async"
                fetchPriority={priority && activeCard ? "high" : "low"}
                onReady={() => {
                  setReadyFrameIds((current) => {
                    if (current.has(frame.id)) return current;
                    const next = new Set(current);
                    next.add(frame.id);
                    return next;
                  });
                }}
                onError={() => {
                  setFailedFrameIds((current) => {
                    if (current.has(frame.id)) return current;
                    const next = new Set(current);
                    next.add(frame.id);
                    return next;
                  });
                }}
              />
              {frame.showCaption !== false && (
                <figcaption>
                  <strong>{frame.label}</strong>
                  {frame.meta && <span>{frame.meta}</span>}
                </figcaption>
              )}
            </figure>
          );
        })}
      </div>

      <div className="property-filmstrip__loading" aria-hidden="true" />

      {!reducedMotion && showPlaybackControl && (
        <button
          type="button"
          className="property-filmstrip__playback"
          aria-label={playing ? "Pause images" : "Resume images"}
          onClick={() => writePlaying(!playing)}
        >
          {playing ? "Pause images" : "Resume images"}
        </button>
      )}

      <div className="property-filmstrip__controls">
        {frames.length > 1 && (
          <>
            <button
              type="button"
              aria-label="Previous image"
              onClick={() => selectFrame(safeActive - 1)}
            >
              ←
            </button>
            <div
              className="property-filmstrip__sequence"
              aria-label="Property views"
            >
              {frames.map((frame, index) => (
                <button
                  key={frame.id}
                  type="button"
                  className={index === safeActive ? "is-active" : ""}
                  aria-label={`Show ${frame.label}`}
                  aria-pressed={index === safeActive}
                  onClick={() => selectFrame(index)}
                >
                  <span className="sr-only">
                    {String(index + 1).padStart(2, "0")}
                  </span>
                </button>
              ))}
            </div>
            <span className="property-filmstrip__count" aria-live="polite">
              {String(safeActive + 1).padStart(2, "0")} /{" "}
              {String(frames.length).padStart(2, "0")}
            </span>
          </>
        )}
        {activeFrame.sourceUrl && (
          <a
            href={activeFrame.sourceUrl}
            target="_blank"
            rel="noreferrer"
          >
            Source ↗
          </a>
        )}
        {galleryLabel && onOpenGallery && (
          <button
            type="button"
            className="property-filmstrip__gallery"
            onClick={() => onOpenGallery(activeFrame.id)}
          >
            {galleryLabel}
          </button>
        )}
        {frames.length > 1 && (
          <button
            type="button"
            aria-label="Next image"
            onClick={() => selectFrame(safeActive + 1)}
          >
            →
          </button>
        )}
      </div>
    </div>
  );
}
