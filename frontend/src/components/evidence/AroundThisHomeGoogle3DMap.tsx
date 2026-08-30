import { useEffect, useMemo, useRef, useState } from "react";
import {
  loadGoogleMaps3dLibrary,
  loadGoogleMarkerLibrary,
  loadGoogleTerrainElevation,
} from "../../lib/googleMaps3d.ts";
import { mapMarkerPinOptions } from "../../lib/mapMarkerVisual.ts";
import { useGuidedStreetViewTour } from "../../hooks/useGuidedStreetViewTour.ts";
import type {
  MapLayerExperience,
  MapPresentation,
  MapOverlayLine,
  MapOverlayPolygon,
  MapWaterContext,
} from "../../lib/types.ts";
import type {
  NearbyCameraMode,
  NumberedPlace,
  PlaceCluster,
  PlateViewport,
} from "../../lib/nearbyPlateProjection.ts";
import {
  cameraCenterForMode,
  corridorCameraFocus,
  corridorTourWaypoints,
} from "../../lib/nearbyPlateProjection.ts";
import { NOTEBOOK_SAVE_ICON_PATH } from "../notebook/NotebookSaveIcon.tsx";

export type AroundThisHomeMapProps = {
  home: {
    latitude: number;
    longitude: number;
    name: string;
    boundary?: MapOverlayPolygon;
  };
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
  terrainCorridor: boolean;
  layerExperience?: MapLayerExperience;
  mapPresentation: MapPresentation;
  pinnedPlaceIds?: string[];
  onSelectCluster: (cluster: PlaceCluster) => void;
  onSelectPlace: (place: NumberedPlace) => void;
  onSelectAccessLine: (id: string) => void;
  onSelectRedFlagLine: (id: string) => void;
  onRememberPlace?: (place: NumberedPlace) => void;
  onBackToHome: () => void;
  onToggleExpanded: () => void;
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
const EVIDENCE_MINIMUM_RANGE_M = 1_100;
const EVIDENCE_CAMERA_DURATION_MS = 600;
const HOME_CAMERA_DURATION_MS = 350;
const DEFAULT_HEADING = 210;
const EMPTY_POLYGONS: MapOverlayPolygon[] = [];

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
  heading: number,
): CameraOptions {
  return {
    center: { lat: latitude, lng: longitude, altitude: elevation },
    heading,
    range,
    tilt,
  };
}

