import { useEffect, useMemo, useRef } from "react";
import maplibregl, { type Map as MapLibreMap, type Marker } from "maplibre-gl";
import "maplibre-gl/dist/maplibre-gl.css";
import type { MapWaterContext } from "../../lib/types.ts";
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

export function AroundThisHomeMap({
  home,
  homeApproximated,
  places,
  clusters,
  selectedId,
  viewport,
  water,
  waterTint,
  onSelectPlace,
  onSelectCluster,
}: AroundThisHomeMapProps) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const mapRef = useRef<MapLibreMap | null>(null);
  const markersRef = useRef<Marker[]>([]);
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
    mapRef.current = map;

    return () => {
      for (const marker of markersRef.current) marker.remove();
      markersRef.current = [];
      map.remove();
      mapRef.current = null;
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
        {waterTint && water && (
          <span>{water.groundwater_class} groundwater context</span>
        )}
      </div>
    </div>
  );
}
