import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type RefObject,
} from "react";
import { loadGoogleStreetViewLibrary } from "../lib/googleMaps3d.ts";
import type {
  ArrivalPlaybackController,
  ArrivalPlaybackRun,
} from "../lib/arrivalPlayback.ts";
import type { MapLayerExperience } from "../lib/types.ts";
import type { CorridorTourWaypoint } from "../lib/arrivalMapProjection.ts";
import {
  buildStreetViewSchedule,
  easedHeadingSteps,
  entranceCameraSequence,
  resolveStreetViewSequence,
  shouldReorientStreetView,
  streetViewAnchorHeading,
  streetViewPlayback,
  type StreetViewFrame,
  type StreetViewLink,
  type StreetViewResolution,
  type StreetViewSchedule,
  type StreetViewCameraPose,
} from "../lib/streetViewTour.ts";

export {
  buildStreetViewSchedule,
  easedHeadingSteps,
  resolveStreetViewSequence,
  shouldReorientStreetView,
  streetViewAnchorHeading,
  streetViewPlayback,
};
export type { StreetViewFrame, StreetViewResolution, StreetViewSchedule };

type StreetViewResponse = {
  data: {
    links?: StreetViewLink[];
    location?: {
      latLng?: { lat: () => number; lng: () => number };
      pano?: string;
    };
  };
};

type StreetViewPanorama = {
  setPano: (pano: string) => void;
  setPov: (pov: { heading: number; pitch: number }) => void;
  setVisible: (visible: boolean) => void;
};

type StreetViewPanoramaOptions = {
  addressControl: boolean;
  clickToGo: boolean;
  disableDefaultUI: boolean;
  enableCloseButton: boolean;
  fullscreenControl: boolean;
  linksControl: boolean;
  motionTracking: boolean;
  panControl: boolean;
  pano: string;
  pov: { heading: number; pitch: number };
  showRoadLabels: boolean;
  visible: boolean;
  zoom: number;
  zoomControl: boolean;
};

type StreetViewLibrary = {
  StreetViewPanorama: new (
    container: HTMLElement,
    options: StreetViewPanoramaOptions,
  ) => StreetViewPanorama;
  StreetViewPreference: { NEAREST: unknown };
  StreetViewService: new () => {
    getPanorama: (request: {
      location: { lat: number; lng: number };
      preference: unknown;
      radius: number;
      source: unknown;
    }) => Promise<StreetViewResponse>;
  };
  StreetViewSource: { OUTDOOR: unknown };
};

type StreetViewBufferSlot = {
  pane: HTMLDivElement;
  pano: string;
  panorama: StreetViewPanorama;
};

export class GoogleStreetViewAdapter {
  private activeSlot = 0;
  private readonly crossfadeMs: number;
  private readonly slots: [StreetViewBufferSlot, StreetViewBufferSlot];
  private animations: Animation[] = [];
  private stopped = false;

  constructor(
    slots: [StreetViewBufferSlot, StreetViewBufferSlot],
    crossfadeMs: number,
  ) {
    this.slots = slots;
    this.crossfadeMs = crossfadeMs;
  }

  stop(): void {
    this.stopped = true;
    this.cancelAnimations();
  }

  resume(): void {
    this.stopped = false;
  }

  setFrame(frame: StreetViewFrame, heading: number, pitch = 0): void {
    if (this.stopped) return;
    this.preload(frame, heading, pitch);
    this.showPreloaded(frame, heading, pitch);
  }

  preload(frame: StreetViewFrame, heading: number, pitch = 0): void {
    if (this.stopped) return;
    const slot = this.slots[this.inactiveSlot()];
    if (slot.pano !== frame.pano) {
      slot.pano = frame.pano;
      slot.panorama.setPano(frame.pano);
    }
    slot.panorama.setPov({ heading, pitch });
  }

  showPreloaded(frame: StreetViewFrame, heading: number, pitch = 0): void {
    if (this.stopped) return;
    const current = this.slots[this.activeSlot];
    if (current.pano === frame.pano) {
      current.panorama.setPov({ heading, pitch });
      return;
    }
    const nextSlotIndex = this.inactiveSlot();
    const next = this.slots[nextSlotIndex];
    if (next.pano !== frame.pano) {
      next.pano = frame.pano;
      next.panorama.setPano(frame.pano);
    }
    next.panorama.setPov({ heading, pitch });
    this.cancelAnimations();
    current.pane.classList.remove("is-active");
    current.pane.setAttribute("aria-hidden", "true");
    current.pane.inert = true;
    next.pane.classList.add("is-active");
    next.pane.setAttribute("aria-hidden", "false");
    next.pane.inert = false;
    this.activeSlot = nextSlotIndex;
    if (
      this.crossfadeMs <= 0
      || typeof current.pane.animate !== "function"
      || typeof next.pane.animate !== "function"
    ) return;
    this.animations = [
      current.pane.animate(
        [{ opacity: 1 }, { opacity: 0 }],
        { duration: this.crossfadeMs, easing: "ease-out" },
      ),
      next.pane.animate(
        [{ opacity: 0 }, { opacity: 1 }],
        { duration: this.crossfadeMs, easing: "ease-out" },
      ),
    ];
  }

