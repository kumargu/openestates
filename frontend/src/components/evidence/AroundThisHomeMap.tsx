import { useEffect, useMemo, useRef } from "react";
import maplibregl, { type GeoJSONSource, type Map as MapLibreMap, type Marker } from "maplibre-gl";
import "maplibre-gl/dist/maplibre-gl.css";
import type { MapOverlayLine, MapWaterContext } from "../../lib/types.ts";
import { NOTEBOOK_SAVE_ICON_PATH } from "../notebook/NotebookSaveIcon.tsx";
import {
  NEARBY_MAP_STYLE,
  type NumberedPlace,
  type PlaceCluster,
  type PlateViewport,
} from "../../lib/nearbyPlateProjection.ts";

export type AroundThisHomeMapProps = {
  home: { latitude: number; longitude: number; name: string };
  places: NumberedPlace[];
  clusters: PlaceCluster[];
  selectedId: string | null;
  viewport: PlateViewport;
  metroLines: MapOverlayLine[];
  redFlagLines: MapOverlayLine[];
  showMetroLines: boolean;
  nearestMetroDistanceKm?: number;
  water?: MapWaterContext | null;
  waterTint: boolean;
  pinnedPlaceIds?: string[];
  onSelectPlace: (id: string) => void;
  onSelectCluster: (cluster: PlaceCluster) => void;
  onRememberPlace?: (place: NumberedPlace) => void;
};

function markerEl(
  kind: "home" | "place" | "cluster",
  options: {
    count?: number;
    selected?: boolean;
    layer?: string;
    name?: string;
  } = {},
): HTMLButtonElement {
  const button = document.createElement("button");
  button.type = "button";
  button.className = [
    "nearby-map-marker",
    `nearby-map-marker--${kind}`,
    options.layer ? `nearby-map-marker--${options.layer}` : "",
    options.selected ? "is-selected" : "",
  ].filter(Boolean).join(" ");

  if (kind === "home") {
    button.innerHTML = `
      <svg viewBox="0 0 24 30" aria-hidden="true">
        <path d="M12 29C9.8 25.6 4 19.2 4 12a8 8 0 1 1 16 0c0 7.2-5.8 13.6-8 17Z" />
        <circle cx="12" cy="12" r="3.2" />
      </svg>`;
    button.setAttribute("aria-label", "This home");
    return button;
  }

  if (kind === "cluster") {
    button.textContent = String(options.count ?? 0);
    button.setAttribute("aria-label", `${options.count} places nearby`);
    return button;
  }

  button.innerHTML = markerGlyph(options.layer);
  button.setAttribute("aria-label", options.name ?? "Nearby place");
  return button;
}

