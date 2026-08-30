import { useEffect, useRef, useState } from "react";
import {
  loadGoogleMaps2dLibrary,
  loadGoogleMaps3dLibrary,
  loadGoogleTerrainElevation,
} from "../../lib/googleMaps3d.ts";
import type {
  MapOverlayLine,
  MapOverlayPolygon,
  MapWaterContext,
} from "../../lib/types.ts";
import type {
  NearbyCameraMode,
  NearbyMapView,
  NumberedPlace,
  PlaceCluster,
  PlateViewport,
} from "../../lib/nearbyPlateProjection.ts";
import { cameraCenterForMode } from "../../lib/nearbyPlateProjection.ts";
import { NOTEBOOK_SAVE_ICON_PATH } from "../notebook/NotebookSaveIcon.tsx";

export type AroundThisHomeMapProps = {
  home: { latitude: number; longitude: number; name: string };
  places: NumberedPlace[];
  clusters: PlaceCluster[];
  selectedId: string | null;
  viewport: PlateViewport;
  metroLines: MapOverlayLine[];
  accessLines: MapOverlayLine[];
  redFlagLines: MapOverlayLine[];
  greenPatches?: MapOverlayPolygon[];
  lakes?: MapOverlayPolygon[];
  showMetroLines: boolean;
  water?: MapWaterContext | null;
  waterTint: boolean;
  expanded: boolean;
  cameraMode: NearbyCameraMode;
  mapView: NearbyMapView;
  pinnedPlaceIds?: string[];
  onSelectCluster: (cluster: PlaceCluster) => void;
  onSelectAccessLine: (id: string) => void;
  onSelectRedFlagLine: (id: string) => void;
  onRememberPlace?: (place: NumberedPlace) => void;
  onMapViewChange: (view: NearbyMapView) => void;
  onBackToHome: () => void;
  onToggleExpanded: () => void;
};

type LatLngAltitude = { lat: number; lng: number; altitude?: number };

type CameraOptions = {
  center: LatLngAltitude;
  heading: number;
  range: number;
  tilt: number;
};

type Map3DElement = HTMLElement & {
  center: LatLngAltitude;
  flyCameraTo: (options: {
    durationMillis: number;
    endCamera: CameraOptions;
  }) => Promise<void>;
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
  }) => Map3DChild & { path: LatLngAltitude[] };
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

type Map2DPosition = {
  lat: () => number;
  lng: () => number;
};

type Map2DFeature = {
  getProperty: (name: string) => unknown;
};

type Map2DDataEvent = {
  feature: Map2DFeature;
  latLng: Map2DPosition;
};

type Map2DListener = { remove: () => void };

type Map2DElement = {
  data: {
    addGeoJson: (geoJson: object) => unknown[];
    addListener: (
      eventName: "click" | "mouseover",
      listener: (event: Map2DDataEvent) => void,
    ) => Map2DListener;
    forEach: (callback: (feature: Map2DFeature) => void) => void;
    remove: (feature: Map2DFeature) => void;
    setStyle: (style: (feature: Map2DFeature) => object) => void;
  };
  setCenter: (center: { lat: number; lng: number }) => void;
  setOptions: (options: { gestureHandling: "cooperative" | "greedy" }) => void;
  setZoom: (zoom: number) => void;
};

type Map2DInfoWindow = {
  close: () => void;
  open: (options: { map: Map2DElement }) => void;
  setContent: (content: HTMLElement) => void;
  setPosition: (position: Map2DPosition | { lat: number; lng: number }) => void;
};

type Maps2DLibrary = {
  InfoWindow: new (options?: { disableAutoPan?: boolean }) => Map2DInfoWindow;
  Map: new (element: HTMLElement, options: object) => Map2DElement;
  SymbolPath: { CIRCLE: number };
};

const HOME_PORTRAIT_RANGE_M = 700;
const HOME_PORTRAIT_TILT = 48;
const EVIDENCE_MINIMUM_RANGE_M = 1_100;
const EVIDENCE_CAMERA_DURATION_MS = 600;
const HOME_CAMERA_DURATION_MS = 350;
const DEFAULT_HEADING = 210;
const HOME_2D_ZOOM = 17;
const EMPTY_POLYGONS: MapOverlayPolygon[] = [];

