import { useEffect, useMemo, useRef } from "react";
import maplibregl, { type GeoJSONSource, type Map as MapLibreMap, type Marker } from "maplibre-gl";
import "maplibre-gl/dist/maplibre-gl.css";
import type { MapOverlayLine, MapWaterContext } from "../../lib/types.ts";
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
  showMetroLines: boolean;
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

  // Quiet numbered dots — names live in the list/receipt, not on the map.
  button.textContent = String(options.number ?? "");
  button.setAttribute("aria-label", `Place ${options.number}`);
  return button;
}

function emptyCollection() {
  return { type: "FeatureCollection" as const, features: [] as Array<Record<string, unknown>> };
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
        "line-color": "#3f5c8a",
        "line-width": 3.4,
        "line-dasharray": [1.6, 1.4],
        "line-opacity": 0.95,
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
  showMetroLines,
  water,
  waterTint,
  onSelectPlace,
  onSelectCluster,
}: AroundThisHomeMapProps) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const mapRef = useRef<MapLibreMap | null>(null);
  const markersRef = useRef<Marker[]>([]);
  const styleReadyRef = useRef(false);
  const homeRef = useRef(home);
  const onSelectPlaceRef = useRef(onSelectPlace);
  const onSelectClusterRef = useRef(onSelectCluster);

  useEffect(() => {
    homeRef.current = home;
  }, [home]);

  useEffect(() => {
    onSelectPlaceRef.current = onSelectPlace;
    onSelectClusterRef.current = onSelectCluster;
  }, [onSelectPlace, onSelectCluster]);

  const selectedPlace = useMemo(
    () => places.find((place) => place.id === selectedId) ?? places[0] ?? null,
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
      dragPan: false,
      dragRotate: false,
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
    });
    map.on("zoomend", () => {
      const anchor = homeRef.current;
      const center = map.getCenter();
      if (
        Math.abs(center.lng - anchor.longitude) > 0.00001
        || Math.abs(center.lat - anchor.latitude) > 0.00001
      ) {
        map.setCenter([anchor.longitude, anchor.latitude]);
      }
    });
    mapRef.current = map;

    return () => {
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
      center: [home.longitude, home.latitude],
      zoom: viewport.zoom,
    });
  }, [home.latitude, home.longitude, viewport.zoom, viewport.radiusKm]);

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
        "oe-metro-lines",
        linesToFeatureCollection(showMetroLines ? metroLines : []),
      );
    };

    if (styleReadyRef.current || map.isStyleLoaded()) {
      syncOverlays();
      return;
    }
    map.once("load", syncOverlays);
  }, [home, metroLines, showMetroLines, viewport.radiusKm]);

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
        selected: place.id === selectedId || place.id === selectedPlace?.id,
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
  }, [clusters, home.latitude, home.longitude, places, selectedId, selectedPlace?.id]);

  return (
    <div className={`nearby-map${waterTint && water ? " nearby-map--water-tint" : ""}`}>
      <div ref={containerRef} className="nearby-map__canvas" role="presentation" />
      <div className="nearby-map__chrome" aria-hidden="true">
        <span>Home centered</span>
        <span>{viewport.radiusKm < 1
          ? `${Math.round(viewport.radiusKm * 1000)} m`
          : `${viewport.radiusKm.toFixed(viewport.radiusKm < 3 ? 1 : 0)} km`}
        </span>
        {homeApproximated && <span>Home estimated</span>}
        {showMetroLines && metroLines.length > 0 && <span>Metro corridor</span>}
      </div>
    </div>
  );
}
