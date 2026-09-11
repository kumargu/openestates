import { useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";
import { useAtlasRoadFlight } from "../../hooks/useAtlasRoadFlight.ts";
import { arrivalAtlasRoute, arrivalAtlasContextLines } from "../../lib/homeAtlasProjection.ts";
import { pointAlongRoute, projectStreetHandoff, type AtlasCameraPose } from "../../../../experiments/home-atlas/src/journey.ts";
import policy from "../../../../app/config/ui/home-atlas.json";
import { nearbySceneCamera, nearbyRelationArc, geometryForPlace, type NearbyDepth } from '../../lib/atlasNearbyScene.ts';
import {
  loadGoogleMaps3dLibrary,
  loadGoogleMarkerLibrary,
  loadGoogleTerrainElevation,
} from "../../lib/googleMaps3d.ts";
import { mapMarkerPinOptions } from "../../lib/mapMarkerVisual.ts";
import { useGuidedStreetViewTour } from "../../hooks/useGuidedStreetViewTour.ts";
import type { ArrivalPlaybackController } from "../../lib/arrivalPlayback.ts";
import type {
  ArrivalSceneExperience,
  ArrivalSearchSociety,
  MapLayerExperience,
  MapOverlayLine,
  MapOverlayPolygon,
} from "../../lib/types.ts";
import type {
  NumberedPlace,
  PlateViewport,
} from "../../lib/nearbyPlateProjection.ts";
import {
  cameraCenterForMode,
  corridorCameraFocus,
  corridorTourWaypoints,
  societyCameraComposition,
  type ArrivalCameraMode,
} from "../../lib/arrivalMapProjection.ts";

export type ArrivalGoogle3DMapProps = {
  home: {
    latitude: number;
    longitude: number;
    name: string;
    boundary?: MapOverlayPolygon;
  };
  places: NumberedPlace[];
  viewport: PlateViewport;
  metroLines: MapOverlayLine[];
  accessLines: MapOverlayLine[];
  showMetroLines: boolean;
  expanded: boolean;
  cameraMode: ArrivalCameraMode;
  terrainCorridor: boolean;
  layerExperience?: MapLayerExperience;
  arrivalExperience?: ArrivalSceneExperience;
  playbackController: ArrivalPlaybackController;
  autoPlaySociety: boolean;
  societyPlaybackVersion?: number;
  autoPlayApproach?: boolean;
  secondarySocieties?: ArrivalSearchSociety[];
  selectedSecondarySocietyId?: string | null;
  onSelectSecondarySociety?: (societyId: string) => void;
  onPlaybackCancelled?: () => void;
  onToggleExpanded: () => void;
  selectedPlaceId?: string | null;
  nearbyDepth?: NearbyDepth;
  onSelectPlace?: (id: string | null) => void;
  above?: boolean;
  quiet?: boolean;
  showBoundary?: boolean;
  polygons?: MapOverlayPolygon[];
  contextLines?: MapOverlayLine[];
  showExpandAction?: boolean;
};

type LatLngAltitude = { lat: number; lng: number; altitude?: number };

type CameraOptions = {
  center?: LatLngAltitude;
  cameraPosition?: LatLngAltitude;
  fov?: number;
  heading: number;
  range: number;
  tilt: number;
};

type Map3DElement = HTMLElement & {
  center: LatLngAltitude;
  cameraPosition?: LatLngAltitude;
  fov?: number;
  flyCameraTo: (options: {
    durationMillis: number;
    endCamera: CameraOptions;
  }) => void;
  stopCameraAnimation: () => void;
  gestureHandling: "COOPERATIVE" | "GREEDY";
  heading: number;
  range: number;
  tilt: number;
};

type Map3DChild = HTMLElement & {
  remove: () => void;
};

type Popover3DElement = Map3DChild & {
  open: boolean;
};

type Maps3DLibrary = {
  Map3DElement: new (options: {
    center: LatLngAltitude;
    defaultUIHidden?: boolean;
    gestureHandling?: "COOPERATIVE" | "GREEDY";
    heading: number;
    mode: "SATELLITE";
    range: number;
    tilt: number;
  }) => Map3DElement;
  Marker3DInteractiveElement: new (options: {
    altitudeMode?: "CLAMP_TO_GROUND" | "RELATIVE_TO_MESH";
    collisionBehavior?: "REQUIRED" | "OPTIONAL_AND_HIDES_LOWER_PRIORITY";
    drawsWhenOccluded?: boolean;
    extruded?: boolean;
    gmpPopoverTargetElement?: Popover3DElement;
    label?: string;
    position: LatLngAltitude;
    title?: string;
  }) => Map3DChild;
  PopoverElement: new (options?: {
    autoPanDisabled?: boolean;
    lightDismissDisabled?: boolean;
    open?: boolean;
  }) => Popover3DElement;
  Polygon3DElement: new (options: {
    altitudeMode?: "CLAMP_TO_GROUND" | "RELATIVE_TO_GROUND";
    drawsOccludedSegments?: boolean;
    fillColor: string;
    strokeColor: string;
    strokeWidth: number;
  }) => Map3DChild & { path: LatLngAltitude[]; innerPaths: LatLngAltitude[][] };
  Polyline3DInteractiveElement: new (options: {
    altitudeMode?: "CLAMP_TO_GROUND" | "RELATIVE_TO_GROUND";
    drawsOccludedSegments?: boolean;
    outerColor?: string;
    outerWidth?: number;
    path: LatLngAltitude[];
    strokeColor: string;
    strokeWidth: number;
  }) => Map3DChild;
};

type MarkerLibrary = {
  PinElement: new (options: {
    background?: string;
    borderColor?: string;
    glyphSrc?: string;
    glyphText?: string;
    scale?: number;
  }) => HTMLElement;
};

const HOME_PORTRAIT_RANGE_M = 700;
const HOME_PORTRAIT_TILT = 48;
const EVIDENCE_CAMERA_DURATION_MS = 600;
const HOME_CAMERA_DURATION_MS = 350;
const DEFAULT_HEADING = 210;
const EMPTY_CONTEXT_LINES: MapOverlayLine[] = [];

function targetCamera(
  latitude: number,
  longitude: number,
  elevation: number,
  range: number,
  tilt: number,
  heading: number,
): CameraOptions {
  return {
    center: { lat: latitude, lng: longitude, altitude: elevation },
    heading,
    range,
    tilt,
  };
}

function settleCameraFraming(map: Map3DElement, camera: CameraOptions) {
  if (camera.center) map.center = camera.center;
  if (camera.cameraPosition) map.cameraPosition = camera.cameraPosition;
  if (camera.fov) map.fov = camera.fov;
  map.heading = camera.heading;
  map.range = camera.range;
  map.tilt = camera.tilt;
}

function pathFromPolygon(polygon: MapOverlayPolygon): LatLngAltitude[] {
  return polygon.coordinates.map(([lng, lat]) => ({ lat, lng }));
}

function circlePath(latitude: number, longitude: number, radiusM = 115): LatLngAltitude[] {
  const latitudeDegrees = radiusM / 111_320;
  const longitudeDegrees = radiusM / (
    111_320 * Math.max(0.2, Math.cos(latitude * Math.PI / 180))
  );
  return Array.from({ length: 25 }, (_, index) => {
    const angle = index / 24 * Math.PI * 2;
    return {
      lat: latitude + Math.sin(angle) * latitudeDegrees,
      lng: longitude + Math.cos(angle) * longitudeDegrees,
    };
  });
}

function lineCoordinates(line: MapOverlayLine): LatLngAltitude[] {
  return line.coordinates.map(([lng, lat]) => ({ lat, lng }));
}

function lineLabelPosition(line: MapOverlayLine): LatLngAltitude | null {
  if (line.coordinates.length === 0) return null;
  const index = Math.floor((line.coordinates.length - 1) * 0.35);
  const [lng, lat] = line.coordinates[index];
  return { lat, lng };
}

function addPolygon(
  map: Map3DElement,
  library: Maps3DLibrary,
  polygon: MapOverlayPolygon,
  colors: { fill: string; stroke: string },
  children: Map3DChild[],
) {
  const element = new library.Polygon3DElement({
    altitudeMode: "CLAMP_TO_GROUND",
    drawsOccludedSegments: false,
    fillColor: colors.fill,
    strokeColor: colors.stroke,
    strokeWidth: 2,
  });
  element.path = pathFromPolygon(polygon);
  if (polygon.holes?.length) element.innerPaths = polygon.holes.map(ring => ring.map(([lng, lat]) => ({lat,lng})));
  map.append(element);
  children.push(element);
}

function addLine(
  map: Map3DElement,
  library: Maps3DLibrary,
  line: MapOverlayLine,
  style: {
    color: string;
    width: number;
    outerColor: string;
    outerWidth: number;
    drawsOccludedSegments: boolean;
  },
  onSelect: (() => void) | null,
  children: Map3DChild[],
) {
  const element = new library.Polyline3DInteractiveElement({
    altitudeMode: "CLAMP_TO_GROUND",
    drawsOccludedSegments: style.drawsOccludedSegments,
    outerColor: style.outerColor,
    outerWidth: style.outerWidth,
    path: lineCoordinates(line),
    strokeColor: style.color,
    strokeWidth: style.width,
  });
  if (onSelect) element.addEventListener("gmp-click", onSelect);
  map.append(element);
  children.push(element);
}

function placeMeta(place: NumberedPlace): string {
  return [
    typeof place.distance_km === "number" ? `${place.distance_km.toFixed(1)} km` : null,
    typeof place.rating === "number" ? `${place.rating.toFixed(1)} rating` : null,
    typeof place.review_count === "number" ? `${place.review_count} reviews` : null,
  ].filter((value): value is string => Boolean(value)).join(" · ");
}

function createPlacePopover(
  library: Maps3DLibrary,
  place: NumberedPlace,
): Popover3DElement {
  const popover = new library.PopoverElement({
    autoPanDisabled: true,
    lightDismissDisabled: false,
  });
  popover.append(createPlacePopoverContent(place));
  return popover;
}

function createPlacePopoverContent(place: NumberedPlace): HTMLDivElement {
  const content = document.createElement("div");
  content.className = "nearby-map-popover";
  const name = document.createElement("strong");
  name.textContent = place.name;
  content.append(name);
  const meta = placeMeta(place);
  if (meta) {
    const details = document.createElement("span");
    details.textContent = meta;
    content.append(details);
  }
  return content;
}

export function PropertyArrivalGoogle3DMap(props: ArrivalGoogle3DMapProps) {
  const {
    home,
    places,
    viewport,
    metroLines,
    accessLines,
    showMetroLines,
    expanded,
    cameraMode,
    terrainCorridor,
    layerExperience,
    arrivalExperience,
    playbackController,
    autoPlaySociety,
    societyPlaybackVersion = 0,
    autoPlayApproach = true,
    secondarySocieties = [],
    selectedSecondarySocietyId = null,
    onSelectSecondarySociety,
    onPlaybackCancelled,
    onToggleExpanded,
    selectedPlaceId = null,
    nearbyDepth = 'overview',
    onSelectPlace,
    above = false,
    quiet = false,
    showBoundary = true,
    polygons,
    contextLines = EMPTY_CONTEXT_LINES,
    showExpandAction = true,
  } = props;
  const containerRef = useRef<HTMLDivElement | null>(null);
  const streetViewContainerRef = useRef<HTMLDivElement | null>(null);
  const mapRef = useRef<Map3DElement | null>(null);
  const libraryRef = useRef<Maps3DLibrary | null>(null);
  const markerLibraryRef = useRef<MarkerLibrary | null>(null);
  const childrenRef = useRef<Map3DChild[]>([]);
  const cameraMoveRef = useRef(0);
  const terrainElevationRef = useRef<number | null>(null);
  const initialSocietyAutoPlayRef = useRef(autoPlaySociety);
  const previousSocietyAutoPlayRef = useRef(autoPlaySociety);
  // An epoch ensures overlays/cameras also attach after a scene-driven map rebuild.
  const [ready, setReady] = useState(0);
  const [groundElevation, setGroundElevation] = useState(0);
  const [mapWidth, setMapWidth] = useState(1000);
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    const observer = new ResizeObserver(entries => setMapWidth(entries[0].contentRect.width));
    observer.observe(container);
    return () => observer.disconnect();
  }, []);
  const [loadError, setLoadError] = useState<Error | null>(null);
  const [streetRequested, setStreetRequested] = useState(false);
  const [streetStart, setStreetStart] = useState(0);
  const homeLatitude = home.latitude;
  const homeLongitude = home.longitude;
  const roadExperience = layerExperience?.kind === "street_view_tour"
    ? layerExperience
    : null;
  const roadTourActive = terrainCorridor && cameraMode === "evidence";
  const atlasRoute = useMemo(() => arrivalAtlasRoute(accessLines), [accessLines]);
  const roadFocus = useMemo(
    () => roadTourActive
      ? corridorCameraFocus(accessLines, {
        latitude: homeLatitude,
        longitude: homeLongitude,
      })
      : null,
    [accessLines, homeLatitude, homeLongitude, roadTourActive],
  );
  const entranceAnchor = useMemo(() => {
    const entrance = places.find((place) => place.icon === "entrance" || place.icon === "entrance-likely");
    return entrance
      ? { latitude: entrance.latitude, longitude: entrance.longitude }
      : null;
  }, [places]);
  const societyInteriorAnchor = useMemo(
    () => ({ latitude: homeLatitude, longitude: homeLongitude }),
    [homeLatitude, homeLongitude],
  );
  const roadWaypoints = useMemo(
    () => roadFocus && roadExperience
      ? corridorTourWaypoints(
        accessLines,
        { latitude: homeLatitude, longitude: homeLongitude },
        roadExperience.waypointSpacingM,
        {
          anchor: entranceAnchor,
          anchorLookAheadM: roadExperience.anchorLookAheadM,
        },
      )
      : [],
    [accessLines, entranceAnchor, homeLatitude, homeLongitude, roadExperience, roadFocus],
  );
  const roadLandingFocus = roadWaypoints[0] ?? roadFocus;
  const societyComposition = useMemo(
    () => arrivalExperience
      ? societyCameraComposition(
        { latitude: homeLatitude, longitude: homeLongitude },
        home.boundary,
        arrivalExperience,
        window.innerWidth,
      )
      : null,
    [arrivalExperience, home.boundary, homeLatitude, homeLongitude],
  );
  const cameraCenter = roadLandingFocus ?? cameraCenterForMode(cameraMode, home, viewport);
  const cameraLatitude = cameraCenter.latitude;
  const cameraLongitude = cameraCenter.longitude;
  const roadTour = useGuidedStreetViewTour({
    active: Boolean(roadLandingFocus) && streetRequested,
    anchor: entranceAnchor,
    autoPlay: autoPlayApproach,
    containerRef: streetViewContainerRef,
    experience: roadExperience,
    interiorAnchor: societyInteriorAnchor,
    onPlaybackCancelled,
    playbackController,
    waypoints: useMemo(() => roadWaypoints.slice(streetStart), [roadWaypoints, streetStart]),
  });
  const playbackState = useSyncExternalStore(
    playbackController.subscribe,
    playbackController.snapshot,
    playbackController.snapshot,
  );
  const streetViewReady = roadTour.active;
  const renderRoad = useCallback((pose: AtlasCameraPose) => {
    const map = mapRef.current;
    if (map) settleCameraFraming(map, targetCamera(pose.center.latitude, pose.center.longitude,
      pose.center.altitude, pose.range, pose.tilt, pose.heading));
  }, []);
  const flyRoad = useCallback((pose: AtlasCameraPose, durationMs: number) => {
    mapRef.current?.flyCameraTo({ durationMillis: durationMs, endCamera: targetCamera(
      pose.center.latitude, pose.center.longitude, pose.center.altitude, pose.range, pose.tilt, pose.heading,
    ) });
  }, []);
  const roadFlight = useAtlasRoadFlight({
    active: Boolean(ready) && roadTourActive && !streetRequested,
    route: atlasRoute, controller: playbackController,
    elevation: groundElevation,
    width: mapWidth,
    render: renderRoad, fly: flyRoad, autoPlay: autoPlayApproach,
  });
  const exitStreet = useCallback(() => {
    if (atlasRoute) {
      const position = roadTour.position();
      if (position) roadFlight.seek(projectStreetHandoff(atlasRoute, position, position.heading).distanceAlongM, position.heading);
      else roadFlight.seek(roadFlight.position(), mapRef.current?.heading ?? 0);
    }
    playbackController.cancel('settled');
    setStreetRequested(false);
  }, [atlasRoute, playbackController, roadFlight, roadTour]);
  useEffect(() => {
    const escape = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && streetRequested) exitStreet();
    };
    document.addEventListener('keydown', escape);
    return () => document.removeEventListener('keydown', escape);
  }, [exitStreet, streetRequested]);
  const roadPlaybackCanPause = playbackState === "preparing" || playbackState === "playing";
  const roadPlaybackCanResume = playbackState === "paused";
  const showRoadPlaybackControls = roadTourActive && Boolean(atlasRoute);
  const selectedSecondarySociety = secondarySocieties.find((candidate) =>
    candidate.societyId === selectedSecondarySocietyId) ?? null;
  const hadSecondarySelectionRef = useRef(false);
  const previousPlaybackStateRef = useRef(playbackState);

  useEffect(() => {
    let cancelled = false;
    let unregisterStopper: () => void = () => {};
    void Promise.all([
      loadGoogleMaps3dLibrary(),
      loadGoogleMarkerLibrary(),
      loadGoogleTerrainElevation(home.latitude, home.longitude),
    ])
      .then(([loaded, loadedMarkerLibrary, terrainElevation]) => {
        if (cancelled || !containerRef.current) return;
        const library = loaded as Maps3DLibrary;
        const markerLibrary = loadedMarkerLibrary as MarkerLibrary;
        terrainElevationRef.current = terrainElevation;
        setGroundElevation(terrainElevation);
        const initialCamera = initialSocietyAutoPlayRef.current
          ? societyComposition?.start
          : societyComposition?.final;
        const map = new library.Map3DElement({
          center: {
            lat: societyComposition?.center.latitude ?? home.latitude,
            lng: societyComposition?.center.longitude ?? home.longitude,
            altitude: terrainElevation,
          },
          defaultUIHidden: true,
          gestureHandling: "COOPERATIVE",
          heading: initialCamera?.heading ?? DEFAULT_HEADING,
          mode: "SATELLITE",
          range: initialCamera?.range ?? HOME_PORTRAIT_RANGE_M,
          tilt: initialCamera?.tilt ?? 0,
        });
        libraryRef.current = library;
        markerLibraryRef.current = markerLibrary;
        mapRef.current = map;
        map.addEventListener('gmp-steadychange', event => {
          map.dataset.googleSteady = String((event as Event & {isSteady:boolean}).isSteady);
        });
        map.addEventListener('gmp-error', () => {
          if (!cancelled) setLoadError(new Error('google_maps_3d_unavailable'));
        });
        unregisterStopper = playbackController.registerStopper(() => {
          map.stopCameraAnimation();
        });
        containerRef.current.replaceChildren(map);
        setReady(epoch => epoch + 1);
      })
      .catch((error: unknown) => {
        if (import.meta.env.DEV) {
          console.warn("[PropertyArrivalGoogle3DMap] Google 3D failed to load", error);
        }
        if (!cancelled) {
          setLoadError(error instanceof Error ? error : new Error("google_maps_3d_unavailable"));
        }
      });
    return () => {
      cancelled = true;
      unregisterStopper();
      cameraMoveRef.current += 1;
      for (const child of childrenRef.current) child.remove();
      childrenRef.current = [];
      mapRef.current?.remove();
      mapRef.current = null;
      libraryRef.current = null;
      markerLibraryRef.current = null;
      terrainElevationRef.current = null;
    };
  }, [
    home.latitude,
    home.longitude,
    playbackController,
    societyComposition,
  ]);

  useEffect(() => {
    const map = mapRef.current;
    if (!map || !ready || terrainElevationRef.current === null) return;
    if (selectedSecondarySociety) {
      hadSecondarySelectionRef.current = true;
      const camera = targetCamera(
        selectedSecondarySociety.home.latitude,
        selectedSecondarySociety.home.longitude,
        terrainElevationRef.current,
        HOME_PORTRAIT_RANGE_M,
        HOME_PORTRAIT_TILT,
        DEFAULT_HEADING,
      );
      void map.flyCameraTo({ endCamera: camera, durationMillis: EVIDENCE_CAMERA_DURATION_MS });
      return;
    }
    if (!hadSecondarySelectionRef.current) return;
    hadSecondarySelectionRef.current = false;
    const primary = societyComposition?.final;
    if (!primary) return;
    const camera = targetCamera(
      societyComposition.center.latitude,
      societyComposition.center.longitude,
      terrainElevationRef.current,
      primary.range,
      primary.tilt,
      primary.heading,
    );
    void map.flyCameraTo({ endCamera: camera, durationMillis: EVIDENCE_CAMERA_DURATION_MS });
  }, [ready, selectedSecondarySociety, societyComposition]);

  useEffect(() => {
    if (!mapRef.current) return;
    mapRef.current.gestureHandling = expanded ? "GREEDY" : "COOPERATIVE";
  }, [expanded]);

  useEffect(() => {
    const map = mapRef.current;
    if (!map || !ready || terrainElevationRef.current === null) return;
    const previouslyAutoPlaying = previousSocietyAutoPlayRef.current;
    previousSocietyAutoPlayRef.current = autoPlaySociety;
    if (
      cameraMode === "home"
      && !terrainCorridor
      && societyComposition
      && arrivalExperience
    ) {
      const finalCamera = targetCamera(
        societyComposition.center.latitude,
        societyComposition.center.longitude,
        terrainElevationRef.current,
        societyComposition.final.range,
        societyComposition.final.tilt,
        societyComposition.final.heading,
      );
      const startCamera = targetCamera(
        societyComposition.center.latitude,
        societyComposition.center.longitude,
        terrainElevationRef.current,
        societyComposition.start.range,
        societyComposition.start.tilt,
        societyComposition.start.heading,
      );
      const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
      if (!autoPlaySociety || reducedMotion) {
        if (reducedMotion || !previouslyAutoPlaying) {
          settleCameraFraming(map, finalCamera);
          playbackController.cancel("settled");
        }
        return;
      }
      settleCameraFraming(map, startCamera);
      const run = playbackController.begin("revealing");
      if (!run.activate()) return;
      void map.flyCameraTo({
        endCamera: finalCamera,
        durationMillis: arrivalExperience.revealDurationMs,
      });
      void run.wait(arrivalExperience.revealDurationMs).then((timerCompleted) => {
        if (!timerCompleted || !run.isCurrent()) return;
        settleCameraFraming(map, finalCamera);
        run.settle();
      });
      return () => {
        if (run.isCurrent()) playbackController.cancel("settled");
      };
    }
    // The Atlas road driver owns this camera until the road view is left.
    if (roadTourActive || cameraMode === 'evidence') return;
    const range = HOME_PORTRAIT_RANGE_M;
    const tilt = above ? policy.above.tilt : HOME_PORTRAIT_TILT;
    const heading = roadLandingFocus?.heading ?? DEFAULT_HEADING;
    const moveId = cameraMoveRef.current + 1;
    cameraMoveRef.current = moveId;
    const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    const durationMillis = reducedMotion
      ? 0
      : HOME_CAMERA_DURATION_MS;
    void loadGoogleTerrainElevation(cameraLatitude, cameraLongitude)
      .then((terrainElevation) => {
        if (cameraMoveRef.current !== moveId || mapRef.current !== map) return;
        const camera = targetCamera(
            cameraLatitude,
            cameraLongitude,
            terrainElevation,
            range,
            tilt,
            heading,
          );
        map.flyCameraTo({ endCamera: camera, durationMillis });
      })
      .catch((error: unknown) => {
        if (import.meta.env.DEV) {
          console.warn("[PropertyArrivalGoogle3DMap] Terrain-safe camera move failed", error);
        }
      });
  }, [
    cameraMode,
    selectedPlaceId,
    above,
    cameraLatitude,
    cameraLongitude,
    accessLines,
    ready,
    arrivalExperience,
    autoPlaySociety,
    playbackController,
    roadExperience,
    roadLandingFocus,
    roadTourActive,
    societyPlaybackVersion,
    societyComposition,
    terrainCorridor,
    viewport.radiusKm,
  ]);

  useEffect(() => {
    const previousState = previousPlaybackStateRef.current;
    previousPlaybackStateRef.current = playbackState;
    const map = mapRef.current;
    if (previousState !== "paused" || !map) return;
    const remainingMs = playbackController.remainingWaitMs();
    if (
      playbackState === "revealing"
      && terrainElevationRef.current !== null
      && societyComposition
    ) {
      const finalCamera = targetCamera(
        societyComposition.center.latitude,
        societyComposition.center.longitude,
        terrainElevationRef.current,
        societyComposition.final.range,
        societyComposition.final.tilt,
        societyComposition.final.heading,
      );
      void map.flyCameraTo({
        endCamera: finalCamera,
        durationMillis: remainingMs,
      });
      return;
    }
  }, [
    playbackController,
    playbackState,
    societyComposition,
  ]);

  useEffect(() => {
    const map = mapRef.current;
    if (!map || !ready) return undefined;
    const cancelAutomaticCamera = () => {
      cameraMoveRef.current += 1;
      playbackController.cancel("settled");
      onPlaybackCancelled?.();
    };
    const visibilityChanged = () => {
      if (document.hidden) cancelAutomaticCamera();
    };
    map.addEventListener("pointerdown", cancelAutomaticCamera);
    map.addEventListener("touchstart", cancelAutomaticCamera, { passive: true });
    map.addEventListener("wheel", cancelAutomaticCamera, { passive: true });
    document.addEventListener("visibilitychange", visibilityChanged);
    return () => {
      map.removeEventListener("pointerdown", cancelAutomaticCamera);
      map.removeEventListener("touchstart", cancelAutomaticCamera);
      map.removeEventListener("wheel", cancelAutomaticCamera);
      document.removeEventListener("visibilitychange", visibilityChanged);
    };
  }, [onPlaybackCancelled, playbackController, ready]);

  useEffect(() => {
    const map = mapRef.current;
    const library = libraryRef.current;
    const markerLibrary = markerLibraryRef.current;
    if (!ready || !map || !library || !markerLibrary) return;
    for (const child of childrenRef.current) child.remove();
    const nextChildren: Map3DChild[] = [];

    if (showBoundary && home.boundary) {
      addPolygon(
        map,
        library,
        home.boundary,
        policy.boundary,
        nextChildren,
      );
    }
    const selected = places.find(place => (place.feature_id ?? place.name) === selectedPlaceId);
    const activePolygons = selected ? geometryForPlace(selected, polygons ?? [], contextLines).polygons : polygons ?? [];
    for (const line of contextLines) {
      const style = (policy.geometryStyles as Record<string, typeof policy.boundary>)[line.kind] ?? policy.boundary;
      addLine(map, library, line, {color:style.stroke,width:4,outerColor:'#ffffff66',outerWidth:0.3,drawsOccludedSegments:false}, null, nextChildren);
    }
    for (const polygon of polygons ?? []) {
      const style = (policy.geometryStyles as Record<string, typeof policy.boundary>)[polygon.kind] ?? policy.boundary;
      addPolygon(map, library, polygon,
        {...style, fill: activePolygons.includes(polygon) ? style.fill : '#ffffff06'}, nextChildren);
    }
    if (quiet && home.boundary) {
      const mask = new library.Polygon3DElement({ altitudeMode: 'CLAMP_TO_GROUND',
        fillColor: policy.quiet.fill, strokeColor: '#00000000', strokeWidth: 0 });
      const r = policy.quiet.radiusDegrees;
      mask.path = [
        {lat: home.latitude-r, lng: home.longitude-r}, {lat: home.latitude-r, lng: home.longitude+r},
        {lat: home.latitude+r, lng: home.longitude+r}, {lat: home.latitude+r, lng: home.longitude-r},
        {lat: home.latitude-r, lng: home.longitude-r},
      ];
      const quietPlaces = selectedPlaceId
        ? places.filter((place) => (place.feature_id ?? place.name) === selectedPlaceId)
        : places;
      mask.innerPaths = [
        pathFromPolygon(home.boundary).reverse(),
        ...activePolygons.map((polygon) => pathFromPolygon(polygon).reverse()),
        ...quietPlaces.map((place) => circlePath(place.latitude, place.longitude).reverse()),
      ];
      map.append(mask); nextChildren.push(mask);
    }
    for (const society of secondarySocieties) {
      if (society.home.boundary) {
        addPolygon(
          map,
          library,
          society.home.boundary,
          { fill: "#74849d14", stroke: "#71819999" },
          nextChildren,
        );
      }
      const marker = new library.Marker3DInteractiveElement({
        altitudeMode: "CLAMP_TO_GROUND",
        collisionBehavior: "OPTIONAL_AND_HIDES_LOWER_PRIORITY",
        drawsWhenOccluded: true,
        label: society.societyId === selectedSecondarySocietyId ? society.home.name : undefined,
        position: { lat: society.home.latitude, lng: society.home.longitude },
        title: society.home.name,
      });
      marker.append(new markerLibrary.PinElement(mapMarkerPinOptions("home", "subdued")));
      marker.tabIndex = 0;
      marker.setAttribute("aria-label", society.home.name);
      if (onSelectSecondarySociety) {
        marker.addEventListener("gmp-click", () => onSelectSecondarySociety(society.societyId));
        marker.addEventListener("keydown", (event) => {
          if (event instanceof KeyboardEvent && (event.key === "Enter" || event.key === " ")) {
            event.preventDefault();
            onSelectSecondarySociety(society.societyId);
          }
        });
      }
      map.append(marker);
      nextChildren.push(marker);
    }
    for (const line of accessLines) {
      {
        addLine(
          map,
          library,
          line,
          {
            color: "#48443d",
            width: 5,
            outerColor: "#fffaf0e6",
            outerWidth: 0.65,
            drawsOccludedSegments: true,
          },
          null,
          nextChildren,
        );
        const labelPosition = lineLabelPosition(line);
        if (labelPosition && !roadTourActive) {
          const routeLabel = new library.Marker3DInteractiveElement({
            altitudeMode: "CLAMP_TO_GROUND",
            collisionBehavior: "REQUIRED",
            drawsWhenOccluded: true,
            label: line.name,
            position: labelPosition,
            title: line.name,
          });
          map.append(routeLabel);
          nextChildren.push(routeLabel);
        }
      }
    }
    if (showMetroLines) {
      for (const contextLine of arrivalAtlasContextLines(metroLines, { ...policy.metro, altitudeMode: 'clamp_to_ground' })) {
        const line = { id: contextLine.id, name: '', kind: 'line', source_type: 'OSM',
          coordinates: contextLine.path.map(p => [p.lng, p.lat] as [number, number]) };
        addLine(
          map,
          library,
          line,
          {
            color: contextLine.style.strokeColor,
            width: contextLine.style.strokeWidth,
            outerColor: "#ffffffcc",
            outerWidth: 0.35,
            drawsOccludedSegments: false,
          },
          null,
          nextChildren,
        );
      }
    }
    if (!roadTourActive) {
      const homeIsContext = cameraMode === "evidence";
      const homeMarker = new library.Marker3DInteractiveElement({
        altitudeMode: "CLAMP_TO_GROUND",
        collisionBehavior: "REQUIRED",
        drawsWhenOccluded: true,
        extruded: true,
        label: "This home",
        position: { lat: home.latitude, lng: home.longitude },
        title: home.name,
      });
      if (homeIsContext) {
        homeMarker.append(new markerLibrary.PinElement(mapMarkerPinOptions("home", "subdued")));
      }
      map.append(homeMarker);
      nextChildren.push(homeMarker);
    }

    let activePopover: Popover3DElement | null = null;
    if (selected && cameraMode === 'evidence') {
      const arc = new library.Polyline3DInteractiveElement({
        altitudeMode: 'RELATIVE_TO_GROUND', path: nearbyRelationArc(home, selected),
        strokeColor: '#d6edbcc4', strokeWidth: 3, drawsOccludedSegments: false,
      });
      arc.setAttribute('aria-label', 'Straight-line relationship to home, not a travel route');
      arc.dataset.atlasRelationship = 'true';
      map.append(arc); nextChildren.push(arc);
    }
    for (const place of places) {
      const popover = createPlacePopover(library, place);
      const marker = new library.Marker3DInteractiveElement({
        altitudeMode: "CLAMP_TO_GROUND",
        collisionBehavior: (place.feature_id ?? place.name) === selectedPlaceId || !selectedPlaceId ? "REQUIRED" : "OPTIONAL_AND_HIDES_LOWER_PRIORITY",
        drawsWhenOccluded: true,
        gmpPopoverTargetElement: popover,
        position: { lat: place.latitude, lng: place.longitude },
        title: place.name,
        label: (place.feature_id ?? place.name) === selectedPlaceId ? place.name : undefined,
      });
      marker.append(new markerLibrary.PinElement({ ...mapMarkerPinOptions(place.icon, "active"),
        ...(cameraMode === 'evidence' ? {glyphSrc: undefined, glyphText: String(place.number)} : {}),
      }));
      marker.tabIndex = 0;
      marker.setAttribute("aria-label", place.name);
      marker.addEventListener('gmp-click', () => onSelectPlace?.(place.feature_id ?? place.name));
      marker.addEventListener("pointerenter", () => {
        if (activePopover && activePopover !== popover) activePopover.open = false;
        popover.open = true;
        activePopover = popover;
      });
      map.append(marker);
      map.append(popover);
      nextChildren.push(marker);
      nextChildren.push(popover);
    }
    childrenRef.current = nextChildren;
  }, [
    accessLines,
    cameraMode,
    home.boundary,
    home.latitude,
    home.longitude,
    home.name,
    metroLines,
    places,
    ready,
    secondarySocieties,
    selectedSecondarySocietyId,
    onSelectSecondarySociety,
    showMetroLines,
    roadTourActive,
    quiet,
    showBoundary,
    onSelectPlace,
    polygons,
    contextLines,
    selectedPlaceId,
    home,
  ]);

  useEffect(() => {
    const map = mapRef.current;
    if (!map || !ready || terrainCorridor || roadTourActive || streetRequested || cameraMode !== 'evidence') return;
    const moveId = ++cameraMoveRef.current;
    let cancelled = false;
    let camera: CameraOptions | undefined;
    const move = () => {
      if (!camera || cancelled || cameraMoveRef.current !== moveId || playbackController.snapshot() === 'paused') return;
      map.dataset.atlasDepth = nearbyDepth;
      map.flyCameraTo({durationMillis: window.matchMedia('(prefers-reduced-motion: reduce)').matches ? 0 : policy.focus.durationMs,
        endCamera: camera});
    };
    const unregister = playbackController.registerResumer(move);
    void loadGoogleTerrainElevation(home.latitude, home.longitude).then(elevation => {
      const pose = nearbySceneCamera(home, places, polygons ?? [], [...metroLines, ...contextLines], selectedPlaceId,
        nearbyDepth, elevation, containerRef.current?.clientWidth ?? window.innerWidth);
      camera = {...pose, tilt: above ? policy.above.tilt : pose.tilt};
      move();
    }).catch(() => undefined);
    return () => {cancelled = true; unregister();};
  }, [above, selectedPlaceId, nearbyDepth, places, home, polygons, metroLines, contextLines, cameraMode, terrainCorridor, playbackController, ready, roadTourActive, streetRequested]);

  if (loadError) throw loadError;

  function toggleExpanded() {
    onToggleExpanded();
  }

  return (
    <div
      className={`nearby-map nearby-map--google${expanded ? " is-expanded" : ""}`}
      role="region"
      aria-label="Nearby evidence map"
      aria-busy={!ready}
      data-map-renderer={streetViewReady ? "google-street-view" : "google-3d"}
    >
      <div
        ref={containerRef}
        className={`nearby-map__canvas nearby-map__canvas--google-3d${streetViewReady ? " is-behind-street-view" : ""}`}
        aria-hidden={streetViewReady}
        inert={streetViewReady}
      />
      <div
        ref={streetViewContainerRef}
        className={`nearby-map__canvas nearby-map__canvas--street-view${streetViewReady ? " is-active" : ""}`}
        aria-hidden={!streetViewReady}
        inert={!streetViewReady}
      />
      {roadTourActive && accessLines[0]?.name && (
        <div className="nearby-map__road-title">
          {accessLines[0].name}
          {roadTour.progress && ` · ${roadTour.progress.current}/${roadTour.progress.total}`}
          {roadTour.status && <span aria-live="polite"> · {roadTour.status}</span>}
        </div>
      )}
      {showRoadPlaybackControls ? (
        <div className="nearby-map__playback-controls" role="group" aria-label="Approach road playback">
          {roadPlaybackCanPause || roadPlaybackCanResume ? (
            <button
              type="button"
              aria-label={roadPlaybackCanResume
                ? 'Resume road tour'
                : 'Pause road tour'}
              title={roadPlaybackCanResume
                ? 'Resume road tour'
                : 'Pause road tour'}
              onClick={() => roadPlaybackCanResume
                ? playbackController.resume()
                : playbackController.pause()}
            >
              <span aria-hidden="true">{roadPlaybackCanResume ? "▶" : "Ⅱ"}</span>
              <span>{roadPlaybackCanResume ? "Resume" : "Pause"}</span>
            </button>
          ) : null}
          {!roadPlaybackCanPause && !roadPlaybackCanResume ? (
            <button
              type="button"
              aria-label="Replay road tour"
              onClick={streetRequested ? roadTour.replay : roadFlight.replay}
            >
              <span aria-hidden="true">↻</span>
              <span>Replay</span>
            </button>
          ) : null}
          {!streetRequested && <label className="atlas-road-speed">Speed
            <input aria-label="Road tour speed" type="range" min={policy.road.minimumRate} max={policy.road.maximumRate}
              step="0.25" value={roadFlight.rate} onChange={e => roadFlight.setRate(Number(e.target.value))} />
            <output>{roadFlight.rate}×</output>
          </label>}
          {roadExperience && <button type="button" onClick={() => {
            if (streetRequested) exitStreet();
            else {
              if (atlasRoute) {
                const point = pointAlongRoute(atlasRoute, roadFlight.position());
                let best = 0;
                roadWaypoints.forEach((candidate, i) => {
                  const distance = (p: typeof candidate) => Math.hypot(p.latitude-point.latitude, p.longitude-point.longitude);
                  if (distance(candidate) < distance(roadWaypoints[best])) best = i;
                });
                setStreetStart(best);
              }
              playbackController.cancel('settled'); setStreetRequested(true);
            }
          }}>{streetRequested ? 'Back to aerial' : 'Street View'}</button>}
        </div>
      ) : null}
      {showExpandAction ? (
        <div className="nearby-map__actions">
          <button type="button" onClick={toggleExpanded}>
            {expanded ? "Close map" : "Expand map"}
          </button>
        </div>
      ) : null}
    </div>
  );
}