function roadCamera(
  focus: { latitude: number; longitude: number; heading: number },
  terrainElevation: number,
  experience: MapLayerExperience,
): CameraOptions {
  return {
    cameraPosition: {
      lat: focus.latitude,
      lng: focus.longitude,
      altitude: terrainElevation + experience.cameraAltitudeM,
    },
    fov: experience.cameraFov,
    heading: focus.heading,
    range: experience.cameraRangeM,
    tilt: experience.cameraTilt,
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
    terrainCorridor,
    layerExperience,
    pinnedPlaceIds = [],
    onSelectCluster,
    onSelectPlace,
    onSelectAccessLine,
    onSelectRedFlagLine,
    onRememberPlace,
    onBackToHome,
    onToggleExpanded,
  } = props;
  const containerRef = useRef<HTMLDivElement | null>(null);
  const streetViewContainerRef = useRef<HTMLDivElement | null>(null);
  const mapRef = useRef<Map3DElement | null>(null);
  const libraryRef = useRef<Maps3DLibrary | null>(null);
  const markerLibraryRef = useRef<MarkerLibrary | null>(null);
  const childrenRef = useRef<Map3DChild[]>([]);
  const cameraMoveRef = useRef(0);
  const terrainElevationRef = useRef<number | null>(null);
  const [ready, setReady] = useState(false);
  const [loadError, setLoadError] = useState<Error | null>(null);
  const homeLatitude = home.latitude;
  const homeLongitude = home.longitude;
  const roadExperience = layerExperience?.kind === "street_view_tour"
    ? layerExperience
    : null;
  const roadTourActive = terrainCorridor && cameraMode === "evidence";
  const roadFocus = useMemo(
    () => roadTourActive
      ? corridorCameraFocus(accessLines, {
        latitude: homeLatitude,
        longitude: homeLongitude,
      })
      : null,
    [accessLines, homeLatitude, homeLongitude, roadTourActive],
  );
  const cameraCenter = roadFocus ?? cameraCenterForMode(cameraMode, home, viewport);
  const cameraLatitude = cameraCenter.latitude;
  const cameraLongitude = cameraCenter.longitude;
  const roadWaypoints = useMemo(
    () => roadFocus && roadExperience
      ? corridorTourWaypoints(
        accessLines,
        { latitude: homeLatitude, longitude: homeLongitude },
        roadExperience.distanceEachDirectionM,
        roadExperience.waypointSpacingM,
      )
      : [],
    [accessLines, homeLatitude, homeLongitude, roadExperience, roadFocus],
  );
  const streetViewReady = useGuidedStreetViewTour({
    active: Boolean(roadFocus),
    containerRef: streetViewContainerRef,
    experience: roadExperience,
    waypoints: roadWaypoints,
  });

  useEffect(() => {
    let cancelled = false;
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
        markerLibraryRef.current = markerLibrary;
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
      markerLibraryRef.current = null;
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
    const range = roadFocus && roadExperience
      ? roadExperience.cameraRangeM
      : evidenceFocused
      ? evidenceCameraRange(viewport.radiusKm)
      : HOME_PORTRAIT_RANGE_M;
    const tilt = roadFocus && roadExperience
      ? roadExperience.cameraTilt
      : evidenceFocused
      ? evidenceCameraTilt(viewport.radiusKm)
      : HOME_PORTRAIT_TILT;
    const heading = roadFocus?.heading ?? DEFAULT_HEADING;
    const moveId = cameraMoveRef.current + 1;
    cameraMoveRef.current = moveId;
    const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    const durationMillis = reducedMotion
      ? 0
      : roadFocus && roadExperience
      ? roadExperience.transitionMs
      : evidenceFocused
      ? EVIDENCE_CAMERA_DURATION_MS
      : HOME_CAMERA_DURATION_MS;
    void loadGoogleTerrainElevation(cameraLatitude, cameraLongitude)
      .then((terrainElevation) => {
        if (cameraMoveRef.current !== moveId || mapRef.current !== map) return;
        const camera = roadFocus && roadExperience
          ? roadCamera(roadFocus, terrainElevation, roadExperience)
          : targetCamera(
            cameraLatitude,
            cameraLongitude,
            terrainElevation,
            range,
            tilt,
            heading,
          );
        return map.flyCameraTo({ endCamera: camera, durationMillis })
          .then(() => {
            if (cameraMoveRef.current === moveId && !roadFocus) {
              settleCameraFraming(map, camera);
            }
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
    accessLines,
    ready,
    roadExperience,
    roadFocus,
    roadTourActive,
    viewport.radiusKm,
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
    const markerLibrary = markerLibraryRef.current;
    if (!ready || !map || !library || !markerLibrary) return;
    for (const child of childrenRef.current) child.remove();
    const nextChildren: Map3DChild[] = [];

    if (cameraMode === "home" && home.boundary) {
      addPolygon(
        map,
        library,
        home.boundary,
        { fill: "#f8f1df1f", stroke: "#fff7e3e6" },
        nextChildren,
      );
    }
    const showGreenPatches = places.some((place) => place.layer === "parks");
    for (const patch of showGreenPatches ? greenPatches : EMPTY_POLYGONS) {
      addPolygon(map, library, patch, { fill: "#6e9d6e26", stroke: "#50795099" }, nextChildren);
    }
    for (const lake of waterTint ? lakes : EMPTY_POLYGONS) {
      addPolygon(map, library, lake, { fill: "#4f9fc42e", stroke: "#357fa7aa" }, nextChildren);
    }
    for (const line of accessLines) {
      if (!roadTourActive) {
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

    if (!roadTourActive) {
      const homeIsContext = cameraMode === "evidence";
      const homeMarker = new library.Marker3DInteractiveElement({
        altitudeMode: "CLAMP_TO_GROUND",
        collisionBehavior: homeIsContext ? "OPTIONAL_AND_HIDES_LOWER_PRIORITY" : "REQUIRED",
        drawsWhenOccluded: true,
        extruded: !homeIsContext,
        label: homeIsContext ? undefined : "This home",
        position: { lat: home.latitude, lng: home.longitude },
        title: home.name,
      });
      if (homeIsContext) {
        homeMarker.append(new markerLibrary.PinElement(mapMarkerPinOptions("home", "subdued")));
      }
      map.append(homeMarker);
      nextChildren.push(homeMarker);
    }

    for (const cluster of clusters) {
      const marker = new library.Marker3DInteractiveElement({
        altitudeMode: "CLAMP_TO_GROUND",
        collisionBehavior: "REQUIRED",
        position: { lat: cluster.latitude, lng: cluster.longitude },
        title: `${cluster.count} nearby places`,
      });
      const clusterPin = mapMarkerPinOptions(cluster.icon, "active");
      marker.append(new markerLibrary.PinElement({
        ...clusterPin,
        glyphSrc: undefined,
        glyphText: String(cluster.count),
      }));
      marker.addEventListener("gmp-click", () => onSelectCluster(cluster));
      map.append(marker);
      nextChildren.push(marker);
    }
    let activePopover: Popover3DElement | null = null;
    for (const place of places) {
      const emphasis = place.id === selectedId
        ? "selected"
        : selectedId
        ? "subdued"
        : "active";
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
      marker.append(new markerLibrary.PinElement(mapMarkerPinOptions(place.icon, emphasis)));
      marker.addEventListener("pointerenter", () => {
        if (activePopover && activePopover !== popover) activePopover.open = false;
        popover.open = true;
        activePopover = popover;
      });
      marker.addEventListener("gmp-click", () => {
        popover.open = false;
        onSelectPlace(place);
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
    cameraMode,
    home.boundary,
    home.latitude,
    home.longitude,
    home.name,
    lakes,
    metroLines,
    onSelectAccessLine,
    onSelectCluster,
    onSelectPlace,
    onSelectRedFlagLine,
    onRememberPlace,
    pinnedPlaceIds,
    places,
    redFlagLines,
    ready,
    selectedId,
    showMetroLines,
    roadTourActive,
    waterTint,
  ]);

  if (loadError) throw loadError;

  function backToHome() {
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
      aria-busy={!ready}
      data-map-renderer={streetViewReady ? "google-street-view" : "google-3d"}
    >
      <div
        ref={containerRef}
        className={`nearby-map__canvas nearby-map__canvas--google-3d${streetViewReady ? " is-behind-street-view" : ""}`}
        aria-hidden={streetViewReady}
      />
      <div
        ref={streetViewContainerRef}
        className={`nearby-map__canvas nearby-map__canvas--street-view${streetViewReady ? " is-active" : ""}`}
        aria-hidden={!streetViewReady}
      />
      {roadTourActive && accessLines[0]?.name && (
        <div className="nearby-map__road-title">{accessLines[0].name}</div>
      )}
      <div className="nearby-map__actions">
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