function markerGlyph(layer?: string): string {
  switch (layer) {
    case "metro":
      return `<svg viewBox="0 0 24 24" aria-hidden="true"><rect x="6" y="4" width="12" height="13" rx="3"/><path d="M6 11h12M8 20l2-3m6 3-2-3"/><circle cx="9" cy="14" r=".8"/><circle cx="15" cy="14" r=".8"/></svg>`;
    case "schools":
      return `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="m3 9 9-4 9 4-9 4-9-4Z"/><path d="M7 11v4c0 1.2 2.2 2.3 5 2.3s5-1.1 5-2.3v-4"/></svg>`;
    case "hospitals":
      return `<svg viewBox="0 0 24 24" aria-hidden="true"><rect x="4" y="4" width="16" height="16" rx="3"/><path d="M12 8v8M8 12h8"/></svg>`;
    case "tech":
      return `<svg viewBox="0 0 24 24" aria-hidden="true"><rect x="5" y="3" width="14" height="18" rx="2"/><path d="M9 7h2m2 0h2M9 11h2m2 0h2M9 15h6"/></svg>`;
    case "red_flags":
      return `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6.5 20V5"/><path d="M7 5.4h8.8l-1.4 3 1.4 3H7"/></svg>`;
    case "lakes":
      return `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 14c2 1.8 4 1.8 6 0s4-1.8 6 0 4 1.8 6 0"/><path d="M4 18c2 1.8 4 1.8 6 0s4-1.8 6 0 4 1.8 6 0"/></svg>`;
    case "breweries":
      return `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M7 8h9v10a3 3 0 0 1-3 3h-3a3 3 0 0 1-3-3V8Z"/><path d="M16 11h2a2 2 0 0 1 0 4h-2"/><path d="M8 5h7"/></svg>`;
    default:
      return `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 21s-6-5.2-6-10a6 6 0 0 1 12 0c0 4.8-6 10-6 10Z"/><circle cx="12" cy="11" r="2"/></svg>`;
  }
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

function placePopupHtml(place: NumberedPlace, pinned: boolean): string {
  const meta = [
    typeof place.distance_km === "number" ? `${place.distance_km.toFixed(1)} km` : null,
    place.layer !== "red_flags" && typeof place.rating === "number" ? `${place.rating.toFixed(1)} rating` : null,
    place.layer !== "red_flags" && typeof place.review_count === "number" ? `${place.review_count} reviews` : null,
    place.note ?? null,
  ].filter(Boolean).join(" · ");
  return `
    <div class="nearby-map-popup__body">
      <div class="nearby-map-popup__copy">
        <strong>${escapeHtml(place.name)}</strong>
        ${meta ? `<span>${escapeHtml(meta)}</span>` : ""}
      </div>
      <button
        type="button"
        class="nearby-map-popup__pin${pinned ? " is-filled" : ""}"
        data-notebook-pin="${escapeHtml(place.id)}"
        aria-label="${pinned ? "Remove from notebook" : "Save to notebook"}"
        title="${pinned ? "Saved" : "Save"}"
      >
        <svg viewBox="0 0 24 24" width="14" height="14" aria-hidden="true">
          <path
            d="${NOTEBOOK_SAVE_ICON_PATH}"
            fill="${pinned ? "currentColor" : "none"}"
            stroke="currentColor"
            stroke-width="1.9"
            stroke-linecap="round"
            stroke-linejoin="round"
          />
        </svg>
      </button>
    </div>
  `;
}

function metroBadgeEl(name: string): HTMLDivElement {
  const element = document.createElement("div");
  const tone = name.toLowerCase().match(/purple|green|yellow|pink|blue|orange/)?.[0] ?? "default";
  element.className = `nearby-map-metro-badge nearby-map-metro-badge--${tone}`;
  element.innerHTML = markerGlyph("metro");
  element.setAttribute("aria-label", `${name} metro corridor`);
  return element;
}

function metroBadgePoints(
  lines: MapOverlayLine[],
  home: { latitude: number; longitude: number },
): Array<{ name: string; coordinate: [number, number] }> {
  const nearestByName = new Map<string, { coordinate: [number, number]; distance: number }>();
  const latitudeScale = Math.cos((home.latitude * Math.PI) / 180);
  for (const line of lines) {
    for (const coordinate of line.coordinates) {
      const dx = (coordinate[0] - home.longitude) * latitudeScale;
      const dy = coordinate[1] - home.latitude;
      const distance = dx * dx + dy * dy;
      const current = nearestByName.get(line.name);
      if (!current || distance < current.distance) {
        nearestByName.set(line.name, { coordinate, distance });
      }
    }
  }
  return [...nearestByName].map(([name, value]) => ({ name, coordinate: value.coordinate }));
}

function emptyCollection() {
  return { type: "FeatureCollection" as const, features: [] as Array<Record<string, unknown>> };
}

function linesToFeatureCollection(
  lines: MapOverlayLine[],
  options: {
    color?: string;
    label?: (line: MapOverlayLine) => string;
  } = {},
) {
  return {
    type: "FeatureCollection" as const,
    features: lines.map((line) => ({
      type: "Feature" as const,
      properties: {
        id: line.id,
        name: line.name,
        label: options.label?.(line) ?? line.name,
        color: options.color ?? metroLineColor(line.name),
      },
      geometry: {
        type: "LineString" as const,
        coordinates: line.coordinates,
      },
    })),
  };
}

function redFlagLineLabel(line: MapOverlayLine): string {
  const normalized = line.name.toLowerCase();
  if (normalized.includes("transmission") || normalized.includes("voltage")) {
    return "Transmission line";
  }
  if (normalized.includes("drain") || normalized.includes("stormwater")) {
    return "Stormwater drain";
  }
  return "Red flag";
}

function metroLineColor(name: string): string {
  const normalized = name.toLowerCase();
  if (normalized.includes("purple")) return "#7651a8";
  if (normalized.includes("green")) return "#2f8a58";
  if (normalized.includes("yellow")) return "#d4a900";
  if (normalized.includes("pink")) return "#d85d8d";
  if (normalized.includes("blue")) return "#397fb5";
  if (normalized.includes("orange")) return "#d87932";
  return "#526d91";
}

function ringFeatureCollection(
  home: { latitude: number; longitude: number },
  radiiKm: number[],
) {
  const features = radiiKm.map((radiusKm) => {
    const points: [number, number][] = [];
    for (let step = 0; step <= 64; step += 1) {
      const angle = (step / 64) * Math.PI * 2;
      const dLat = (radiusKm / 110.57) * Math.cos(angle);
      const dLng = (radiusKm / (111.32 * Math.cos((home.latitude * Math.PI) / 180)))
        * Math.sin(angle);
      points.push([home.longitude + dLng, home.latitude + dLat]);
    }
    return {
      type: "Feature" as const,
      properties: { radiusKm },
      geometry: {
        type: "LineString" as const,
        coordinates: points,
      },
    };
  });
  return { type: "FeatureCollection" as const, features };
}

function waterZoneFeatureCollection(
  home: { latitude: number; longitude: number },
  water: MapWaterContext,
) {
  const scopeRadiusKm = water.scope_radius_km ?? 3;
  const majorRadiusKm = scopeRadiusKm * 0.75;
  const minorRadiusKm = scopeRadiusKm * 0.45;
  const rotation = 18 * (Math.PI / 180);
  const points: [number, number][] = [];

  for (let step = 0; step <= 64; step += 1) {
    const angle = (step / 64) * Math.PI * 2;
    const x = majorRadiusKm * Math.cos(angle);
    const y = minorRadiusKm * Math.sin(angle);
    const rotatedX = x * Math.cos(rotation) - y * Math.sin(rotation);
    const rotatedY = x * Math.sin(rotation) + y * Math.cos(rotation);
    const latitude = home.latitude + rotatedY / 110.57;
    const longitude = home.longitude
      + rotatedX / (111.32 * Math.cos((home.latitude * Math.PI) / 180));
    points.push([longitude, latitude]);
  }

  return {
    type: "FeatureCollection" as const,
    features: [{
      type: "Feature" as const,
      properties: {
        label: `${water.groundwater_class} groundwater`,
        scope: `Around ${scopeRadiusKm.toFixed(0)} km`,
      },
      geometry: {
        type: "Polygon" as const,
        coordinates: [points],
      },
    }],
  };
}

function ringRadiiForViewport(radiusKm: number): number[] {
  if (radiusKm <= 0.6) return [0.25, 0.5];
  if (radiusKm <= 1.2) return [0.5, 1];
  if (radiusKm <= 2) return [0.5, 1, 2];
  if (radiusKm <= 3) return [1, 2];
  return [2, 5];
}

/** Strip basemap POI/label noise so the plate reads like a focused activity map. */
function quietBasemap(map: MapLibreMap) {
  const style = map.getStyle();
  if (!style?.layers) return;
  for (const layer of style.layers) {
    const id = layer.id.toLowerCase();
    if (id.startsWith("oe-")) continue;
    const isLabel = layer.type === "symbol";
    const isPoiNoise = /poi|place-|housenumber|transit|rail|airport|golf|pitch/.test(id);
    if (isLabel || isPoiNoise) {
      try {
        map.setLayoutProperty(layer.id, "visibility", "none");
      } catch {
        // Some style layers are immutable; ignore.
      }
    }
  }
}

function ensureOverlayLayers(map: MapLibreMap) {
  if (!map.getSource("oe-water-zone")) {
    map.addSource("oe-water-zone", { type: "geojson", data: emptyCollection() });
    map.addLayer({
      id: "oe-water-zone-fill",
      type: "fill",
      source: "oe-water-zone",
      paint: {
        "fill-color": "#78b8b3",
        "fill-opacity": 0.2,
      },
    });
    map.addLayer({
      id: "oe-water-zone-line",
      type: "line",
      source: "oe-water-zone",
      paint: {
        "line-color": "#3f8884",
        "line-width": 2,
        "line-dasharray": [2, 1.5],
        "line-opacity": 0.8,
      },
    });
    map.addLayer({
      id: "oe-water-zone-label",
      type: "symbol",
      source: "oe-water-zone",
      layout: {
        "text-field": ["concat", ["get", "label"], "\n", ["get", "scope"]],
        "text-font": ["Noto Sans Regular"],
        "text-size": 12,
        "text-line-height": 1.25,
        "text-anchor": "center",
        "text-offset": [0, -2.4],
      },
      paint: {
        "text-color": "#285f5d",
        "text-halo-color": "rgba(255, 255, 255, 0.86)",
        "text-halo-width": 1.5,
      },
    });
  }

  if (!map.getSource("oe-rings")) {
    map.addSource("oe-rings", { type: "geojson", data: emptyCollection() });
    map.addLayer({
      id: "oe-rings",
      type: "line",
      source: "oe-rings",
      paint: {
        "line-color": "rgba(0, 0, 0, 0.14)",
        "line-width": 1,
        "line-opacity": 0.65,
      },
    });
  }

  if (!map.getSource("oe-metro-lines")) {
    map.addSource("oe-metro-lines", {
      type: "geojson",
      data: emptyCollection(),
    });
    map.addLayer({
      id: "oe-metro-lines-casing",
      type: "line",
      source: "oe-metro-lines",
      layout: {
        "line-cap": "round",
        "line-join": "round",
      },
      paint: {
        "line-color": "rgba(255, 255, 255, 0.9)",
        "line-width": 6.5,
        "line-opacity": 0.95,
      },
    });
    map.addLayer({
      id: "oe-metro-lines",
      type: "line",
      source: "oe-metro-lines",
      layout: {
        "line-cap": "round",
        "line-join": "round",
      },
      paint: {
        "line-color": ["get", "color"],
        "line-width": 4.2,
        "line-opacity": 0.95,
      },
    });
    map.addLayer({
      id: "oe-metro-lines-label",
      type: "symbol",
      source: "oe-metro-lines",
      layout: {
        "symbol-placement": "line",
        "text-field": ["get", "label"],
        "text-font": ["Noto Sans Regular"],
        "text-size": 11,
        "text-letter-spacing": 0,
        "text-padding": 10,
        "text-rotation-alignment": "map",
      },
      paint: {
        "text-color": "#375a83",
        "text-halo-color": "rgba(255, 255, 255, 0.92)",
        "text-halo-width": 1.6,
      },
    });
  }

  if (!map.getSource("oe-red-flag-lines")) {
    map.addSource("oe-red-flag-lines", {
      type: "geojson",
      data: emptyCollection(),
    });
    map.addLayer({
      id: "oe-red-flag-lines-casing",
      type: "line",
      source: "oe-red-flag-lines",
      layout: {
        "line-cap": "round",
        "line-join": "round",
      },
      paint: {
        "line-color": "rgba(255, 255, 255, 0.92)",
        "line-width": 7,
        "line-opacity": 0.96,
      },
    });
    map.addLayer({
      id: "oe-red-flag-lines",
      type: "line",
      source: "oe-red-flag-lines",
      layout: {
        "line-cap": "round",
        "line-join": "round",
      },
      paint: {
        "line-color": ["get", "color"],
        "line-width": 4.6,
        "line-dasharray": [1.4, 1],
        "line-opacity": 0.96,
      },
    });
    map.addLayer({
      id: "oe-red-flag-lines-label",
      type: "symbol",
      source: "oe-red-flag-lines",
      layout: {
        "symbol-placement": "line",
        "text-field": ["get", "label"],
        "text-font": ["Noto Sans Regular"],
        "text-size": 11,
        "text-letter-spacing": 0,
        "text-padding": 12,
        "text-rotation-alignment": "map",
      },
      paint: {
        "text-color": "#9a2634",
        "text-halo-color": "rgba(255, 255, 255, 0.94)",
        "text-halo-width": 1.8,
      },
    });
  }
}

function setSourceData(
  map: MapLibreMap,
  sourceId: string,
  data: {
    type: "FeatureCollection";
    features: Array<Record<string, unknown>>;
  },
) {
  const source = map.getSource(sourceId) as GeoJSONSource | undefined;
  source?.setData(data as never);
}

export function AroundThisHomeMap({
  home,
  places,
  clusters,
  selectedId,
  viewport,
  metroLines,
  redFlagLines,
  showMetroLines,
  nearestMetroDistanceKm,
  water,
  waterTint,
  pinnedPlaceIds = [],
  onSelectPlace,
  onSelectCluster,
  onRememberPlace,
}: AroundThisHomeMapProps) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const mapRef = useRef<MapLibreMap | null>(null);
  const markersRef = useRef<Marker[]>([]);
  const styleReadyRef = useRef(false);
  const viewportCenterRef = useRef(viewport.center);
  const onSelectPlaceRef = useRef(onSelectPlace);
  const onSelectClusterRef = useRef(onSelectCluster);
  const onRememberPlaceRef = useRef(onRememberPlace);
  const pinnedPlaceIdsRef = useRef(new Set(pinnedPlaceIds));

  useEffect(() => {
    viewportCenterRef.current = viewport.center;
  }, [viewport.center]);

  useEffect(() => {
    onSelectPlaceRef.current = onSelectPlace;
    onSelectClusterRef.current = onSelectCluster;
    onRememberPlaceRef.current = onRememberPlace;
  }, [onSelectPlace, onSelectCluster, onRememberPlace]);

  useEffect(() => {
    pinnedPlaceIdsRef.current = new Set(pinnedPlaceIds);
  }, [pinnedPlaceIds]);

  const selectedPlace = useMemo(
    () => places.find((place) => place.id === selectedId) ?? places[0] ?? null,
    [places, selectedId],
  );
  const metroLineLabel = useMemo(
    () => [...new Set(metroLines.map((line) => line.name).filter(Boolean))].join(" · "),
    [metroLines],
  );

  useEffect(() => {
    if (!containerRef.current || mapRef.current) return;

    const container = containerRef.current;
    const map = new maplibregl.Map({
      container,
      style: NEARBY_MAP_STYLE,
      center: [viewport.center.longitude, viewport.center.latitude],
      zoom: viewport.zoom,
      attributionControl: { compact: true },
      dragPan: false,
      dragRotate: false,
      scrollZoom: false,
      pitchWithRotate: false,
      touchPitch: false,
      keyboard: false,
    });
    map.addControl(
      new maplibregl.NavigationControl({ showCompass: false, visualizePitch: false }),
      "top-right",
    );
    map.on("load", () => {
      quietBasemap(map);
      ensureOverlayLayers(map);
      styleReadyRef.current = true;
      map.resize();
      map.jumpTo({
        center: [viewportCenterRef.current.longitude, viewportCenterRef.current.latitude],
      });
    });
    map.on("zoomend", () => {
      const anchor = viewportCenterRef.current;
      const center = map.getCenter();
      if (
        Math.abs(center.lng - anchor.longitude) > 0.00001
        || Math.abs(center.lat - anchor.latitude) > 0.00001
      ) {
        map.setCenter([anchor.longitude, anchor.latitude]);
      }
    });
    mapRef.current = map;

    let resizeFrame: number | null = null;
    const resizeMap = () => {
      if (resizeFrame !== null) window.cancelAnimationFrame(resizeFrame);
      resizeFrame = window.requestAnimationFrame(() => {
        resizeFrame = null;
        map.resize();
        const anchor = viewportCenterRef.current;
        map.jumpTo({ center: [anchor.longitude, anchor.latitude] });
      });
    };
    const resizeObserver = typeof ResizeObserver === "undefined"
      ? null
      : new ResizeObserver(resizeMap);
    resizeObserver?.observe(container);
    resizeMap();

    return () => {
      resizeObserver?.disconnect();
      if (resizeFrame !== null) window.cancelAnimationFrame(resizeFrame);
      for (const marker of markersRef.current) marker.remove();
      markersRef.current = [];
      map.remove();
      mapRef.current = null;
      styleReadyRef.current = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    const map = mapRef.current;
    if (!map) return;
    map.jumpTo({
      center: [viewport.center.longitude, viewport.center.latitude],
      zoom: viewport.zoom,
    });
  }, [viewport.center.latitude, viewport.center.longitude, viewport.zoom, viewport.radiusKm]);

  useEffect(() => {
    const map = mapRef.current;
    if (!map) return;

    const syncOverlays = () => {
      quietBasemap(map);
      ensureOverlayLayers(map);
      setSourceData(
        map,
        "oe-rings",
        ringFeatureCollection(home, ringRadiiForViewport(viewport.radiusKm)),
      );
      setSourceData(
        map,
        "oe-water-zone",
        waterTint && water
          ? waterZoneFeatureCollection(home, water)
          : emptyCollection(),
      );
      setSourceData(
        map,
        "oe-metro-lines",
        linesToFeatureCollection(showMetroLines ? metroLines : []),
      );
      setSourceData(
        map,
        "oe-red-flag-lines",
        linesToFeatureCollection(redFlagLines, {
          color: "#c93f3f",
          label: redFlagLineLabel,
        }),
      );
    };

    if (styleReadyRef.current || map.isStyleLoaded()) {
      syncOverlays();
      return;
    }
    map.once("load", syncOverlays);
  }, [home, metroLines, redFlagLines, showMetroLines, viewport.radiusKm, water, waterTint]);

  useEffect(() => {
    const map = mapRef.current;
    if (!map) return;

    for (const marker of markersRef.current) marker.remove();
    markersRef.current = [];

    const homeMarker = new maplibregl.Marker({
      element: markerEl("home"),
      anchor: "center",
    })
      .setLngLat([home.longitude, home.latitude])
      .addTo(map);
    markersRef.current.push(homeMarker);

    if (showMetroLines) {
      for (const badge of metroBadgePoints(metroLines, home)) {
        const marker = new maplibregl.Marker({
          element: metroBadgeEl(badge.name),
          anchor: "center",
        })
          .setLngLat(badge.coordinate)
          .addTo(map);
        markersRef.current.push(marker);
      }
    }

    for (const cluster of clusters) {
      const element = markerEl("cluster", { count: cluster.count });
      element.addEventListener("click", (event) => {
        event.stopPropagation();
        onSelectClusterRef.current(cluster);
      });
      const marker = new maplibregl.Marker({ element, anchor: "center" })
        .setLngLat([cluster.longitude, cluster.latitude])
        .addTo(map);
      markersRef.current.push(marker);
    }

    for (const place of places) {
      if (clusters.some((cluster) => cluster.placeIds.includes(place.id))) continue;
      const element = markerEl("place", {
        selected: place.id === selectedId || place.id === selectedPlace?.id,
        layer: place.layer,
        name: place.name,
      });
      const popup = new maplibregl.Popup({
        closeButton: false,
        closeOnClick: false,
        offset: 18,
        className: "nearby-map-popup",
      })
        .setLngLat([place.longitude, place.latitude])
        .setHTML(placePopupHtml(place, pinnedPlaceIdsRef.current.has(place.id)));

      let hideTimer: number | undefined;
      let lastPointerPinAt = 0;
      const showPopup = () => {
        window.clearTimeout(hideTimer);
        popup.setHTML(placePopupHtml(place, pinnedPlaceIdsRef.current.has(place.id)));
        popup.addTo(map);
        bindPopupControls();
      };
      const keepPopupOpen = () => {
        window.clearTimeout(hideTimer);
      };
      const hidePopup = () => {
        hideTimer = window.setTimeout(() => {
          const root = popup.getElement();
          if (
            root?.matches(":hover")
            || root?.contains(document.activeElement)
            || element.matches(":hover")
          ) {
            return;
          }
          popup.remove();
        }, 360);
      };
      const togglePopupPin = (event: Event) => {
        event.preventDefault();
        event.stopPropagation();
        window.clearTimeout(hideTimer);
        const nextPinned = !pinnedPlaceIdsRef.current.has(place.id);
        if (nextPinned) pinnedPlaceIdsRef.current.add(place.id);
        else pinnedPlaceIdsRef.current.delete(place.id);
        onRememberPlaceRef.current?.(place);
        popup.setHTML(placePopupHtml(place, nextPinned));
        bindPopupControls();
      };
      const bindPopupControls = () => {
        const root = popup.getElement();
        if (!root) return;
        if (root.dataset.oeNotebookBound !== "true") {
          root.dataset.oeNotebookBound = "true";
          root.addEventListener("pointerenter", keepPopupOpen);
          root.addEventListener("pointerleave", hidePopup);
          root.addEventListener("focusin", keepPopupOpen);
          root.addEventListener("focusout", hidePopup);
        }
        const pin = root.querySelector<HTMLButtonElement>("[data-notebook-pin]");
        if (!pin || pin.dataset.oeNotebookBound === "true") return;
        pin.dataset.oeNotebookBound = "true";
        pin.addEventListener("pointerdown", (event) => {
          lastPointerPinAt = window.performance.now();
          togglePopupPin(event);
        });
        pin.addEventListener("click", (event) => {
          if (window.performance.now() - lastPointerPinAt < 400) {
            event.preventDefault();
            event.stopPropagation();
            return;
          }
          togglePopupPin(event);
        });
      };

      popup.on("open", bindPopupControls);

      element.addEventListener("pointerenter", showPopup);
      element.addEventListener("pointerleave", hidePopup);
      element.addEventListener("focus", showPopup);
      element.addEventListener("blur", hidePopup);
      element.addEventListener("click", (event) => {
        event.stopPropagation();
        onSelectPlaceRef.current(place.id);
      });
      const marker = new maplibregl.Marker({ element, anchor: "center" })
        .setLngLat([place.longitude, place.latitude])
        .addTo(map);
      markersRef.current.push(marker);
    }
  }, [
    clusters,
    home,
    metroLines,
    places,
    selectedId,
    selectedPlace?.id,
    showMetroLines,
    pinnedPlaceIds,
  ]);

  return (
    <div
      className="nearby-map"
      role="region"
      aria-label={`Map of places around ${home.name}`}
    >
      <div ref={containerRef} className="nearby-map__canvas" role="presentation" />
      <div className="nearby-map__chrome" aria-hidden="true">
        <span>{viewport.radiusKm < 1
          ? `${Math.round(viewport.radiusKm * 1000)} m`
          : `${viewport.radiusKm.toFixed(viewport.radiusKm < 3 ? 1 : 0)} km`}
        </span>
        {typeof nearestMetroDistanceKm === "number" && (
          <span>Metro · {nearestMetroDistanceKm.toFixed(1)} km</span>
        )}
        {showMetroLines && metroLineLabel && (
          <span>{metroLineLabel}</span>
        )}
      </div>
    </div>
  );
}
