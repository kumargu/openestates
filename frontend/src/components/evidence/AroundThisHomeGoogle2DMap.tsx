import { useEffect, useRef, useState } from "react";
import {
  loadGoogleMaps2dLibrary,
  loadGoogleMarkerLibrary,
} from "../../lib/googleMaps3d.ts";
import { mapMarkerIconUrl } from "../../lib/mapMarkerVisual.ts";
import type { MapOverlayLine, MapOverlayPolygon } from "../../lib/types.ts";
import type { NumberedPlace } from "../../lib/nearbyPlateProjection.ts";
import type { AroundThisHomeMapProps } from "./AroundThisHomeGoogle3DMap.tsx";

type LatLng = { lat: number; lng: number };

type Map2DElement = {
  fitBounds: (bounds: LatLngBounds, padding?: number) => void;
  setCenter: (center: LatLng) => void;
  setOptions: (options: Record<string, unknown>) => void;
  setZoom: (zoom: number) => void;
};

type LatLngBounds = {
  extend: (position: LatLng) => void;
};

type MapListener = { remove: () => void };

type MapOverlay = {
  addListener?: (eventName: string, listener: () => void) => MapListener;
  setMap: (map: Map2DElement | null) => void;
};

type MapMarker = MapOverlay;

type InfoWindow = {
  close: () => void;
  open: (options: { anchor: MapMarker; map: Map2DElement }) => void;
};

type Maps2DLibrary = {
  InfoWindow: new (options: {
    content: HTMLElement;
    disableAutoPan: boolean;
    headerDisabled: boolean;
  }) => InfoWindow;
  LatLngBounds: new () => LatLngBounds;
  Map: new (container: HTMLElement, options: Record<string, unknown>) => Map2DElement;
  Polygon: new (options: Record<string, unknown>) => MapOverlay;
  Polyline: new (options: Record<string, unknown>) => MapOverlay;
  Size: new (width: number, height: number) => unknown;
};

type MarkerLibrary = {
  Marker: new (options: Record<string, unknown>) => MapMarker;
};

const MAP_STYLE = [
  { featureType: "poi", elementType: "labels", stylers: [{ visibility: "off" }] },
  { featureType: "transit", elementType: "labels", stylers: [{ visibility: "off" }] },
  { featureType: "road", elementType: "geometry", stylers: [{ color: "#f4f0e8" }] },
  { featureType: "road", elementType: "labels.text.fill", stylers: [{ color: "#68645d" }] },
  { featureType: "landscape", elementType: "geometry", stylers: [{ color: "#eeeae2" }] },
  { featureType: "water", elementType: "geometry", stylers: [{ color: "#cbdde1" }] },
] as const;

function polygonPath(polygon: MapOverlayPolygon): LatLng[] {
  return polygon.coordinates.map(([lng, lat]) => ({ lat, lng }));
}

function linePath(line: MapOverlayLine): LatLng[] {
  return line.coordinates.map(([lng, lat]) => ({ lat, lng }));
}

function compactMeta(place: NumberedPlace): string {
  return [
    typeof place.distance_km === "number" ? `${place.distance_km.toFixed(1)} km` : null,
    typeof place.rating === "number" ? `★ ${place.rating.toFixed(1)}` : null,
  ].filter((value): value is string => Boolean(value)).join(" · ");
}

function tooltipContent(place: NumberedPlace): HTMLDivElement {
  const content = document.createElement("div");
  content.className = "nearby-map-tooltip";
  const name = document.createElement("strong");
  name.textContent = place.name;
  content.append(name);
  const meta = compactMeta(place);
  if (meta) {
    const details = document.createElement("span");
    details.textContent = meta;
    content.append(details);
  }
  return content;
}

