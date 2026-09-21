import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
} from "react";
import { useAtlasRoadFlight } from "../../hooks/useAtlasRoadFlight.ts";
import {
  arrivalAtlasRoute,
  arrivalAtlasContextLines,
  lineColorFromName,
} from "../../lib/homeAtlasProjection.ts";
import {
  blendCamera,
  projectStreetHandoff,
  type AtlasCameraPose,
} from "../../lib/atlas/journey.ts";
import { distanceMetres } from "../../lib/atlas/geometry.ts";
import { buildCategoryTour } from "../../lib/atlas/scenes.ts";
import type { AtlasFeature, AtlasScene } from "../../lib/atlas/types.ts";
import policy from "../../lib/atlasPolicy.ts";
import {
  geometryForPlace,
  homeOrbitCamera,
  homeSceneCamera,
  nearbyRelationArc,
  nearbySceneCamera,
  type AtlasSafeFrame,
  type NearbyCameraOrientation,
  type NearbyDepth,
} from '../../lib/atlasNearbyScene.ts';
import {
  loadGoogleMaps3dLibrary,
  loadGoogleMarkerLibrary,
  loadGoogleTerrainElevation,
} from "../../lib/googleMaps3d.ts";
import { mapMarkerPinOptions } from "../../lib/mapMarkerVisual.ts";
import { useGuidedStreetViewTour } from "../../hooks/useGuidedStreetViewTour.ts";
import type { ArrivalPlaybackController } from "../../lib/arrivalPlayback.ts";
import { AtlasCameraArbiter } from "../../lib/atlasCameraArbiter.ts";
import { attachAtlasSpotlight, type SpotlightSubject } from '../../lib/atlasSpotlight.ts';
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

export type NearbyTourChapter = Readonly<{
  categoryId: string;
  view: "nearby" | "metro";
  layerId?: string;
  places: NumberedPlace[];
  polygons: MapOverlayPolygon[];
  lines: MapOverlayLine[];
}>;

export type NearbyTourRequest = Readonly<{
  id: number;
  chapters: readonly NearbyTourChapter[];
}>;

export type NearbyTourSceneState = Readonly<{
  view: "nearby" | "metro";
  layerId?: string;
  selectedPlaceId: string | null;
  depth: NearbyDepth;
}>;