const MUTED_ROAD_MAP_STYLES = [
  { featureType: "poi", stylers: [{ visibility: "off" }] },
  { featureType: "transit", elementType: "labels.icon", stylers: [{ visibility: "off" }] },
  { featureType: "landscape", elementType: "geometry", stylers: [{ color: "#edf0eb" }] },
  { featureType: "road", elementType: "geometry", stylers: [{ color: "#ffffff" }] },
  { featureType: "road", elementType: "geometry.stroke", stylers: [{ color: "#d7d9d3" }] },
  { featureType: "road", elementType: "labels.text.fill", stylers: [{ color: "#555c57" }] },
  { featureType: "road", elementType: "labels.text.stroke", stylers: [{ color: "#ffffff" }] },
  { featureType: "water", elementType: "geometry", stylers: [{ color: "#dce8e9" }] },
] as const;

function evidenceCameraRange(radiusKm: number): number {
  return Math.max(EVIDENCE_MINIMUM_RANGE_M, radiusKm * 900);
}

function evidenceCameraTilt(radiusKm: number): number {
  if (radiusKm > 3) return 45;
  return 55;
}

function targetCamera(
  latitude: number,
  longitude: number,
  elevation: number,
  range: number,
  tilt: number,
): CameraOptions {
  return {
    center: { lat: latitude, lng: longitude, altitude: elevation },
    heading: DEFAULT_HEADING,
    range,
    tilt,
  };
}

function settleCameraFraming(map: Map3DElement, camera: CameraOptions) {
  map.heading = camera.heading;
  map.range = camera.range;
  map.tilt = camera.tilt;
}