  setPov(heading: number, pitch = 0): void {
    if (!this.stopped) {
      this.slots[this.activeSlot].panorama.setPov({ heading, pitch });
    }
  }

  hide(): void {
    this.cancelAnimations();
    for (const slot of this.slots) slot.panorama.setVisible(false);
  }

  private inactiveSlot(): 0 | 1 {
    return this.activeSlot === 0 ? 1 : 0;
  }

  private cancelAnimations(): void {
    for (const animation of this.animations) animation.cancel();
    this.animations = [];
  }
}

const CAMERA_POSE_STEP_COUNT = 4;

async function transitionCameraPose(
  adapter: GoogleStreetViewAdapter,
  run: ArrivalPlaybackRun,
  from: StreetViewCameraPose,
  to: StreetViewCameraPose,
  durationMs: number,
): Promise<boolean> {
  if (durationMs <= 0) {
    adapter.setPov(to.heading, to.pitch);
    return true;
  }
  const headings = easedHeadingSteps(from.heading, to.heading, CAMERA_POSE_STEP_COUNT);
  const stepDwellMs = Math.floor(durationMs / CAMERA_POSE_STEP_COUNT);
  let remainderMs = durationMs - stepDwellMs * CAMERA_POSE_STEP_COUNT;
  for (let index = 0; index < headings.length; index += 1) {
    const progress = (index + 1) / CAMERA_POSE_STEP_COUNT;
    adapter.setPov(
      headings[index],
      from.pitch + (to.pitch - from.pitch) * progress,
    );
    const extraMs = remainderMs > 0 ? 1 : 0;
    remainderMs -= extraMs;
    if (!await run.wait(stepDwellMs + extraMs)) return false;
  }
  return true;
}

function panoramaOptions(
  frame: StreetViewFrame,
  streetViewZoom: number,
): StreetViewPanoramaOptions {
  return {
    addressControl: false,
    clickToGo: true,
    disableDefaultUI: true,
    enableCloseButton: false,
    fullscreenControl: false,
    linksControl: true,
    motionTracking: false,
    panControl: false,
    pano: frame.pano,
    pov: { heading: frame.waypoint.heading, pitch: 0 },
    showRoadLabels: false,
    visible: true,
    zoom: streetViewZoom,
    zoomControl: false,
  };
}

const SEARCH_RADIUS_M = 35;

async function loadResolutions(
  library: StreetViewLibrary,
  waypoints: CorridorTourWaypoint[],
): Promise<StreetViewResolution[]> {
  const service = new library.StreetViewService();
  return Promise.all(waypoints.map(async (waypoint) => {
    try {
      const response = await service.getPanorama({
        location: { lat: waypoint.latitude, lng: waypoint.longitude },
        preference: library.StreetViewPreference.NEAREST,
        radius: SEARCH_RADIUS_M,
        source: library.StreetViewSource.OUTDOOR,
      });
      const pano = response.data.location?.pano;
      const latLng = response.data.location?.latLng;
      const panoramaPosition = latLng
        ? { latitude: latLng.lat(), longitude: latLng.lng() }
        : null;
      return {
        frame: pano && panoramaPosition
          ? { links: response.data.links ?? [], pano, panoramaPosition, waypoint }
          : null,
        waypoint,
      };
    } catch {
      return { frame: null, waypoint };
    }
  }));
}

type GuidedStreetViewTourOptions = {
  active: boolean;
  anchor?: { latitude: number; longitude: number } | null;
  autoPlay: boolean;
  containerRef: RefObject<HTMLDivElement | null>;
  experience: MapLayerExperience | null;
  interiorAnchor?: { latitude: number; longitude: number } | null;
  onPlaybackCancelled?: () => void;
  playbackController: ArrivalPlaybackController;
  waypoints: CorridorTourWaypoint[];
};

