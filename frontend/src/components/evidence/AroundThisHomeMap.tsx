import { useEffect, useMemo, useRef } from "react";
import maplibregl, { type GeoJSONSource, type Map as MapLibreMap, type Marker } from "maplibre-gl";
import "maplibre-gl/dist/maplibre-gl.css";
import type {
  MapOverlayLine,
  MapOverlayPolygon,
  MapWaterContext,
} from "../../lib/types.ts";
import {
  NEARBY_MAP_STYLE,
  type NumberedPlace,
  type PlaceCluster,
  type PlateViewport,
} from "../../lib/nearbyPlateProjection.ts";

type AroundThisHomeMapProps = {
  home: { latitude: number; longitude: number; name: string };
  homeApproximated: boolean;
  places: NumberedPlace[];
  clusters: PlaceCluster[];
  selectedId: string | null;
  viewport: PlateViewport;
  metroLines: MapOverlayLine[];
  greenPatches: MapOverlayPolygon[];
  lakes: MapOverlayPolygon[];
  showMetroLines: boolean;
  showGreen: boolean;
  water?: MapWaterContext | null;
  waterTint: boolean;
  onSelectPlace: (id: string) => void;
  onSelectCluster: (cluster: PlaceCluster) => void;
};

function markerEl(
  kind: "home" | "place" | "cluster",
  options: {
    number?: number;
    count?: number;
    selected?: boolean;
    layer?: string;
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
    button.innerHTML = `<span class="nearby-map-marker__home-dot"></span><span class="nearby-map-marker__home-label">Home</span>`;
    button.setAttribute("aria-label", "This home");
    return button;
  }

  if (kind === "cluster") {
    button.textContent = String(options.count ?? 0);
    button.setAttribute("aria-label", `${options.count} places nearby`);
    return button;
  }

  button.textContent = String(options.number ?? "");
  button.setAttribute("aria-label", `Place ${options.number}`);
  return button;
}

function linesToFeatureCollection(lines: MapOverlayLine[]) {
  return {
    type: "FeatureCollection" as const,
    features: lines.map((line) => ({
      type: "Feature" as const,
      properties: { id: line.id, name: line.name },
      geometry: {
        type: "LineString" as const,
        coordinates: line.coordinates,
      },
    })),
  };
}

function polygonsToFeatureCollection(polygons: MapOverlayPolygon[]) {
  return {
    type: "FeatureCollection" as const,
    features: polygons.map((polygon) => ({
      type: "Feature" as const,
      properties: {
        id: polygon.id,
        name: polygon.name,
        kind: polygon.kind,
      },
      geometry: {
        type: "Polygon" as const,
        coordinates: [polygon.coordinates],
      },
    })),
  };
}