export type AtlasCameraRequest = { id: number; action: "zoom-in" | "zoom-out" | "rotate" };

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
  nearbyCameraOrientation?: NearbyCameraOrientation;
  nearbySelectionVersion?: number;
  onSelectPlace?: (id: string | null) => void;
  above?: boolean;
  quiet?: boolean;
  showBoundary?: boolean;
  polygons?: MapOverlayPolygon[];
  contextLines?: MapOverlayLine[];
  showExpandAction?: boolean;
  drawerOpen?: boolean;
  immersive?: boolean;
  pageScrollable?: boolean;
  cameraRequest?: AtlasCameraRequest | null;
  onReady?: () => void;
  nearbyTransitionMs?: number;
  nearbyOrientationPlaces?: NumberedPlace[];
  nearbyTourRequest?: NearbyTourRequest | null;
  onNearbyTourScene?: (state: NearbyTourSceneState) => void;
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
  flyCameraAround: (options: {
    camera: CameraOptions;
    durationMillis: number;
    repeatCount: number;
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
    fov?: number;
  }) => Map3DElement;
  Marker3DInteractiveElement: new (options: {
    altitudeMode?: "CLAMP_TO_GROUND" | "RELATIVE_TO_GROUND" | "RELATIVE_TO_MESH";
    collisionBehavior?: "REQUIRED" | "OPTIONAL_AND_HIDES_LOWER_PRIORITY";
    drawsWhenOccluded?: boolean;
    extruded?: boolean;
    sizePreserved?: boolean;
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
  }) => Map3DChild & { path: LatLngAltitude[]; innerPaths: LatLngAltitude[][]; strokeColor: string };
  Polyline3DInteractiveElement: new (options: {
    altitudeMode?: "CLAMP_TO_GROUND" | "RELATIVE_TO_GROUND";
    coordinates: LatLngAltitude[];
    drawsOccludedSegments?: boolean;
    outerColor?: string;
    outerWidth?: number;
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

function societyCameraAt(
  home: { latitude: number; longitude: number },
  elevation: number,
  viewportWidth: number,
  progress: number,
): AtlasCameraPose {
  const stages = policy.society.stages;
  const clamped = Math.max(0, Math.min(1, progress));
  const rightIndex = Math.max(1, stages.findIndex((stage) => stage.progress >= clamped));
  const left = stages[rightIndex - 1];
  const right = stages[rightIndex] ?? stages.at(-1)!;
  const segmentProgress = right.progress === left.progress
    ? 1
    : (clamped - left.progress) / (right.progress - left.progress);
  const scale = viewportWidth < policy.road.mobileBreakpointPx
    ? policy.society.mobileRangeScale
    : 1;
  const pose = (stage: (typeof stages)[number]): AtlasCameraPose => ({
    center: {
      latitude: home.latitude,
      longitude: home.longitude,
      altitude: elevation + stage.altitudeOffsetM,
    },
    heading: stage.heading,
    range: stage.rangeM * scale,
    tilt: stage.tilt,
  });
  return blendCamera(pose(left), pose(right), segmentProgress);
}

function societyStageIndex(progress: number): number {
  let index = 0;
  for (let candidate = 1; candidate < policy.society.stages.length; candidate += 1) {
    if (policy.society.stages[candidate].progress > progress) break;
    index = candidate;
  }
  return index;
}

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

function pointsFromCoordinates(coordinates: [number, number][]) {
  return coordinates.map(([lng, lat]) => ({lat, lng}));
}

function lineCoordinates(line: MapOverlayLine): LatLngAltitude[] {
  return line.coordinates.map(([lng, lat]) => ({ lat, lng }));
}

function corridorFootprint(
  lines: MapOverlayLine[],
  radiusM: number,
): LatLngAltitude[] | null {
  const line = lines.reduce<MapOverlayLine | null>((longest, candidate) =>
    !longest || candidate.coordinates.length > longest.coordinates.length ? candidate : longest, null);
  if (!line || line.coordinates.length < 2) return null;
  const offsetPoint = (index: number, side: 1 | -1): LatLngAltitude => {
    const [lng, lat] = line.coordinates[index];
    const [previousLng, previousLat] = line.coordinates[Math.max(0, index - 1)];
    const [nextLng, nextLat] = line.coordinates[Math.min(line.coordinates.length - 1, index + 1)];
    const longitudeScale = 111_320 * Math.max(0.2, Math.cos(lat * Math.PI / 180));
    const east = (nextLng - previousLng) * longitudeScale;
    const north = (nextLat - previousLat) * 111_320;
    const length = Math.max(0.001, Math.hypot(east, north));
    const offsetEast = -north / length * radiusM * side;
    const offsetNorth = east / length * radiusM * side;
    return {
      lat: lat + offsetNorth / 111_320,
      lng: lng + offsetEast / longitudeScale,
    };
  };
  const left = line.coordinates.map((_, index) => offsetPoint(index, 1));
  const right = line.coordinates.map((_, index) => offsetPoint(index, -1)).reverse();
  return [...left, ...right, left[0]];
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
  colors: { fill: string; stroke: string; strokeWidth?: number },
  children: Map3DChild[],
) {
  const element = new library.Polygon3DElement({
    altitudeMode: "CLAMP_TO_GROUND",
    drawsOccludedSegments: false,
    fillColor: colors.fill,
    strokeColor: colors.stroke,
    strokeWidth: colors.strokeWidth ?? 2,
  });
  element.path = pathFromPolygon(polygon);
  element.dataset.atlasPolygonId = polygon.id;
  element.dataset.atlasPolygonKind = polygon.kind;
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
    coordinates: lineCoordinates(line),
    drawsOccludedSegments: style.drawsOccludedSegments,
    outerColor: style.outerColor,
    outerWidth: style.outerWidth,
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
    nearbyCameraOrientation = 'category-stable',
    nearbySelectionVersion = 0,
    onSelectPlace,
    above = false,
    quiet = false,
    showBoundary = true,
    polygons,
    contextLines = EMPTY_CONTEXT_LINES,
    showExpandAction = true,
    drawerOpen = false,
    immersive = false,
    pageScrollable = false,
    cameraRequest = null,
    onReady,
    nearbyTransitionMs = policy.focus.durationMs,
    nearbyOrientationPlaces = places,
    nearbyTourRequest = null,
    onNearbyTourScene,
  } = props;
  const containerRef = useRef<HTMLDivElement | null>(null);
  const streetViewContainerRef = useRef<HTMLDivElement | null>(null);
  const mapRef = useRef<Map3DElement | null>(null);
  const libraryRef = useRef<Maps3DLibrary | null>(null);
  const markerLibraryRef = useRef<MarkerLibrary | null>(null);
  const childrenRef = useRef<Map3DChild[]>([]);
  const cameraMoveRef = useRef(0);
  const manualCameraRef = useRef(false);
  const cameraArbiterRef = useRef(new AtlasCameraArbiter());
  const executedTourRequestRef = useRef<number | null>(null);
  const onNearbyTourSceneRef = useRef(onNearbyTourScene);
  useEffect(() => {
    onNearbyTourSceneRef.current = onNearbyTourScene;
  }, [onNearbyTourScene]);
  const terrainElevationRef = useRef<number | null>(null);
  const initialSocietyAutoPlayRef = useRef(autoPlaySociety);
  const previousSocietyAutoPlayRef = useRef(autoPlaySociety);
  // An epoch ensures overlays/cameras also attach after a scene-driven map rebuild.
  const [ready, setReady] = useState(0);
  useEffect(() => { if (ready) onReady?.(); }, [ready, onReady]);
  const [groundElevation, setGroundElevation] = useState(0);
  const [mapWidth, setMapWidth] = useState(1000);
  const [safeFrame, setSafeFrame] = useState<AtlasSafeFrame | undefined>();
  const safeFrameRef = useRef<AtlasSafeFrame | undefined>(undefined);
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    const observer = new ResizeObserver(entries => setMapWidth(entries[0].contentRect.width));
    observer.observe(container);
    return () => observer.disconnect();
  }, []);
  useLayoutEffect(() => {
    const container = containerRef.current;
    const shell = container?.closest<HTMLElement>('.property-arrival-map--atlas');
    if (!container || !shell) return undefined;
    const measure = () => {
      const mapRect = container.getBoundingClientRect();
      const margin = policy.cameraFit.safeMarginPx;
      const rect = (selector: string) => shell.querySelector(selector)?.getBoundingClientRect();
      const identity = rect('.property-atlas__identity');
      const actions = rect('.property-atlas__actions');
      const setInset = (name: string, value: number) => {
        const pixels = `${Math.ceil(value)}px`;
        if (shell.style.getPropertyValue(name) !== pixels) shell.style.setProperty(name, pixels);
      };
      setInset('--atlas-navigation-top', Math.max(
        (identity?.bottom ?? mapRect.top) - mapRect.top,
        (actions?.bottom ?? mapRect.top) - mapRect.top,
      ) + 24);
      const dock = rect('.property-atlas__dock');
      setInset('--atlas-drawer-top', (dock?.bottom ?? mapRect.top) - mapRect.top + 24);
      const drawer = drawerOpen ? rect('.property-atlas__drawer') : undefined;
      const tools = rect('.property-atlas__view-tools');
      const sidebar = document.querySelector('.workspace-sidebar')?.getBoundingClientRect();
      const drawerIsBottomSheet = Boolean(drawer && drawer.width >= mapRect.width * 0.7);
      // The identity occupies only the upper-left corner. Treating it as a
      // full-height exclusion leaves a sliver of map and forces a huge zoom
      // out. Pair focus can safely use the canvas below it.
      const left = sidebar && sidebar.right > mapRect.left && sidebar.left < mapRect.right
        ? Math.max(margin, sidebar.right - mapRect.left + margin)
        : margin;
      const toolsRight = tools ? Math.max(margin, mapRect.right - tools.left + margin) : margin;
      const right = drawer && !drawerIsBottomSheet && drawer.left < mapRect.right
        ? Math.max(0, mapRect.right - drawer.left + margin)
        : toolsRight;
      const top = Math.max(margin, (identity?.bottom ?? mapRect.top) - mapRect.top + margin,
        (actions?.bottom ?? mapRect.top) - mapRect.top + margin,
        (dock?.bottom ?? mapRect.top) - mapRect.top + margin);
      const drawerBottom = drawer && drawerIsBottomSheet && drawer.top < mapRect.bottom
        ? Math.max(0, mapRect.bottom - drawer.top + margin)
        : margin;
      const bottom = Math.max(margin, drawerBottom);
      const next = {
        width: mapRect.width,
        height: mapRect.height,
        left: Math.min(left, Math.max(margin, mapRect.width - right - 160)),
        right: Math.min(right, Math.max(margin, mapRect.width - left - 160)),
        top: Math.min(top, Math.max(margin, mapRect.height - bottom - 120)),
        bottom: Math.min(bottom, Math.max(margin, mapRect.height - top - 120)),
      };
      safeFrameRef.current = next;
      if (mapRef.current) mapRef.current.dataset.atlasSafeFrame = JSON.stringify(next);
      setSafeFrame((current) => current
        && Object.keys(next).every((key) => current[key as keyof AtlasSafeFrame] === next[key as keyof AtlasSafeFrame])
        ? current
        : next);
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(container);
    observer.observe(shell);
    const panel = shell.querySelector('.property-atlas__drawer');
    if (panel) observer.observe(panel);
    for (const selector of ['.property-atlas__identity', '.property-atlas__actions', '.property-atlas__dock', '.property-atlas__view-tools']) {
      const element = shell.querySelector(selector);
      if (element) observer.observe(element);
    }
    const sidebar = document.querySelector('.workspace-sidebar');
    if (sidebar) observer.observe(sidebar);
    return () => observer.disconnect();
  }, [drawerOpen]);
  const [loadError, setLoadError] = useState<Error | null>(null);
  const [streetRequested, setStreetRequested] = useState(false);
  const homeLatitude = home.latitude;
  const homeLongitude = home.longitude;
  const roadExperience = layerExperience?.kind === "street_view_tour"
    ? layerExperience
    : null;
  const approachViewActive = terrainCorridor && cameraMode === "evidence";
  useLayoutEffect(() => {
    cameraArbiterRef.current.activate(approachViewActive
      ? "road"
      : cameraMode === "evidence"
      ? "nearby"
      : "society");
  }, [approachViewActive, cameraMode]);
  const atlasRoute = useMemo(
    () => arrivalAtlasRoute(accessLines, roadExperience?.routeDirection ?? "as-mapped"),
    [accessLines, roadExperience?.routeDirection],
  );
  const corridorViewActive = approachViewActive && Boolean(atlasRoute);
  const roadFocus = useMemo(
    () => approachViewActive
      ? corridorCameraFocus(accessLines, {
        latitude: homeLatitude,
        longitude: homeLongitude,
      })
      : null,
    [accessLines, approachViewActive, homeLatitude, homeLongitude],
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
  const streetViewWaypoints = useMemo(
    () => roadWaypoints.length > 0
      ? roadWaypoints
      : approachViewActive && entranceAnchor
      ? [{
          ...entranceAnchor,
          anchorOffsetM: 0,
          heading: DEFAULT_HEADING,
          offsetM: 0,
        }]
      : [],
    [approachViewActive, entranceAnchor, roadWaypoints],
  );
  const roadLandingFocus = useMemo(
    () => roadWaypoints[0]
      ?? roadFocus
      ?? (approachViewActive && entranceAnchor
        ? { ...entranceAnchor, heading: DEFAULT_HEADING }
        : null),
    [approachViewActive, entranceAnchor, roadFocus, roadWaypoints],
  );
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
    active: streetViewWaypoints.length > 0 && streetRequested,
    anchor: entranceAnchor,
    // Choosing Street View starts the complete home-arrival journey, even if
    // the separate aerial tour was interrupted or had already passed the gate.
    autoPlay: true,
    containerRef: streetViewContainerRef,
    experience: roadExperience,
    interiorAnchor: societyInteriorAnchor,
    onPlaybackCancelled,
    playbackController,
    waypoints: streetViewWaypoints,
  });
  const playbackState = useSyncExternalStore(
    playbackController.subscribe,
    playbackController.snapshot,
    playbackController.snapshot,
  );
  const streetViewReady = roadTour.active;
  const renderRoad = useCallback((pose: AtlasCameraPose) => {
    const map = mapRef.current;
    if (map) cameraArbiterRef.current.submit('road', () => {
      settleCameraFraming(map, targetCamera(pose.center.latitude, pose.center.longitude,
        pose.center.altitude, pose.range, pose.tilt, pose.heading));
      map.dataset.atlasCameraOwner = 'road';
    });
  }, []);
  const flyRoad = useCallback((pose: AtlasCameraPose, durationMs: number) => {
    const map = mapRef.current;
    if (!map) return;
    cameraArbiterRef.current.submit('road', () => {
      map.dataset.atlasCameraOwner = 'road';
      map.flyCameraTo({ durationMillis: durationMs, endCamera: targetCamera(
        pose.center.latitude, pose.center.longitude, pose.center.altitude, pose.range, pose.tilt, pose.heading,
      ) });
    });
  }, []);
  const traceRoadProgress = useCallback((distanceM: number, routeLengthM: number, heading: number) => {
    const map = mapRef.current;
    if (!map) return;
    map.dataset.atlasRoadDistance = distanceM.toFixed(2);
    map.dataset.atlasRoadLength = routeLengthM.toFixed(2);
    map.dataset.atlasHeading = heading.toFixed(2);
  }, []);
  const traceRoadPhase = useCallback((phase: "context" | "descent" | "flight" | "settled") => {
    const map = mapRef.current;
    if (!map) return;
    map.dataset.atlasCameraOwner = 'road';
    map.dataset.atlasScene = `road:${phase}`;
  }, []);
  const roadFlight = useAtlasRoadFlight({
    active: Boolean(ready) && approachViewActive && !streetRequested,
    route: atlasRoute, controller: playbackController,
    elevation: groundElevation,
    width: mapWidth,
    render: renderRoad, fly: flyRoad, autoPlay: autoPlayApproach,
    onProgress: traceRoadProgress,
    onPhase: traceRoadPhase,
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
  const showRoutePlaybackControls = corridorViewActive;
  const showArrivalViewControls = approachViewActive
    && Boolean(roadExperience)
    && (Boolean(atlasRoute) || Boolean(entranceAnchor));
  const selectedSecondarySociety = secondarySocieties.find((candidate) =>
    candidate.societyId === selectedSecondarySocietyId) ?? null;
  const hadSecondarySelectionRef = useRef(false);

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
        const manualStageIndex = Math.max(0, policy.society.stages.findIndex(
          (stage) => stage.id === policy.society.manualStageId,
        ));
        const initialProgress = initialSocietyAutoPlayRef.current
          ? 0
          : policy.society.stages[manualStageIndex].progress;
        const initialCamera = societyCameraAt(
          { latitude: homeLatitude, longitude: homeLongitude },
          terrainElevation,
          containerRef.current.clientWidth || window.innerWidth,
          initialProgress,
        );
        const openingCamera = immersive && !initialSocietyAutoPlayRef.current && safeFrameRef.current
          ? homeSceneCamera({latitude: homeLatitude, longitude: homeLongitude,
            name: home.name, boundary: home.boundary}, terrainElevation, safeFrameRef.current)
          : targetCamera(initialCamera.center.latitude, initialCamera.center.longitude,
            initialCamera.center.altitude, initialCamera.range, initialCamera.tilt, initialCamera.heading);
        const map = new library.Map3DElement({
          center: openingCamera.center!,
          defaultUIHidden: true,
          gestureHandling: immersive && !pageScrollable ? "GREEDY" : "COOPERATIVE",
          heading: openingCamera.heading,
          mode: "SATELLITE",
          range: openingCamera.range,
          tilt: openingCamera.tilt,
          fov: openingCamera.fov,
        });
        libraryRef.current = library;
        markerLibraryRef.current = markerLibrary;
        mapRef.current = map;
        if (safeFrameRef.current) map.dataset.atlasSafeFrame = JSON.stringify(safeFrameRef.current);
        let settled = false;
        map.addEventListener('gmp-steadychange', event => {
          const isSteady = (event as Event & {isSteady:boolean}).isSteady;
          map.dataset.googleSteady = String(isSteady);
          if (isSteady && !settled) {
            settled = true;
            map.dataset.googleInitialized = 'true';
            setReady(epoch => epoch + 1);
          }
        });
        map.addEventListener('gmp-error', () => {
          if (!cancelled) setLoadError(new Error('google_maps_3d_unavailable'));
        });
        unregisterStopper = playbackController.registerStopper(() => {
          map.stopCameraAnimation();
        });
        containerRef.current.replaceChildren(map);
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
    home.name,
    home.boundary,
    immersive,
    pageScrollable,
    homeLatitude,
    homeLongitude,
    playbackController,
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
      cameraArbiterRef.current.submit('society', () => {
        map.flyCameraTo({ endCamera: camera, durationMillis: EVIDENCE_CAMERA_DURATION_MS });
      });
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
    cameraArbiterRef.current.submit('society', () => {
      map.flyCameraTo({ endCamera: camera, durationMillis: EVIDENCE_CAMERA_DURATION_MS });
    });
  }, [ready, selectedSecondarySociety, societyComposition]);

  useEffect(() => {
    if (!mapRef.current) return;
    mapRef.current.gestureHandling = expanded || (immersive && !pageScrollable) ? "GREEDY" : "COOPERATIVE";
  }, [expanded, immersive, pageScrollable]);

  useEffect(() => {
    const map = mapRef.current;
    if (!map || !ready || terrainElevationRef.current === null) return;
    const previouslyAutoPlaying = previousSocietyAutoPlayRef.current;
    previousSocietyAutoPlayRef.current = autoPlaySociety;
    if (selectedSecondarySociety) return;
    if (immersive && cameraMode === 'home' && !terrainCorridor && safeFrame) {
      const homeSubject = {latitude: homeLatitude, longitude: homeLongitude,
        name: home.name, boundary: home.boundary};
      const openingCamera = homeSceneCamera(homeSubject, terrainElevationRef.current, safeFrame,
        above ? policy.above.tilt : undefined);
      const applyOpening = () => cameraArbiterRef.current.submit('society', () => {
        map.stopCameraAnimation();
        settleCameraFraming(map, openingCamera);
        map.dataset.atlasCameraOwner = 'society';
        map.dataset.atlasScene = 'society:opening';
        map.dataset.atlasHomePhase = 'opening';
        map.dataset.atlasSafeFrame = JSON.stringify(safeFrame);
      });
      const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
      if (!autoPlaySociety || reducedMotion) {
        if (reducedMotion || above || !previouslyAutoPlaying) applyOpening();
        if (playbackController.snapshot() === 'idle') playbackController.cancel('settled');
        return;
      }

      applyOpening();
      const orbitCamera = homeOrbitCamera(homeSubject, terrainElevationRef.current, safeFrame);
      const run = playbackController.begin('playing');
      if (!run.activate()) return;
      let phase: 'opening' | 'orbit' = 'opening';
      const startOrbit = () => {
        if (!run.isCurrent()) return;
        phase = 'orbit';
        cameraArbiterRef.current.submit('society', () => {
          map.dataset.atlasScene = 'society:orbit';
          map.dataset.atlasHomePhase = 'orbit';
          map.flyCameraAround({
            camera: {...orbitCamera, heading: map.heading},
            durationMillis: policy.homeOrbit.cycleMs,
            repeatCount: Number.POSITIVE_INFINITY,
          });
        });
      };
      const stop = playbackController.registerStopper(() => {
        map.stopCameraAnimation();
      });
      const resume = playbackController.registerResumer(() => {
        if (!run.isCurrent()) return;
        if (phase === 'orbit') startOrbit();
      });
      void (async () => {
        if (!(await run.wait(policy.homeOrbit.openingHoldMs)) || !run.isCurrent()) return;
        startOrbit();
      })();
      return () => {
        stop();
        resume();
        if (run.isCurrent()) playbackController.cancel('settled');
      };
    }
    if (
      cameraMode === "home"
      && !terrainCorridor
      && societyComposition
      && arrivalExperience
    ) {
      const elevation = terrainElevationRef.current;
      const width = containerRef.current?.clientWidth ?? window.innerWidth;
      const manualStageIndex = Math.max(0, policy.society.stages.findIndex(
        (stage) => stage.id === policy.society.manualStageId,
      ));
      const manualProgress = policy.society.stages[manualStageIndex].progress;
      const applySocietyPose = (progress: number) => {
        const pose = societyCameraAt(
          { latitude: homeLatitude, longitude: homeLongitude },
          elevation,
          width,
          progress,
        );
        cameraArbiterRef.current.submit('society', () => {
          settleCameraFraming(map, targetCamera(
            pose.center.latitude,
            pose.center.longitude,
            pose.center.altitude,
            pose.range,
            above ? policy.above.tilt : pose.tilt,
            pose.heading,
          ));
          const stage = policy.society.stages[societyStageIndex(progress)];
          map.dataset.atlasCameraOwner = 'society';
          map.dataset.atlasScene = `society:${stage.id}`;
          map.dataset.atlasProgress = progress.toFixed(4);
        });
      };
      const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
      if (!autoPlaySociety || reducedMotion) {
        if (reducedMotion || !previouslyAutoPlaying) {
          applySocietyPose(manualProgress);
          playbackController.cancel("settled");
        }
        return;
      }
      const run = playbackController.begin("revealing");
      if (!run.activate()) return;
      let frame = 0;
      let previousFrame: number | null = null;
      let elapsedMs = 0;
      let filmStarted = false;
      const tick = (now: number) => {
        if (!run.isCurrent() || playbackController.snapshot() !== 'revealing') return;
        if (previousFrame !== null) elapsedMs += Math.min(80, now - previousFrame);
        previousFrame = now;
        const progress = Math.min(1, elapsedMs / policy.society.durationMs);
        applySocietyPose(progress);
        if (progress >= 1) {
          run.settle();
          return;
        }
        frame = requestAnimationFrame(tick);
      };
      const stop = playbackController.registerStopper(() => {
        cancelAnimationFrame(frame);
        previousFrame = null;
      });
      const resume = playbackController.registerResumer(() => {
        if (!run.isCurrent()) return;
        if (filmStarted) frame = requestAnimationFrame(tick);
      });
      map.dataset.atlasCameraOwner = 'society';
      map.dataset.atlasScene = 'society:settle';
      const startPose = societyCameraAt(
        { latitude: homeLatitude, longitude: homeLongitude },
        elevation,
        width,
        0,
      );
      cameraArbiterRef.current.submit('society', () => {
        map.flyCameraTo({
          endCamera: targetCamera(
            startPose.center.latitude,
            startPose.center.longitude,
            startPose.center.altitude,
            startPose.range,
            startPose.tilt,
            startPose.heading,
          ),
          durationMillis: policy.society.settleMs,
        });
      });
      void (async () => {
        if (!(await run.wait(policy.society.settleMs)) || !run.isCurrent()) return;
        applySocietyPose(0);
        filmStarted = true;
        frame = requestAnimationFrame(tick);
      })();
      return () => {
        cancelAnimationFrame(frame);
        stop();
        resume();
        if (run.isCurrent()) playbackController.cancel("settled");
      };
    }
    // The Atlas road driver owns this camera until the road view is left.
    if (approachViewActive || cameraMode === 'evidence') return;
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
        cameraArbiterRef.current.submit('society', () => {
          map.flyCameraTo({ endCamera: camera, durationMillis });
        });
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
    approachViewActive,
    societyPlaybackVersion,
    societyComposition,
    terrainCorridor,
    homeLatitude,
    homeLongitude,
    viewport.radiusKm,
    home.name,
    home.boundary,
    immersive,
    safeFrame,
    drawerOpen,
    selectedSecondarySociety,
  ]);

  useEffect(() => {
    const map = mapRef.current;
    if (!map || !ready) return undefined;
    const cancelAutomaticCamera = () => {
      manualCameraRef.current = true;
      cameraMoveRef.current += 1;
      playbackController.cancel("settled");
      onPlaybackCancelled?.();
    };
    const visibilityChanged = () => {
      if (document.hidden) playbackController.pause();
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
    manualCameraRef.current = false;
  }, [selectedPlaceId, nearbyDepth, nearbySelectionVersion, cameraMode, above, societyPlaybackVersion, nearbyTourRequest, places]);

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
        policy.homeBoundary,
        nextChildren,
      );
    }
    const selected = places.find(place => (place.id) === selectedPlaceId);
    const isolatesSelection = Boolean(selected) && nearbyDepth !== 'overview';
    const visibleContextLines = contextLines;
    const visiblePolygons = polygons ?? [];
    for (const line of visibleContextLines) {
      const style = (policy.geometryStyles as Record<string, typeof policy.boundary>)[line.kind] ?? policy.boundary;
      addLine(map, library, line, {color:style.stroke,width:4,outerColor:'#ffffff66',outerWidth:0.3,drawsOccludedSegments:false}, null, nextChildren);
    }
    for (const polygon of visiblePolygons) {
      const style = (policy.geometryStyles as Record<string, typeof policy.boundary>)[polygon.kind] ?? policy.boundary;
      addPolygon(map, library, polygon, style, nextChildren);
    }
    const illuminatedPlaces = corridorViewActive || nearbyDepth === 'home' ? []
      : isolatesSelection && selected ? [selected] : cameraMode === 'evidence' ? places : [];
    const subjects: SpotlightSubject[] = [{ anchor: {lat: home.latitude, lng: home.longitude},
      extent: home.boundary ? pathFromPolygon(home.boundary) : undefined, emphasis: 'home' },
      ...illuminatedPlaces.map((place): SpotlightSubject => {
        const geometry = geometryForPlace(place, polygons ?? [], contextLines);
        return {anchor: {lat: place.latitude, lng: place.longitude},
          footprints: geometry.polygons.map(pathFromPolygon),
          extent: [...geometry.polygons.flatMap(pathFromPolygon), ...geometry.lines.flatMap(lineCoordinates)],
          emphasis: isolatesSelection ? 'selected' : 'place'};
      })];
    const roadCorridor = approachViewActive ? corridorFootprint(accessLines, policy.road.veilCorridorRadiusM) : null;
    if (roadCorridor) subjects.push({anchor: roadCorridor[Math.floor(roadCorridor.length / 2)], extent: roadCorridor, emphasis: 'road'});
    const removeSpotlight = quiet ? attachAtlasSpotlight(map, subjects,
      options => new library.Polygon3DElement({ ...options, altitudeMode: 'CLAMP_TO_GROUND', drawsOccludedSegments: false }),
      !window.matchMedia('(prefers-reduced-motion: reduce)').matches,
      cameraMode === 'evidence' && !corridorViewActive
        ? isolatesSelection ? policy.spotlight.nearbySelectedVeilFill : policy.spotlight.nearbyOverviewVeilFill
        : policy.spotlight.veilFill) : undefined;
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
        if (labelPosition && !corridorViewActive) {
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
      for (const contextLine of arrivalAtlasContextLines(metroLines, line => ({
        strokeColor: lineColorFromName(line.name, policy.metro.strokeColor),
        strokeWidth: policy.metro.strokeWidth,
        altitudeMode: 'clamp_to_ground',
        drawsOccludedSegments: policy.metro.drawsOccludedSegments,
      }))) {
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
            drawsOccludedSegments: contextLine.style.drawsOccludedSegments,
          },
          null,
          nextChildren,
        );
      }
    }
    if (!corridorViewActive) {
      const homeMarker = new library.Marker3DInteractiveElement({
        altitudeMode: cameraMode === "evidence" ? "RELATIVE_TO_GROUND" : "CLAMP_TO_GROUND",
        collisionBehavior: "REQUIRED",
        drawsWhenOccluded: true,
        extruded: true,
        sizePreserved: true,
        position: {
          lat: home.latitude,
          lng: home.longitude,
          ...(cameraMode === "evidence" ? {altitude: policy.nearby.markerLiftM} : {}),
        },
        title: home.name,
      });
      homeMarker.append(new markerLibrary.PinElement({
        ...mapMarkerPinOptions("home", cameraMode === "evidence" ? "selected" : "active"),
        ...(cameraMode === "evidence" ? {
          scale: policy.nearby.homePinScale,
          background: policy.nearby.homePinBackground,
        } : {}),
        glyphSrc: undefined,
        glyphText: "H",
      }));
      homeMarker.setAttribute("aria-label", "This home");
      homeMarker.dataset.atlasHome = 'true';
      map.append(homeMarker);
      nextChildren.push(homeMarker);
    }

    let activePopover: Popover3DElement | null = null;
    if (selected && cameraMode === 'evidence') {
      const arc = new library.Polyline3DInteractiveElement({
        altitudeMode: 'RELATIVE_TO_GROUND', coordinates: nearbyRelationArc(home, selected),
        strokeColor: '#e5f6cfe6', strokeWidth: 4, drawsOccludedSegments: true,
      });
      arc.setAttribute('aria-label', 'Straight-line relationship to home, not a travel route');
      arc.dataset.atlasRelationship = 'true';
      map.append(arc); nextChildren.push(arc);
    }
    const markerPlaces = corridorViewActive || nearbyDepth === 'home'
      ? []
      : places;
    for (const place of markerPlaces) {
      const popover = cameraMode === 'evidence' ? null : createPlacePopover(library, place);
      const isSelected = (place.id) === selectedPlaceId;
      const pinLabel = place.name;
      const marker = new library.Marker3DInteractiveElement({
        altitudeMode: cameraMode === "evidence" ? "RELATIVE_TO_GROUND" : "CLAMP_TO_GROUND",
        collisionBehavior: "REQUIRED",
        drawsWhenOccluded: true,
        sizePreserved: true,
        label: cameraMode === 'evidence' && isSelected ? place.name : undefined,
        extruded: cameraMode === "evidence",
        gmpPopoverTargetElement: popover ?? undefined,
        position: {
          lat: place.latitude,
          lng: place.longitude,
          ...(cameraMode === "evidence" ? {altitude: policy.nearby.markerLiftM} : {}),
        },
        title: pinLabel,
      });
      marker.append(new markerLibrary.PinElement({ ...mapMarkerPinOptions(
        place.icon,
        isSelected ? "selected" : selectedPlaceId ? "subdued" : "active",
      ),
        ...(cameraMode === 'evidence' ? {glyphSrc: undefined, glyphText: String(place.number)} : {}),
      }));
      marker.tabIndex = 0;
      marker.setAttribute("aria-label", pinLabel);
      marker.dataset.atlasPlaceId = place.id;
      marker.dataset.atlasSelected = String(isSelected);
      marker.addEventListener('gmp-click', () => onSelectPlace?.(place.id));
      marker.addEventListener('keydown', event => {
        if (event instanceof KeyboardEvent && (event.key === 'Enter' || event.key === ' ')) {
          event.preventDefault(); onSelectPlace?.(place.id);
        }
      });
      if (popover) {
        marker.addEventListener("pointerenter", () => {
          if (activePopover && activePopover !== popover) activePopover.open = false;
          popover.open = true;
          activePopover = popover;
        });
      }
      map.append(marker);
      nextChildren.push(marker);
      if (popover) {
        map.append(popover);
        nextChildren.push(popover);
      }
    }
    map.dataset.atlasDepth = nearbyDepth;
    map.dataset.atlasVisibility = corridorViewActive
      ? 'road'
      : nearbyDepth === 'home'
      ? 'home'
      : isolatesSelection
      ? 'pair'
      : 'category';
    map.dataset.atlasMarkerCount = String(markerPlaces.length + (corridorViewActive ? 0 : 1));
    if (selected && isolatesSelection) {
      map.dataset.atlasPairDistance = String(distanceMetres(
        {lat: home.latitude, lng: home.longitude},
        {lat: selected.latitude, lng: selected.longitude},
      ));
    } else {
      delete map.dataset.atlasPairDistance;
    }
    childrenRef.current = nextChildren;
    return () => removeSpotlight?.();
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
    approachViewActive,
    corridorViewActive,
    quiet,
    showBoundary,
    onSelectPlace,
    polygons,
    contextLines,
    selectedPlaceId,
    nearbyDepth,
    home,
  ]);

  useEffect(() => {
    const map = mapRef.current;
    const request = nearbyTourRequest;
    if (
      !map
      || !ready
      || !safeFrame
      || !request
      || request.chapters.length === 0
      || executedTourRequestRef.current === request.id
    ) return undefined;
    executedTourRequestRef.current = request.id;

    type PlannedScene = { scene: AtlasScene; chapter: NearbyTourChapter };
    const homeFeature: AtlasFeature = {
      id: 'home',
      name: home.name,
      categoryId: 'home',
      position: {lat: home.latitude, lng: home.longitude},
      boundary: home.boundary ? pointsFromCoordinates(home.boundary.coordinates) : undefined,
      evidence: {
        location: {providerId: 'scene'},
        distance: {metres: 0, method: 'straight_line', target: 'place_point'},
      },
    };
    const planned: PlannedScene[] = [];
    let finalHome: PlannedScene | undefined;
    for (const chapter of request.chapters) {
      const features: AtlasFeature[] = chapter.places.map((place) => {
        const geometry = geometryForPlace(place, chapter.polygons, chapter.lines);
        return {
          id: place.id,
          name: place.name,
          categoryId: chapter.categoryId,
          position: {lat: place.latitude, lng: place.longitude},
          boundary: geometry.polygons[0]
            ? pointsFromCoordinates(geometry.polygons[0].coordinates)
            : undefined,
          segments: geometry.lines.length ? geometry.lines.map((line) => ({
            id: line.id,
            path: pointsFromCoordinates(line.coordinates),
          })) : undefined,
          evidence: {
            location: {providerId: 'scene'},
            distance: {
              metres: distanceMetres(
                homeFeature.position,
                {lat: place.latitude, lng: place.longitude},
              ),
              method: 'straight_line',
              target: geometry.polygons.length
                ? 'mapped_extent_centre'
                : geometry.lines.length
                ? 'representative_segment_point'
                : 'place_point',
            },
          },
        };
      });
      const camera = (feature: AtlasFeature | null, depth: NearbyDepth) => nearbySceneCamera(
        home,
        chapter.places,
        chapter.polygons,
        chapter.lines,
        feature?.id ?? null,
        depth,
        groundElevation,
        mapWidth,
        safeFrame,
        undefined,
        chapter.view === 'nearby' ? 'selected-home-foreground' : 'category-stable',
      );
      const scenes = buildCategoryTour({
        categoryId: chapter.categoryId,
        home: homeFeature,
        features,
        cameras: {
          group: () => camera(null, 'overview'),
          pair: (feature) => camera(feature, 'pair'),
          focus: (feature) => camera(feature, 'inspect'),
          home: () => camera(null, 'home'),
        },
        copy: {
          overview: () => '',
          pair: () => '',
          focus: () => '',
          returnHome: () => '',
        },
        timing: policy.nearby,
      });
      for (const scene of scenes) {
        const item = {scene, chapter};
        if (scene.phase === 'home') finalHome = item;
        else planned.push(item);
      }
    }
    if (finalHome) planned.push(finalHome);

    const run = playbackController.begin('playing');
    if (!run.activate()) return undefined;
    let current: PlannedScene | undefined;
    const apply = (item: PlannedScene, durationMs: number) => {
      current = item;
      const selectedPlaceId = item.scene.visibility.mode === 'pair'
        || item.scene.visibility.mode === 'feature'
        ? item.scene.visibility.featureId
        : null;
      const depth: NearbyDepth = item.scene.phase === 'overview'
        ? 'overview'
        : item.scene.phase === 'pair'
        ? 'pair'
        : item.scene.phase === 'home'
        ? 'home'
        : 'inspect';
      onNearbyTourSceneRef.current?.({
        view: item.chapter.view,
        layerId: item.chapter.layerId,
        selectedPlaceId,
        depth,
      });
      cameraArbiterRef.current.submit('nearby', () => {
        map.dataset.atlasCameraOwner = 'nearby';
        map.dataset.atlasScene = `tour:${item.scene.id}`;
        map.dataset.atlasDepth = depth;
        map.dataset.atlasCameraTargetRange = String(item.scene.camera.range);
        map.flyCameraTo({
          durationMillis: window.matchMedia('(prefers-reduced-motion: reduce)').matches
            ? 0
            : durationMs,
          endCamera: item.scene.camera,
        });
      });
    };
    const unregisterResume = playbackController.registerResumer(() => {
      if (!current || !run.isCurrent()) return;
      apply(current, Math.min(
        Math.round(current.scene.durationMs * policy.focus.movementFraction),
        playbackController.remainingWaitMs(),
      ));
    });
    void (async () => {
      for (const item of planned) {
        if (!run.isCurrent()) return;
        apply(item, Math.round(item.scene.durationMs * policy.focus.movementFraction));
        if (!(await run.wait(item.scene.durationMs))) return;
      }
      run.settle();
    })();
    return () => unregisterResume();
  }, [
    groundElevation,
    home,
    mapWidth,
    nearbyTourRequest,
    playbackController,
    ready,
    safeFrame,
  ]);

  useEffect(() => {
    const map = mapRef.current;
    if (!map || !ready || manualCameraRef.current || terrainCorridor || approachViewActive || streetRequested || cameraMode !== 'evidence'
      || playbackState === 'playing' || playbackState === 'paused' || playbackState === 'preparing') return;
    const moveId = ++cameraMoveRef.current;
    let cancelled = false;
    let onEnd: ((event: Event) => void) | undefined;
    const current = () => !cancelled && !manualCameraRef.current && cameraMoveRef.current === moveId
      && cameraArbiterRef.current.owner() === 'nearby';
    const detach = () => { if (onEnd) map.removeEventListener('gmp-animationend', onEnd); onEnd = undefined; };
    const stop = () => { cancelled = true; detach(); };
    const unregister = playbackController.registerStopper(stop);
    const selected = places.find((place) => (place.id) === selectedPlaceId);
    const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    const pose = (depth: NearbyDepth, elevation: number) => nearbySceneCamera(home, places, polygons ?? [],
      [...metroLines, ...contextLines], selectedPlaceId, depth, elevation,
      containerRef.current?.clientWidth ?? window.innerWidth, safeFrame, above ? policy.above.tilt : undefined,
      nearbyCameraOrientation, nearbyOrientationPlaces);
    const fly = (camera: CameraOptions, stage: string, durationMs: number, next?: () => void) => {
      if (!current()) return;
      detach();
      cameraArbiterRef.current.submit('nearby', () => {
        map.stopCameraAnimation();
        map.dataset.atlasDepth = nearbyDepth;
        map.dataset.atlasCameraOwner = 'nearby';
        map.dataset.atlasScene = `nearby:${nearbyDepth}`;
        map.dataset.atlasFlightStage = stage;
        map.dataset.atlasCameraTargetRange = String(camera.range);
        onEnd = (event: Event) => {
          if (event.target !== map) return;
          detach();
          if (!current()) return;
          if (next) next(); else map.dataset.atlasFlightStage = 'settled';
        };
        map.addEventListener('gmp-animationend', onEnd);
        if (reducedMotion) {
          detach(); settleCameraFraming(map, camera); map.dataset.atlasFlightStage = 'settled';
        } else map.flyCameraTo({durationMillis: durationMs, endCamera: camera});
      });
    };
    const elevationTarget = nearbyDepth === 'inspect' && selected ? selected : home;
    const elevation = loadGoogleTerrainElevation(elevationTarget.latitude, elevationTarget.longitude)
      .catch(() => groundElevation);
    if (selected && nearbyDepth === 'inspect' && !above && !reducedMotion) {
      // Start the already-grounded context flight immediately while the selected
      // place's elevation loads. Only the close reveal needs that measurement.
      fly(pose('pair', groundElevation), 'context', policy.nearby.selectionContextMs,
        () => { void elevation.then(value => {
          if (current()) fly(pose(nearbyDepth, value), 'reveal', policy.nearby.selectionRevealMs);
        }); });
    } else {
      void elevation.then(value => {
        if (current()) fly(pose(nearbyDepth, value), 'overview', nearbyTransitionMs);
      });
    }
    return () => { stop(); unregister(); };
  }, [above, selectedPlaceId, nearbyDepth, nearbySelectionVersion, nearbyTransitionMs, places, home, polygons, metroLines, contextLines, cameraMode, terrainCorridor, playbackController, playbackState, ready, approachViewActive, safeFrame, streetRequested, groundElevation, nearbyCameraOrientation, nearbyOrientationPlaces]);

  const appliedCameraRequest = useRef<number | null>(null);
  useEffect(() => {
    const map = mapRef.current;
    if (!map || !ready || !cameraRequest || appliedCameraRequest.current === cameraRequest.id) return;
    appliedCameraRequest.current = cameraRequest.id;
    manualCameraRef.current = true;
    cameraMoveRef.current += 1;
    const { action } = cameraRequest;
    const zoom = action === "zoom-in" ? 1 / policy.controls.zoomFactor : action === "zoom-out" ? policy.controls.zoomFactor : 1;
    const camera = {
      center: map.center,
      ...(typeof map.fov === "number" ? { fov: map.fov } : {}),
      heading: (map.heading + (action === "rotate" ? policy.controls.rotateDegrees : 0)) % 360,
      tilt: map.tilt,
      range: Math.max(policy.controls.minimumRangeM, Math.min(policy.controls.maximumRangeM, map.range * zoom)),
    };
    cameraArbiterRef.current.submit(cameraArbiterRef.current.owner(), () => {
      map.stopCameraAnimation();
      if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) settleCameraFraming(map, camera);
      else map.flyCameraTo({ endCamera: camera, durationMillis: policy.controls.durationMs });
    });
  }, [cameraRequest, ready]);

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
      {approachViewActive && (accessLines[0]?.name || roadTour.status) && (
        <div className="nearby-map__road-title">
          {accessLines[0]?.name}
          {showRoutePlaybackControls && roadTour.progress
            && ` · ${roadTour.progress.current}/${roadTour.progress.total}`}
          {roadTour.status && <span aria-live="polite">
            {accessLines[0]?.name ? " · " : ""}{roadTour.status}
          </span>}
        </div>
      )}
      {showArrivalViewControls ? (
        <div className="nearby-map__playback-controls" role="group" aria-label={showRoutePlaybackControls
          ? "Approach road playback"
          : "Entrance view"}>
          {showRoutePlaybackControls && (roadPlaybackCanPause || roadPlaybackCanResume) ? (
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
          {showRoutePlaybackControls && !roadPlaybackCanPause && !roadPlaybackCanResume ? (
            <button
              type="button"
              aria-label="Replay road tour"
              onClick={streetRequested ? roadTour.replay : roadFlight.replay}
            >
              <span aria-hidden="true">↻</span>
              <span>Replay</span>
            </button>
          ) : null}
          {showRoutePlaybackControls && !streetRequested && <label className="atlas-road-speed">Speed
            <input aria-label="Road tour speed" type="range" min={policy.road.minimumRate} max={policy.road.maximumRate}
              step="0.25" value={roadFlight.rate} onChange={e => roadFlight.setRate(Number(e.target.value))} />
            <output>{roadFlight.rate}×</output>
          </label>}
          {roadExperience && <button type="button" onClick={() => {
            if (streetRequested) exitStreet();
            else {
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