function pathFromPolygon(polygon: MapOverlayPolygon): LatLngAltitude[] {
  return polygon.coordinates.map(([lng, lat]) => ({ lat, lng }));
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
    altitudeMode: "RELATIVE_TO_GROUND",
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

function notebookIcon(pinned: boolean): SVGSVGElement {
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("width", "15");
  svg.setAttribute("height", "15");
  svg.setAttribute("aria-hidden", "true");
  const path = document.createElementNS("http://www.w3.org/2000/svg", "path");
  path.setAttribute("d", NOTEBOOK_SAVE_ICON_PATH);
  path.setAttribute("fill", pinned ? "currentColor" : "none");
  path.setAttribute("stroke", "currentColor");
  path.setAttribute("stroke-width", "1.9");
  path.setAttribute("stroke-linecap", "round");
  path.setAttribute("stroke-linejoin", "round");
  svg.append(path);
  return svg;
}

function createPlacePopover(
  library: Maps3DLibrary,
  place: NumberedPlace,
  pinned: boolean,
  onRememberPlace?: (place: NumberedPlace) => void,
): Popover3DElement {
  const popover = new library.PopoverElement({
    autoPanDisabled: true,
    lightDismissDisabled: false,
  });
  popover.append(createPlacePopoverContent(place, pinned, onRememberPlace));
  return popover;
}

function createPlacePopoverContent(
  place: NumberedPlace,
  pinned: boolean,
  onRememberPlace?: (place: NumberedPlace) => void,
): HTMLDivElement {
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
  if (onRememberPlace) {
    const noteButton = document.createElement("button");
    noteButton.type = "button";
    noteButton.className = "nearby-map-popover__note";
    noteButton.setAttribute("aria-label", pinned ? "Remove from notes" : "Add to notes");
    noteButton.title = pinned ? "Saved" : "Add to notes";
    noteButton.setAttribute("aria-pressed", String(pinned));
    noteButton.append(notebookIcon(pinned));
    noteButton.addEventListener("click", (event) => {
      event.preventDefault();
      event.stopPropagation();
      onRememberPlace(place);
    });
    content.append(noteButton);
  }
  return content;
}

function createLinePopoverContent(line: MapOverlayLine): HTMLDivElement {
  const content = document.createElement("div");
  content.className = "nearby-map-popover";
  const name = document.createElement("strong");
  name.textContent = line.name;
  content.append(name);
  if (typeof line.distance_km === "number") {
    const distance = document.createElement("span");
    distance.textContent = `${line.distance_km.toFixed(1)} km`;
    content.append(distance);
  }
  return content;
}

function lineFeature(line: MapOverlayLine, featureType: string): object {
  return {
    type: "Feature",
    properties: {
      featureId: line.id,
      featureType,
      name: line.name,
    },
    geometry: {
      type: "LineString",
      coordinates: line.coordinates,
    },
  };
}

function polygonFeature(polygon: MapOverlayPolygon, featureType: string): object {
  return {
    type: "Feature",
    properties: { featureId: polygon.id, featureType, name: polygon.name },
    geometry: { type: "Polygon", coordinates: [polygon.coordinates] },
  };
}

function map2dGeoJson(
  home: AroundThisHomeMapProps["home"],
  places: NumberedPlace[],
  clusters: PlaceCluster[],
  accessLines: MapOverlayLine[],
  metroLines: MapOverlayLine[],
  redFlagLines: MapOverlayLine[],
  greenPatches: MapOverlayPolygon[],
  lakes: MapOverlayPolygon[],
): object {
  return {
    type: "FeatureCollection",
    features: [
      {
        type: "Feature",
        properties: { featureId: "home", featureType: "home", name: home.name },
        geometry: { type: "Point", coordinates: [home.longitude, home.latitude] },
      },
      ...places.map((place) => ({
        type: "Feature",
        properties: { featureId: place.id, featureType: "place", name: place.name },
        geometry: { type: "Point", coordinates: [place.longitude, place.latitude] },
      })),
      ...clusters.map((cluster) => ({
        type: "Feature",
        properties: {
          featureId: cluster.id,
          featureType: "cluster",
          name: `${cluster.count} nearby places`,
        },
        geometry: { type: "Point", coordinates: [cluster.longitude, cluster.latitude] },
      })),
      ...accessLines.map((line) => lineFeature(line, "access")),
      ...metroLines.map((line) => lineFeature(line, "metro")),
      ...redFlagLines.map((line) => lineFeature(line, "redFlag")),
      ...greenPatches.map((polygon) => polygonFeature(polygon, "green")),
      ...lakes.map((polygon) => polygonFeature(polygon, "water")),
    ],
  };
}

function map2dStyle(feature: Map2DFeature, circlePath: number): object {
  const type = feature.getProperty("featureType");
  switch (type) {
    case "access":
      return { strokeColor: "#414743", strokeOpacity: 0.96, strokeWeight: 7, zIndex: 8 };
    case "metro":
      return { strokeColor: "#7651a8", strokeOpacity: 0.92, strokeWeight: 6, zIndex: 7 };
    case "redFlag":
      return { strokeColor: "#b83b49", strokeOpacity: 0.9, strokeWeight: 6, zIndex: 9 };
    case "green":
      return { fillColor: "#8aaf83", fillOpacity: 0.2, strokeColor: "#64815f", strokeWeight: 1 };
    case "water":
      return { fillColor: "#78adc0", fillOpacity: 0.22, strokeColor: "#4f8ba0", strokeWeight: 1 };
    case "home":
      return {
        icon: {
          path: circlePath,
          fillColor: "#bc603c",
          fillOpacity: 1,
          scale: 8,
          strokeColor: "#ffffff",
          strokeWeight: 3,
        },
        zIndex: 12,
      };
    case "cluster":
      return {
        icon: {
          path: circlePath,
          fillColor: "#455f7a",
          fillOpacity: 0.95,
          scale: 9,
          strokeColor: "#ffffff",
          strokeWeight: 2,
        },
        zIndex: 11,
      };
    default:
      return {
        icon: {
          path: circlePath,
          fillColor: "#516f8d",
          fillOpacity: 0.96,
          scale: 6,
          strokeColor: "#ffffff",
          strokeWeight: 2,
        },
        zIndex: 10,
      };
  }
}

export function AroundThisHomeGoogle3DMap(props: AroundThisHomeMapProps) {
  const {
    home,
    places,
    clusters,
    selectedId,
    viewport,
    metroLines,
    accessLines,
    redFlagLines,
    greenPatches = EMPTY_POLYGONS,
    lakes = EMPTY_POLYGONS,
    showMetroLines,
    waterTint,
    expanded,
    cameraMode,
    mapView,
    pinnedPlaceIds = [],
    onSelectCluster,
    onSelectAccessLine,
    onSelectRedFlagLine,
    onRememberPlace,
    onMapViewChange,
    onBackToHome,
    onToggleExpanded,
  } = props;
  const containerRef = useRef<HTMLDivElement | null>(null);
  const map2dContainerRef = useRef<HTMLDivElement | null>(null);
  const mapRef = useRef<Map3DElement | null>(null);
  const map2dRef = useRef<Map2DElement | null>(null);
  const map2dLibraryRef = useRef<Maps2DLibrary | null>(null);
  const map2dInfoWindowRef = useRef<Map2DInfoWindow | null>(null);
  const libraryRef = useRef<Maps3DLibrary | null>(null);
  const childrenRef = useRef<Map3DChild[]>([]);
  const cameraMoveRef = useRef(0);
  const terrainElevationRef = useRef<number | null>(null);
  const [ready, setReady] = useState(false);
  const [ready2d, setReady2d] = useState(false);
  const [loadError, setLoadError] = useState<Error | null>(null);
  const cameraCenter = cameraCenterForMode(cameraMode, home, viewport);
  const cameraLatitude = cameraCenter.latitude;
  const cameraLongitude = cameraCenter.longitude;

  useEffect(() => {
    let cancelled = false;
    void Promise.all([
      loadGoogleMaps3dLibrary(),
      loadGoogleTerrainElevation(home.latitude, home.longitude),
    ])
      .then(([loaded, terrainElevation]) => {
        if (cancelled || !containerRef.current) return;
        const library = loaded as Maps3DLibrary;
        terrainElevationRef.current = terrainElevation;
        const map = new library.Map3DElement({
          center: {
            lat: home.latitude,
            lng: home.longitude,
            altitude: terrainElevation,
          },
          defaultUIHidden: true,
          gestureHandling: "COOPERATIVE",
          heading: DEFAULT_HEADING,
          mode: "SATELLITE",
          range: HOME_PORTRAIT_RANGE_M,
          tilt: 0,
        });
        libraryRef.current = library;
        mapRef.current = map;
        containerRef.current.replaceChildren(map);
        setReady(true);
      })
      .catch((error: unknown) => {
        if (import.meta.env.DEV) {
          console.warn("[AroundThisHomeGoogle3DMap] Google 3D failed to load", error);
        }
        if (!cancelled) {
          setLoadError(error instanceof Error ? error : new Error("google_maps_3d_unavailable"));
        }
      });
    return () => {
      cancelled = true;
      cameraMoveRef.current += 1;
      for (const child of childrenRef.current) child.remove();
      childrenRef.current = [];
      mapRef.current?.remove();
      mapRef.current = null;
      libraryRef.current = null;
      terrainElevationRef.current = null;
    };
  }, [home.latitude, home.longitude]);

  useEffect(() => {
    if (!mapRef.current) return;
    mapRef.current.gestureHandling = expanded ? "GREEDY" : "COOPERATIVE";
  }, [expanded]);

  useEffect(() => {
    const map = mapRef.current;
    if (!map || !ready || terrainElevationRef.current === null) return;
    const evidenceFocused = cameraMode === "evidence";
    const range = evidenceFocused
      ? evidenceCameraRange(viewport.radiusKm)
      : HOME_PORTRAIT_RANGE_M;
    const tilt = evidenceFocused
      ? evidenceCameraTilt(viewport.radiusKm)
      : HOME_PORTRAIT_TILT;
    const moveId = cameraMoveRef.current + 1;
    cameraMoveRef.current = moveId;
    const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    const durationMillis = reducedMotion
      ? 0
      : evidenceFocused
      ? EVIDENCE_CAMERA_DURATION_MS
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
        );
        return map.flyCameraTo({ endCamera: camera, durationMillis })
          .then(() => {
            if (cameraMoveRef.current === moveId) settleCameraFraming(map, camera);
          });
      })
      .catch((error: unknown) => {
        if (import.meta.env.DEV) {
          console.warn("[AroundThisHomeGoogle3DMap] Terrain-safe camera move failed", error);
        }
      });
  }, [
    cameraMode,
    cameraLatitude,
    cameraLongitude,
    ready,
    viewport.radiusKm,
  ]);

  useEffect(() => {
    if (mapView !== "2d" || map2dRef.current) return undefined;
    let cancelled = false;
    const container = map2dContainerRef.current;
    if (!container) return undefined;
    void loadGoogleMaps2dLibrary()
      .then((loaded) => {
        if (cancelled) return;
        const library = loaded as Maps2DLibrary;
        const map = new library.Map(container, {
          center: { lat: home.latitude, lng: home.longitude },
          clickableIcons: false,
          disableDefaultUI: true,
          gestureHandling: "cooperative",
          keyboardShortcuts: true,
          styles: MUTED_ROAD_MAP_STYLES,
          zoom: HOME_2D_ZOOM,
          zoomControl: true,
        });
        map2dLibraryRef.current = library;
        map2dRef.current = map;
        map2dInfoWindowRef.current = new library.InfoWindow({ disableAutoPan: true });
        setReady2d(true);
      })
      .catch((error: unknown) => {
        if (import.meta.env.DEV) {
          console.warn("[AroundThisHomeGoogle3DMap] Google 2D failed to load", error);
        }
        if (!cancelled) {
          setLoadError(error instanceof Error ? error : new Error("google_maps_2d_unavailable"));
        }
      });
    return () => {
      cancelled = true;
      map2dInfoWindowRef.current?.close();
      const map = map2dRef.current;
      if (map) {
        const features: Map2DFeature[] = [];
        map.data.forEach((feature) => features.push(feature));
        for (const feature of features) map.data.remove(feature);
      }
      container.replaceChildren();
      map2dInfoWindowRef.current = null;
      map2dLibraryRef.current = null;
      map2dRef.current = null;
      setReady2d(false);
    };
  }, [home.latitude, home.longitude, mapView]);

  useEffect(() => {
    const map = map2dRef.current;
    if (!map || !ready2d) return;
    map.setCenter({ lat: cameraLatitude, lng: cameraLongitude });
    map.setZoom(cameraMode === "home" ? HOME_2D_ZOOM : viewport.zoom);
    map.setOptions({ gestureHandling: expanded ? "greedy" : "cooperative" });
  }, [cameraLatitude, cameraLongitude, cameraMode, expanded, ready2d, viewport.zoom]);

  useEffect(() => {
    const map = map2dRef.current;
    const library = map2dLibraryRef.current;
    const infoWindow = map2dInfoWindowRef.current;
    if (!map || !library || !infoWindow || !ready2d) return undefined;

    const currentFeatures: Map2DFeature[] = [];
    map.data.forEach((feature) => currentFeatures.push(feature));
    for (const feature of currentFeatures) map.data.remove(feature);

    const showGreenPatches = places.some((place) => place.layer === "parks");
    map.data.addGeoJson(map2dGeoJson(
      home,
      places,
      clusters,
      accessLines,
      showMetroLines ? metroLines : [],
      redFlagLines,
      showGreenPatches ? greenPatches : EMPTY_POLYGONS,
      waterTint ? lakes : EMPTY_POLYGONS,
    ));
    map.data.setStyle((feature) => map2dStyle(feature, library.SymbolPath.CIRCLE));

    const placesById = new Map(places.map((place) => [place.id, place]));
    const clustersById = new Map(clusters.map((cluster) => [cluster.id, cluster]));
    const accessLinesById = new Map(accessLines.map((line) => [line.id, line]));
    const redFlagLinesById = new Map(redFlagLines.map((line) => [line.id, line]));

    const showFeature = (event: Map2DDataEvent, select: boolean) => {
      const featureId = String(event.feature.getProperty("featureId") ?? "");
      const featureType = event.feature.getProperty("featureType");
      const place = placesById.get(featureId);
      if (place) {
        infoWindow.setContent(createPlacePopoverContent(
          place,
          pinnedPlaceIds.includes(place.id),
          onRememberPlace,
        ));
      } else {
        const line = accessLinesById.get(featureId) ?? redFlagLinesById.get(featureId);
        if (line) {
          infoWindow.setContent(createLinePopoverContent(line));
          if (select) {
            if (featureType === "redFlag") onSelectRedFlagLine(line.id);
            else onSelectAccessLine(line.id);
          }
        } else {
          const name = String(event.feature.getProperty("name") ?? "");
          if (!name || featureType === "home") return;
          const content = document.createElement("div");
          content.className = "nearby-map-popover";
          const title = document.createElement("strong");
          title.textContent = name;
          content.append(title);
          infoWindow.setContent(content);
          const cluster = clustersById.get(featureId);
          if (select && cluster) onSelectCluster(cluster);
        }
      }
      infoWindow.setPosition(event.latLng);
      infoWindow.open({ map });
    };
    const hoverListener = map.data.addListener("mouseover", (event) => showFeature(event, false));
    const clickListener = map.data.addListener("click", (event) => showFeature(event, true));
    return () => {
      hoverListener.remove();
      clickListener.remove();
      infoWindow.close();
    };
  }, [
    accessLines,
    clusters,
    greenPatches,
    home,
    lakes,
    metroLines,
    onRememberPlace,
    onSelectAccessLine,
    onSelectCluster,
    onSelectRedFlagLine,
    pinnedPlaceIds,
    places,
    ready2d,
    redFlagLines,
    showMetroLines,
    waterTint,
  ]);

  useEffect(() => {
    const map = mapRef.current;
    if (!map || !ready) return undefined;
    const cancelAutomaticCamera = () => {
      cameraMoveRef.current += 1;
    };
    map.addEventListener("pointerdown", cancelAutomaticCamera);
    map.addEventListener("touchstart", cancelAutomaticCamera, { passive: true });
    map.addEventListener("wheel", cancelAutomaticCamera, { passive: true });
    map.addEventListener("keydown", cancelAutomaticCamera);
    return () => {
      map.removeEventListener("pointerdown", cancelAutomaticCamera);
      map.removeEventListener("touchstart", cancelAutomaticCamera);
      map.removeEventListener("wheel", cancelAutomaticCamera);
      map.removeEventListener("keydown", cancelAutomaticCamera);
    };
  }, [ready]);

  useEffect(() => {
    const map = mapRef.current;
    const library = libraryRef.current;
    if (!ready || !map || !library) return;
    for (const child of childrenRef.current) child.remove();
    const nextChildren: Map3DChild[] = [];

    const showGreenPatches = places.some((place) => place.layer === "parks");
    for (const patch of showGreenPatches ? greenPatches : EMPTY_POLYGONS) {
      addPolygon(map, library, patch, { fill: "#6e9d6e26", stroke: "#50795099" }, nextChildren);
    }
    for (const lake of waterTint ? lakes : EMPTY_POLYGONS) {
      addPolygon(map, library, lake, { fill: "#4f9fc42e", stroke: "#357fa7aa" }, nextChildren);
    }
    for (const line of accessLines) {
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
        () => onSelectAccessLine(line.id),
        nextChildren,
      );
      const labelPosition = lineLabelPosition(line);
      if (labelPosition) {
        const routeLabel = new library.Marker3DInteractiveElement({
          altitudeMode: "CLAMP_TO_GROUND",
          collisionBehavior: "REQUIRED",
          drawsWhenOccluded: true,
          label: line.name,
          position: labelPosition,
          title: line.name,
        });
        routeLabel.addEventListener("gmp-click", () => onSelectAccessLine(line.id));
        map.append(routeLabel);
        nextChildren.push(routeLabel);
      }
    }
    if (showMetroLines) {
      for (const line of metroLines) {
        addLine(
          map,
          library,
          line,
          {
            color: "#7651a8",
            width: 7,
            outerColor: "#ffffffcc",
            outerWidth: 0.35,
            drawsOccludedSegments: false,
          },
          null,
          nextChildren,
        );
      }
    }
    for (const line of redFlagLines) {
      addLine(
        map,
        library,
        line,
        {
          color: "#c93f3f",
          width: 7,
          outerColor: "#ffffffcc",
          outerWidth: 0.35,
          drawsOccludedSegments: false,
        },
        () => onSelectRedFlagLine(line.id),
        nextChildren,
      );
    }

    const homeMarker = new library.Marker3DInteractiveElement({
      altitudeMode: "CLAMP_TO_GROUND",
      collisionBehavior: "REQUIRED",
      drawsWhenOccluded: true,
      extruded: true,
      label: "This home",
      position: { lat: home.latitude, lng: home.longitude },
      title: home.name,
    });
    map.append(homeMarker);
    nextChildren.push(homeMarker);

    for (const cluster of clusters) {
      const marker = new library.Marker3DInteractiveElement({
        altitudeMode: "CLAMP_TO_GROUND",
        collisionBehavior: "REQUIRED",
        label: `${cluster.count} places`,
        position: { lat: cluster.latitude, lng: cluster.longitude },
        title: `${cluster.count} nearby places`,
      });
      marker.addEventListener("gmp-click", () => onSelectCluster(cluster));
      map.append(marker);
      nextChildren.push(marker);
    }
    for (const place of places) {
      const popover = createPlacePopover(
        library,
        place,
        pinnedPlaceIds.includes(place.id),
        onRememberPlace,
      );
      const marker = new library.Marker3DInteractiveElement({
        altitudeMode: "CLAMP_TO_GROUND",
        collisionBehavior: place.id === selectedId
          ? "REQUIRED"
          : "OPTIONAL_AND_HIDES_LOWER_PRIORITY",
        drawsWhenOccluded: place.id === selectedId,
        gmpPopoverTargetElement: popover,
        label: place.id === selectedId ? place.name : undefined,
        position: { lat: place.latitude, lng: place.longitude },
        title: place.name,
      });
      map.append(marker);
      map.append(popover);
      nextChildren.push(marker);
      nextChildren.push(popover);
    }
    childrenRef.current = nextChildren;
  }, [
    accessLines,
    clusters,
    greenPatches,
    home.latitude,
    home.longitude,
    home.name,
    lakes,
    metroLines,
    onSelectAccessLine,
    onSelectCluster,
    onSelectRedFlagLine,
    onRememberPlace,
    pinnedPlaceIds,
    places,
    redFlagLines,
    ready,
    selectedId,
    showMetroLines,
    waterTint,
  ]);

  if (loadError) throw loadError;

  function backToHome() {
    onMapViewChange("3d");
    onBackToHome();
  }

  function toggleExpanded() {
    onToggleExpanded();
  }

  return (
    <div
      className={`nearby-map nearby-map--google${expanded ? " is-expanded" : ""}`}
      role="region"
      aria-label="Nearby evidence map"
      aria-busy={mapView === "3d" ? !ready : !ready2d}
      data-map-renderer={`google-${mapView}`}
    >
      <div
        ref={containerRef}
        className={`nearby-map__canvas nearby-map__canvas--google-3d${mapView === "2d" ? " is-hidden" : ""}`}
        aria-hidden={mapView === "2d"}
      />
      <div
        ref={map2dContainerRef}
        className={`nearby-map__canvas nearby-map__canvas--google-2d${mapView === "3d" ? " is-hidden" : ""}`}
        aria-hidden={mapView === "3d"}
      />
      <div className="nearby-map__actions">
        <div className="nearby-map__view-switch" role="group" aria-label="Map view">
          <button
            type="button"
            aria-pressed={mapView === "3d"}
            onClick={() => onMapViewChange("3d")}
          >
            3D
          </button>
          <button
            type="button"
            aria-pressed={mapView === "2d"}
            onClick={() => onMapViewChange("2d")}
          >
            2D
          </button>
        </div>
        {cameraMode === "evidence" && (
          <button type="button" onClick={backToHome}>Back to home</button>
        )}
        <button type="button" onClick={toggleExpanded}>
          {expanded ? "Close map" : "Expand map"}
        </button>
      </div>
    </div>
  );
}