function ensureOverlayLayers(map: MapLibreMap) {
  if (!map.getSource("oe-metro-lines")) {
    map.addSource("oe-metro-lines", {
      type: "geojson",
      data: { type: "FeatureCollection", features: [] },
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
        "line-color": "#3f5c8a",
        "line-width": 3.2,
        "line-dasharray": [1.2, 1.6],
        "line-opacity": 0.9,
      },
    });
  }

  if (!map.getSource("oe-green-patches")) {
    map.addSource("oe-green-patches", {
      type: "geojson",
      data: { type: "FeatureCollection", features: [] },
    });
    map.addLayer({
      id: "oe-green-patches-fill",
      type: "fill",
      source: "oe-green-patches",
      paint: {
        "fill-color": "#6f9b6f",
        "fill-opacity": 0.28,
      },
    });
    map.addLayer({
      id: "oe-green-patches-outline",
      type: "line",
      source: "oe-green-patches",
      paint: {
        "line-color": "#4f7a4f",
        "line-width": 1.2,
        "line-opacity": 0.7,
      },
    });
  }

  if (!map.getSource("oe-lakes")) {
    map.addSource("oe-lakes", {
      type: "geojson",
      data: { type: "FeatureCollection", features: [] },
    });
    map.addLayer({
      id: "oe-lakes-fill",
      type: "fill",
      source: "oe-lakes",
      paint: {
        "fill-color": "#6f9fba",
        "fill-opacity": 0.32,
      },
    });
    map.addLayer({
      id: "oe-lakes-outline",
      type: "line",
      source: "oe-lakes",
      paint: {
        "line-color": "#4d7777",
        "line-width": 1.1,
        "line-opacity": 0.75,
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
  homeApproximated,
  places,
  clusters,
  selectedId,
  viewport,
  metroLines,
  greenPatches,
  lakes,
  showMetroLines,
  showGreen,
  water,
  waterTint,
  onSelectPlace,
  onSelectCluster,
}: AroundThisHomeMapProps) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const mapRef = useRef<MapLibreMap | null>(null);
  const markersRef = useRef<Marker[]>([]);
  const styleReadyRef = useRef(false);
  const onSelectPlaceRef = useRef(onSelectPlace);
  const onSelectClusterRef = useRef(onSelectCluster);

  useEffect(() => {
    onSelectPlaceRef.current = onSelectPlace;
    onSelectClusterRef.current = onSelectCluster;
  }, [onSelectPlace, onSelectCluster]);

  const selectedPlace = useMemo(
    () => places.find((place) => place.id === selectedId) ?? null,
    [places, selectedId],
  );

  useEffect(() => {
    if (!containerRef.current || mapRef.current) return;

    const map = new maplibregl.Map({
      container: containerRef.current,
      style: NEARBY_MAP_STYLE,
      center: [home.longitude, home.latitude],
      zoom: viewport.zoom,
      attributionControl: { compact: true },
      interactive: true,
      dragRotate: false,
      pitchWithRotate: false,
    });
    map.addControl(new maplibregl.NavigationControl({ showCompass: false }), "top-right");
    map.on("load", () => {
      ensureOverlayLayers(map);
      styleReadyRef.current = true;
    });
    mapRef.current = map;

    return () => {
      for (const marker of markersRef.current) marker.remove();
      markersRef.current = [];
      map.remove();
      mapRef.current = null;
      styleReadyRef.current = false;
    };
    // Mount once for this home anchor; pan/zoom updates live below.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    const map = mapRef.current;
    if (!map) return;
    map.easeTo({
      center: [home.longitude, home.latitude],
      zoom: viewport.zoom,
      duration: 450,
    });
  }, [home.latitude, home.longitude, viewport.zoom, viewport.radiusKm]);

  useEffect(() => {
    const map = mapRef.current;
    if (!map) return;

    const syncOverlays = () => {
      ensureOverlayLayers(map);
      setSourceData(
        map,
        "oe-metro-lines",
        linesToFeatureCollection(showMetroLines ? metroLines : []),
      );
      setSourceData(
        map,
        "oe-green-patches",
        polygonsToFeatureCollection(showGreen ? greenPatches : []),
      );
      setSourceData(
        map,
        "oe-lakes",
        polygonsToFeatureCollection(showGreen ? lakes : []),
      );
    };

    if (styleReadyRef.current || map.isStyleLoaded()) {
      syncOverlays();
      return;
    }
    map.once("load", syncOverlays);
  }, [greenPatches, lakes, metroLines, showGreen, showMetroLines]);

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
        number: place.number,
        selected: place.id === selectedId,
        layer: place.layer,
      });
      element.addEventListener("click", (event) => {
        event.stopPropagation();
        onSelectPlaceRef.current(place.id);
      });
      const marker = new maplibregl.Marker({ element, anchor: "center" })
        .setLngLat([place.longitude, place.latitude])
        .addTo(map);
      markersRef.current.push(marker);
    }
  }, [clusters, home.latitude, home.longitude, places, selectedId]);

  useEffect(() => {
    const map = mapRef.current;
    if (!map || !selectedPlace) return;
    map.easeTo({
      center: [
        (home.longitude + selectedPlace.longitude) / 2,
        (home.latitude + selectedPlace.latitude) / 2,
      ],
      duration: 350,
    });
  }, [home.latitude, home.longitude, selectedPlace]);

  return (
    <div className={`nearby-map${waterTint && water ? " nearby-map--water-tint" : ""}`}>
      <div ref={containerRef} className="nearby-map__canvas" role="presentation" />
      <div className="nearby-map__chrome" aria-hidden="true">
        <span>{viewport.radiusKm < 1
          ? `${Math.round(viewport.radiusKm * 1000)} m scale`
          : `${viewport.radiusKm.toFixed(viewport.radiusKm < 3 ? 1 : 0)} km scale`}
        </span>
        {homeApproximated && <span>Home pin estimated from nearby places</span>}
        {showMetroLines && metroLines.length > 0 && <span>Metro stretch ≤ 15 km</span>}
        {showGreen && (greenPatches.length > 0 || lakes.length > 0) && (
          <span>Green + lakes ≤ 4 km</span>
        )}
        {waterTint && water && (
          <span>{water.groundwater_class} groundwater context</span>
        )}
      </div>
    </div>
  );
}