export type GuidedStreetViewTour = {
  active: boolean;
  progress: { current: number; total: number } | null;
  replay: () => void;
  skip: () => void;
  status: string | null;
};

export function useGuidedStreetViewTour({
  active,
  anchor,
  autoPlay,
  containerRef,
  experience,
  interiorAnchor,
  onPlaybackCancelled,
  playbackController,
  waypoints,
}: GuidedStreetViewTourOptions): GuidedStreetViewTour {
  const adapterRef = useRef<GoogleStreetViewAdapter | null>(null);
  const scheduleRef = useRef<StreetViewSchedule | null>(null);
  const autoPlayRef = useRef(autoPlay);
  const [ready, setReady] = useState(false);
  const [progress, setProgress] = useState<{ current: number; total: number } | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [replayVersion, setReplayVersion] = useState(0);
  const [manualPlay, setManualPlay] = useState(false);

  useEffect(() => {
    autoPlayRef.current = autoPlay;
  }, [autoPlay]);

  useEffect(() => {
    const container = containerRef.current;
    let disposed = false;
    adapterRef.current = null;
    scheduleRef.current = null;
    void Promise.resolve().then(() => {
      if (disposed) return;
      setReady(false);
      setProgress(null);
      setStatus(null);
    });
    container?.replaceChildren();
    if (!active || !container || !experience || waypoints.length === 0) return undefined;

    const run = playbackController.begin("playing");
    let unregisterStopper = () => {};
    let unregisterResumer = () => {};
    const cancel = () => {
      playbackController.cancel("settled");
      onPlaybackCancelled?.();
    };
    const visibilityChanged = () => {
      if (document.hidden) cancel();
    };
    container.addEventListener("pointerdown", cancel, { capture: true });
    container.addEventListener("touchstart", cancel, { capture: true, passive: true });
    container.addEventListener("wheel", cancel, { capture: true, passive: true });
    document.addEventListener("visibilitychange", visibilityChanged);

    void loadGoogleStreetViewLibrary()
      .then(async (loaded) => {
        const library = loaded as StreetViewLibrary;
        const resolutions = await loadResolutions(library, waypoints);
        if (disposed || !run.isCurrent()) return;
        const sequence = resolveStreetViewSequence(
          resolutions,
          experience.maximumPanoramaGapM ?? experience.waypointSpacingM * 2,
        );
        const schedule = buildStreetViewSchedule(sequence.frames, experience, anchor);
        const first = schedule.entries[0]?.frame;
        if (!first) {
          setStatus(experience.unavailableState ?? null);
          run.unavailable();
          return;
        }
        const second = schedule.entries[1]?.frame ?? first;
        const firstPane = document.createElement("div");
        firstPane.className = "nearby-map__street-view-buffer is-active";
        firstPane.setAttribute("aria-hidden", "false");
        firstPane.inert = false;
        const secondPane = document.createElement("div");
        secondPane.className = "nearby-map__street-view-buffer";
        secondPane.setAttribute("aria-hidden", "true");
        secondPane.inert = true;
        container.replaceChildren(firstPane, secondPane);
        const firstPanorama = new library.StreetViewPanorama(
          firstPane,
          panoramaOptions(first, experience.streetViewZoom),
        );
        const secondPanorama = new library.StreetViewPanorama(
          secondPane,
          panoramaOptions(second, experience.streetViewZoom),
        );
        const crossfadeMs = window.matchMedia("(prefers-reduced-motion: reduce)").matches
          ? 0
          : experience.panoramaCrossfadeMs ?? 0;
        const adapter = new GoogleStreetViewAdapter([
          { pane: firstPane, pano: first.pano, panorama: firstPanorama },
          { pane: secondPane, pano: second.pano, panorama: secondPanorama },
        ], crossfadeMs);
        adapterRef.current = adapter;
        scheduleRef.current = schedule;
        unregisterStopper = playbackController.registerStopper(() => adapter.stop());
        unregisterResumer = playbackController.registerResumer(() => adapter.resume());
        setReady(true);
        setProgress({ current: 1, total: schedule.entries.length });
        if (sequence.endedEarly) setStatus(experience.endsHereState ?? null);
        else if (sequence.skippedShortGap) setStatus(experience.shortGapState ?? null);

        const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
        if ((!autoPlayRef.current && !manualPlay) || reducedMotion) {
          playbackController.cancel("settled");
          adapter.resume();
          return;
        }
        if (!run.activate()) return;
        if (!await run.wait(experience.transitionMs) || !run.isCurrent()) return;

        let cameraHeading = first.waypoint.heading;
        for (let index = 0; index < schedule.entries.length; index += 1) {
          if (!run.isCurrent()) return;
          const entry = schedule.entries[index];
          const roadHeading = entry.frame.waypoint.heading;
          let orientationDwellMs = 0;
          if (index > 0) {
            adapter.showPreloaded(entry.frame, cameraHeading);
            const crossfadeDwellMs = Math.min(crossfadeMs, entry.dwellMs);
            if (!await run.wait(crossfadeDwellMs)) return;
            orientationDwellMs += crossfadeDwellMs;
          }
          const next = schedule.entries[index + 1];
          if (next) adapter.preload(next.frame, next.frame.waypoint.heading);
          if (shouldReorientStreetView(cameraHeading, roadHeading)) {
            const headingSteps = easedHeadingSteps(cameraHeading, roadHeading);
            const stepDwellMs = Math.min(90, Math.floor(entry.dwellMs / 8));
            for (const heading of headingSteps) {
              adapter.setPov(heading);
              if (!await run.wait(stepDwellMs)) return;
              orientationDwellMs += stepDwellMs;
            }
          }
          cameraHeading = roadHeading;
          setProgress({ current: index + 1, total: schedule.entries.length });
          const remainingDwellMs = Math.max(0, entry.dwellMs - orientationDwellMs);
          if (entry.lookAtEntrance && anchor) {
            const entranceDwellMs = experience.entranceDwellMs ?? 0;
            const approachDwellMs = Math.max(0, remainingDwellMs - entranceDwellMs);
            if (!await run.wait(Math.round(approachDwellMs / 2))) return;
            const cameraSequence = entranceCameraSequence(
              entry.frame.panoramaPosition,
              anchor,
              interiorAnchor,
              experience,
            );
            adapter.setPov(
              cameraSequence.entrance.heading,
              cameraSequence.entrance.pitch,
            );
            if (!await run.wait(cameraSequence.entrance.dwellMs)) return;
            if (cameraSequence.interior) {
              if (!await transitionCameraPose(
                adapter,
                run,
                cameraSequence.entrance,
                cameraSequence.interior,
                cameraSequence.interior.transitionMs,
              )) return;
              if (!await run.wait(cameraSequence.interior.dwellMs)) return;
            }
            adapter.setPov(roadHeading, 0);
            if (!await run.wait(Math.ceil(approachDwellMs / 2))) return;
          } else if (!await run.wait(remainingDwellMs)) return;
        }
        run.settle();
      })
      .catch((error: unknown) => {
        if (import.meta.env.DEV) {
          console.warn("[useGuidedStreetViewTour] Street View tour unavailable", error);
        }
        if (!disposed) {
          setStatus(experience.unavailableState ?? null);
          run.unavailable();
        }
      });

    return () => {
      disposed = true;
      unregisterStopper();
      unregisterResumer();
      if (run.isCurrent()) playbackController.cancel("settled");
      container.removeEventListener("pointerdown", cancel, { capture: true });
      container.removeEventListener("touchstart", cancel, { capture: true });
      container.removeEventListener("wheel", cancel, { capture: true });
      document.removeEventListener("visibilitychange", visibilityChanged);
      adapterRef.current?.hide();
      adapterRef.current = null;
      container.replaceChildren();
    };
  }, [
    active,
    anchor,
    containerRef,
    experience,
    interiorAnchor,
    manualPlay,
    onPlaybackCancelled,
    playbackController,
    replayVersion,
    waypoints,
  ]);

  const replay = useCallback(() => {
    playbackController.cancel("idle");
    setManualPlay(true);
    setReplayVersion((current) => current + 1);
  }, [playbackController]);

  const skip = useCallback(() => {
    const schedule = scheduleRef.current;
    const adapter = adapterRef.current;
    if (!schedule || !adapter || schedule.entries.length === 0) return;
    playbackController.cancel("settled");
    adapter.resume();
    const targetIndex = schedule.entranceIndex ?? schedule.entries.length - 1;
    const target = schedule.entries[targetIndex];
    adapter.setFrame(target.frame, target.frame.waypoint.heading);
    if (target.lookAtEntrance && anchor) {
      adapter.setPov(
        streetViewAnchorHeading(target.frame.panoramaPosition, anchor),
        experience?.anchorPitch ?? 0,
      );
    }
    setProgress({ current: targetIndex + 1, total: schedule.entries.length });
  }, [anchor, experience?.anchorPitch, playbackController]);

  return { active: active && ready, progress, replay, skip, status };
}
