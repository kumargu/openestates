import { useEffect, useRef, useState, type RefObject } from "react";
import { loadGoogleStreetViewLibrary } from "../lib/googleMaps3d.ts";
import type { MapLayerExperience } from "../lib/types.ts";
import type { CorridorTourWaypoint } from "../lib/arrivalMapProjection.ts";

type StreetViewLink = {
  heading: number;
  pano: string;
};

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

export type StreetViewFrame = {
  links: StreetViewLink[];
  pano: string;
  waypoint: CorridorTourWaypoint;
};

const SEARCH_RADIUS_M = 35;
const CURVE_THRESHOLD_DEGREES = 12;
const SIDE_ROAD_MIN_DEGREES = 38;
const SIDE_ROAD_MAX_DEGREES = 132;

function normalizeHeading(heading: number): number {
  return (heading % 360 + 360) % 360;
}

function headingDistance(left: number, right: number): number {
  const difference = Math.abs(normalizeHeading(left) - normalizeHeading(right));
  return Math.min(difference, 360 - difference);
}

export function shouldReorientStreetView(currentHeading: number, nextHeading: number): boolean {
  return headingDistance(currentHeading, nextHeading) >= CURVE_THRESHOLD_DEGREES;
}

export function streetViewPlayback(frames: StreetViewFrame[]): StreetViewFrame[] {
  if (frames.length === 0) return [];
  const sorted = frames.slice().sort((left, right) =>
    left.waypoint.offsetM - right.waypoint.offsetM);
  const center = sorted.reduce((nearest, frame) =>
    Math.abs(frame.waypoint.offsetM) < Math.abs(nearest.waypoint.offsetM)
      ? frame
      : nearest);
  const forward = sorted.filter((frame) => frame.waypoint.offsetM >= center.waypoint.offsetM);
  const backward = sorted
    .filter((frame) => frame.waypoint.offsetM < center.waypoint.offsetM)
    .sort((left, right) => right.waypoint.offsetM - left.waypoint.offsetM);
  const playback = [
    ...forward,
    ...forward.slice(0, -1).reverse(),
    ...backward,
    ...backward.slice(0, -1).reverse(),
    center,
  ];
  return playback.filter((frame, index) =>
    index === 0 || frame.pano !== playback[index - 1].pano);
}

export function sideRoadHeading(
  links: StreetViewLink[],
  roadHeading: number,
): number | null {
  const candidates = links
    .map((link) => ({
      distance: Math.min(
        headingDistance(link.heading, roadHeading),
        headingDistance(link.heading, normalizeHeading(roadHeading + 180)),
      ),
      heading: link.heading,
    }))
    .filter(({ distance }) =>
      distance >= SIDE_ROAD_MIN_DEGREES && distance <= SIDE_ROAD_MAX_DEGREES)
    .sort((left, right) => Math.abs(90 - left.distance) - Math.abs(90 - right.distance));
  return candidates[0]?.heading ?? null;
}

function waitFor(milliseconds: number): Promise<void> {
  return new Promise((resolve) => window.setTimeout(resolve, milliseconds));
}

async function loadFrames(
  library: StreetViewLibrary,
  waypoints: CorridorTourWaypoint[],
): Promise<StreetViewFrame[]> {
  const service = new library.StreetViewService();
  const frames = await Promise.all(waypoints.map(async (waypoint) => {
    try {
      const response = await service.getPanorama({
        location: { lat: waypoint.latitude, lng: waypoint.longitude },
        preference: library.StreetViewPreference.NEAREST,
        radius: SEARCH_RADIUS_M,
        source: library.StreetViewSource.OUTDOOR,
      });
      const pano = response.data.location?.pano;
      if (!pano) return null;
      return { links: response.data.links ?? [], pano, waypoint } satisfies StreetViewFrame;
    } catch {
      return null;
    }
  }));
  return frames.filter((frame): frame is StreetViewFrame => Boolean(frame));
}

type GuidedStreetViewTourOptions = {
  active: boolean;
  containerRef: RefObject<HTMLDivElement | null>;
  experience: MapLayerExperience | null;
  waypoints: CorridorTourWaypoint[];
};

