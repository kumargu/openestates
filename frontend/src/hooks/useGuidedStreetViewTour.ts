import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type RefObject,
} from "react";
import { loadGoogleStreetViewLibrary } from "../lib/googleMaps3d.ts";
import type { ArrivalPlaybackController } from "../lib/arrivalPlayback.ts";
import type { MapLayerExperience } from "../lib/types.ts";
import type { CorridorTourWaypoint } from "../lib/arrivalMapProjection.ts";
import {
  buildStreetViewSchedule,
  easedHeadingSteps,
  resolveStreetViewSequence,
  shouldReorientStreetView,
  streetViewAnchorHeading,
  streetViewPlayback,
  type StreetViewFrame,
  type StreetViewLink,
  type StreetViewResolution,
  type StreetViewSchedule,
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
    location?: { pano?: string };
  };
};

type StreetViewPanorama = {
  setPano: (pano: string) => void;
  setPov: (pov: { heading: number; pitch: number }) => void;
  setVisible: (visible: boolean) => void;
};

type StreetViewLibrary = {
  StreetViewPanorama: new (
    container: HTMLElement,
    options: {
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
    },
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

class GoogleStreetViewAdapter {
  private readonly panorama: StreetViewPanorama;
  private stopped = false;

  constructor(panorama: StreetViewPanorama) {
    this.panorama = panorama;
  }

  stop(): void {
    this.stopped = true;
  }

  resume(): void {
    this.stopped = false;
  }

  setFrame(frame: StreetViewFrame, heading: number, pitch = 0): void {
    if (this.stopped) return;
    this.panorama.setPano(frame.pano);
    this.panorama.setPov({ heading, pitch });
  }

  setPano(frame: StreetViewFrame): void {
    if (!this.stopped) this.panorama.setPano(frame.pano);
  }

  setPov(heading: number, pitch = 0): void {
    if (!this.stopped) this.panorama.setPov({ heading, pitch });
  }

  hide(): void {
    this.panorama.setVisible(false);
  }
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
      return {
        frame: pano ? { links: response.data.links ?? [], pano, waypoint } : null,
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
        const panorama = new library.StreetViewPanorama(container, {
          addressControl: false,
          clickToGo: true,
          disableDefaultUI: true,
          enableCloseButton: false,
          fullscreenControl: false,
          linksControl: true,
          motionTracking: false,
          panControl: false,
          pano: first.pano,
          pov: { heading: first.waypoint.heading, pitch: 0 },
          showRoadLabels: false,
          visible: true,
          zoom: experience.streetViewZoom,
          zoomControl: false,
        });
        const adapter = new GoogleStreetViewAdapter(panorama);
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
          adapter.setPano(entry.frame);
          let orientationDwellMs = 0;
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
            adapter.setPov(
              streetViewAnchorHeading(entry.frame.waypoint, anchor),
              experience.anchorPitch ?? 0,
            );
            if (!await run.wait(entranceDwellMs)) return;
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
        streetViewAnchorHeading(target.frame.waypoint, anchor),
        experience?.anchorPitch ?? 0,
      );
    }
    setProgress({ current: targetIndex + 1, total: schedule.entries.length });
  }, [anchor, experience?.anchorPitch, playbackController]);

  return { active: active && ready, progress, replay, skip, status };
}