function addPolygon(
  map: Map2DElement,
  library: Maps2DLibrary,
  polygon: MapOverlayPolygon,
  colors: { fill: string; stroke: string },
  overlays: MapOverlay[],
) {
  const overlay = new library.Polygon({
    clickable: false,
    fillColor: colors.fill,
    fillOpacity: 0.16,
    map,
    paths: polygonPath(polygon),
    strokeColor: colors.stroke,
    strokeOpacity: 0.78,
    strokeWeight: 2,
  });
  overlays.push(overlay);
}

function addLine(
  map: Map2DElement,
  library: Maps2DLibrary,
  line: MapOverlayLine,
  color: string,
  onSelect: (() => void) | null,
  overlays: MapOverlay[],
) {
  const overlay = new library.Polyline({
    clickable: Boolean(onSelect),
    map,
    path: linePath(line),
    strokeColor: color,
    strokeOpacity: 0.9,
    strokeWeight: 5,
  });
  if (onSelect) overlay.addListener?.("click", onSelect);
  overlays.push(overlay);
}

export function AroundThisHomeGoogle2DMap(props: AroundThisHomeMapProps) {
  const {
    home,
    places,
    selectedId,
    metroLines,
    accessLines,
    redFlagLines,
    greenPatches = [],
    lakes = [],
    showMetroLines,
    waterTint,
    expanded,
    onSelectPlace,
    onSelectAccessLine,
    onSelectRedFlagLine,
    onBackToHome,
    onToggleExpanded,
  } = props;
  const containerRef = useRef<HTMLDivElement | null>(null);
  const mapRef = useRef<Map2DElement | null>(null);
  const libraryRef = useRef<Maps2DLibrary | null>(null);
  const markerLibraryRef = useRef<MarkerLibrary | null>(null);
  const overlaysRef = useRef<MapOverlay[]>([]);
  const infoWindowsRef = useRef<InfoWindow[]>([]);
  const [ready, setReady] = useState(false);
  const [loadError, setLoadError] = useState<Error | null>(null);

  useEffect(() => {
    let cancelled = false;
    void Promise.all([loadGoogleMaps2dLibrary(), loadGoogleMarkerLibrary()])
      .then(([mapsLibrary, markerLibrary]) => {
        if (cancelled || !containerRef.current) return;
        const maps = mapsLibrary as Maps2DLibrary;
        libraryRef.current = maps;
        markerLibraryRef.current = markerLibrary as MarkerLibrary;
        mapRef.current = new maps.Map(containerRef.current, {
          backgroundColor: "#eeeae2",
          clickableIcons: false,
          disableDefaultUI: true,
          gestureHandling: "cooperative",
          mapTypeId: "roadmap",
          styles: MAP_STYLE,
        });
        setReady(true);
      })
      .catch((error: unknown) => {
        if (!cancelled) {
          setLoadError(error instanceof Error ? error : new Error("google_maps_2d_unavailable"));
        }
      });
    return () => {
      cancelled = true;
      for (const overlay of overlaysRef.current) overlay.setMap(null);
      for (const infoWindow of infoWindowsRef.current) infoWindow.close();
      overlaysRef.current = [];
      infoWindowsRef.current = [];
      mapRef.current = null;
      libraryRef.current = null;
      markerLibraryRef.current = null;
    };
  }, []);

  useEffect(() => {
    mapRef.current?.setOptions({ gestureHandling: expanded ? "greedy" : "cooperative" });
  }, [expanded]);

  useEffect(() => {
    const map = mapRef.current;
    const library = libraryRef.current;
    const markerLibrary = markerLibraryRef.current;
    if (!ready || !map || !library || !markerLibrary) return;

    for (const overlay of overlaysRef.current) overlay.setMap(null);
    for (const infoWindow of infoWindowsRef.current) infoWindow.close();
    const overlays: MapOverlay[] = [];
    const infoWindows: InfoWindow[] = [];
    const bounds = new library.LatLngBounds();
    bounds.extend({ lat: home.latitude, lng: home.longitude });

    if (home.boundary) {
      addPolygon(map, library, home.boundary, { fill: "#f3c96f", stroke: "#9b762a" }, overlays);
      for (const point of polygonPath(home.boundary)) bounds.extend(point);
    }
    const showGreenPatches = places.some((place) => place.layer === "parks");
    for (const patch of showGreenPatches ? greenPatches : []) {
      addPolygon(map, library, patch, { fill: "#6e9d6e", stroke: "#507950" }, overlays);
    }
    for (const lake of waterTint ? lakes : []) {
      addPolygon(map, library, lake, { fill: "#4f9fc4", stroke: "#357fa7" }, overlays);
    }
    for (const line of showMetroLines ? metroLines : []) {
      addLine(map, library, line, "#7651a8", null, overlays);
      for (const point of linePath(line)) bounds.extend(point);
    }
    for (const line of accessLines) {
      addLine(map, library, line, "#575149", () => onSelectAccessLine(line.id), overlays);
      for (const point of linePath(line)) bounds.extend(point);
    }
    for (const line of redFlagLines) {
      addLine(map, library, line, "#b23f4c", () => onSelectRedFlagLine(line.id), overlays);
      for (const point of linePath(line)) bounds.extend(point);
    }

    const homeMarker = new markerLibrary.Marker({
      icon: {
        scaledSize: new library.Size(30, 30),
        url: mapMarkerIconUrl("home", "subdued"),
      },
      map,
      position: { lat: home.latitude, lng: home.longitude },
      title: home.name,
      zIndex: 1,
    });
    overlays.push(homeMarker);

    let activeInfoWindow: InfoWindow | null = null;
    for (const place of places) {
      const emphasis = place.id === selectedId
        ? "selected"
        : selectedId
        ? "subdued"
        : "active";
      const markerSize = emphasis === "selected" ? 42 : emphasis === "subdued" ? 28 : 34;
      const marker = new markerLibrary.Marker({
        icon: {
          scaledSize: new library.Size(markerSize, markerSize),
          url: mapMarkerIconUrl(place.icon, emphasis),
        },
        map,
        position: { lat: place.latitude, lng: place.longitude },
        title: place.name,
        zIndex: emphasis === "selected" ? 5 : 2,
      });
      const infoWindow = new library.InfoWindow({
        content: tooltipContent(place),
        disableAutoPan: true,
        headerDisabled: true,
      });
      marker.addListener?.("mouseover", () => {
        activeInfoWindow?.close();
        infoWindow.open({ anchor: marker, map });
        activeInfoWindow = infoWindow;
      });
      marker.addListener?.("mouseout", () => infoWindow.close());
      marker.addListener?.("click", () => {
        infoWindow.close();
        onSelectPlace(place);
      });
      overlays.push(marker);
      infoWindows.push(infoWindow);
      bounds.extend({ lat: place.latitude, lng: place.longitude });
    }

    overlaysRef.current = overlays;
    infoWindowsRef.current = infoWindows;
    if (places.length === 0 && accessLines.length === 0 && redFlagLines.length === 0) {
      map.setCenter({ lat: home.latitude, lng: home.longitude });
      map.setZoom(14);
    } else {
      map.fitBounds(bounds, 64);
    }
  }, [accessLines, greenPatches, home, lakes, metroLines, onSelectAccessLine, onSelectPlace, onSelectRedFlagLine, places, ready, redFlagLines, selectedId, showMetroLines, waterTint]);

  if (loadError) throw loadError;

  return (
    <div
      className={`nearby-map nearby-map--google${expanded ? " is-expanded" : ""}`}
      role="region"
      aria-label="Nearby evidence map"
      aria-busy={!ready}
      data-map-renderer="google-2d"
    >
      <div ref={containerRef} className="nearby-map__canvas nearby-map__canvas--google-2d" />
      <div className="nearby-map__actions">
        <button type="button" onClick={onBackToHome}>Back to home</button>
        <button type="button" onClick={onToggleExpanded}>
          {expanded ? "Close map" : "Expand map"}
        </button>
      </div>
    </div>
  );
}