export function useGuidedStreetViewTour({
  active,
  containerRef,
  experience,
  waypoints,
}: GuidedStreetViewTourOptions): boolean {
  const panoramaRef = useRef<StreetViewPanorama | null>(null);
  const tourRunRef = useRef(0);
  const [ready, setReady] = useState(false);

  useEffect(() => {
    const container = containerRef.current;
    const runId = tourRunRef.current + 1;
    tourRunRef.current = runId;
    void Promise.resolve().then(() => {
      if (tourRunRef.current === runId) setReady(false);
    });
    panoramaRef.current = null;
    container?.replaceChildren();
    if (!active || !container || !experience || waypoints.length === 0) return undefined;

    const stopTour = () => {
      if (tourRunRef.current === runId) tourRunRef.current += 1;
    };
    container.addEventListener("pointerdown", stopTour, { capture: true });
    container.addEventListener("touchstart", stopTour, { capture: true, passive: true });
    container.addEventListener("wheel", stopTour, { capture: true, passive: true });
    container.addEventListener("keydown", stopTour, { capture: true });
    const visibilityChanged = () => {
      if (document.hidden) stopTour();
    };
    document.addEventListener("visibilitychange", visibilityChanged);
    const observer = new IntersectionObserver(([entry]) => {
      if (entry && !entry.isIntersecting) stopTour();
    }, { threshold: 0.15 });
    observer.observe(container);

    void Promise.all([
      loadGoogleStreetViewLibrary().then((loaded) =>
        loadFrames(loaded as StreetViewLibrary, waypoints)
          .then((frames) => ({ frames, library: loaded as StreetViewLibrary }))),
      waitFor(experience.transitionMs),
    ]).then(([{ frames, library }]) => {
      if (tourRunRef.current !== runId || frames.length === 0) return;
      const playback = streetViewPlayback(frames);
      const first = playback[0];
      if (!first) return;
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
      panoramaRef.current = panorama;
      setReady(true);

      if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;
      const seenJunctions = new Set<string>();
      let cameraHeading = first.waypoint.heading;
      void (async () => {
        for (let index = 0; index < playback.length; index += 1) {
          if (tourRunRef.current !== runId) return;
          const frame = playback[index];
          const next = playback[index + 1];
          const previous = playback[index - 1];
          const directionDelta = next
            ? next.waypoint.offsetM - frame.waypoint.offsetM
            : frame.waypoint.offsetM - (previous?.waypoint.offsetM ?? frame.waypoint.offsetM);
          const roadHeading = directionDelta >= 0
            ? frame.waypoint.heading
            : normalizeHeading(frame.waypoint.heading + 180);
          panorama.setPano(frame.pano);
          if (shouldReorientStreetView(cameraHeading, roadHeading)) {
            panorama.setPov({ heading: roadHeading, pitch: 0 });
            cameraHeading = roadHeading;
          }

          const curve = next
            ? headingDistance(frame.waypoint.heading, next.waypoint.heading)
              >= CURVE_THRESHOLD_DEGREES
            : false;
          const previousDelta = previous
            ? frame.waypoint.offsetM - previous.waypoint.offsetM
            : directionDelta;
          const turnaround = previousDelta * directionDelta < 0;
          await waitFor(curve || turnaround ? experience.curveDwellMs : experience.dwellMs);
          if (tourRunRef.current !== runId) return;

          if (frame.links.length >= 3 && !seenJunctions.has(frame.pano)) {
            const sideHeading = sideRoadHeading(frame.links, frame.waypoint.heading);
            if (sideHeading !== null) {
              seenJunctions.add(frame.pano);
              panorama.setPov({ heading: sideHeading, pitch: 0 });
              await waitFor(experience.sideRoadDwellMs);
              if (tourRunRef.current !== runId) return;
              panorama.setPov({ heading: roadHeading, pitch: 0 });
              cameraHeading = roadHeading;
              await waitFor(Math.round(experience.dwellMs / 2));
            }
          }
        }
      })();
    }).catch((error: unknown) => {
      if (import.meta.env.DEV) {
        console.warn("[useGuidedStreetViewTour] Street View tour unavailable", error);
      }
    });

    return () => {
      stopTour();
      container.removeEventListener("pointerdown", stopTour, { capture: true });
      container.removeEventListener("touchstart", stopTour, { capture: true });
      container.removeEventListener("wheel", stopTour, { capture: true });
      container.removeEventListener("keydown", stopTour, { capture: true });
      document.removeEventListener("visibilitychange", visibilityChanged);
      observer.disconnect();
      panoramaRef.current?.setVisible(false);
      panoramaRef.current = null;
      container.replaceChildren();
    };
  }, [active, containerRef, experience, waypoints]);

  return active && ready;
}
