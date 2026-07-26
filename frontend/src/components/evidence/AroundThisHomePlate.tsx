import {
  Component,
  lazy,
  Suspense,
  useMemo,
  useState,
  type ErrorInfo,
  type ReactNode,
} from "react";
import type { PropertyMapContext } from "../../lib/types.ts";
import {
  availableLayers,
  buildNumberedPlaces,
  buildPlateViewport,
  clusterClosePlaces,
  compactPlaceLabel,
  filterPlacesByScale,
  layerLabel,
  metroStationsAroundHome,
  placesForStory,
  resolveHomeAnchor,
  zoomForRadiusKm,
  type PlateScaleMode,
  type PlateStory,
  type PlaceCluster,
} from "../../lib/nearbyPlateProjection.ts";
import { SoftNearbyIcon } from "../ui/SoftIcons.tsx";

const AroundThisHomeMap = lazy(async () => {
  const module = await import("./AroundThisHomeMap.tsx");
  return { default: module.AroundThisHomeMap };
});

class NearbyMapBoundary extends Component<
  { children: ReactNode },
  { failed: boolean }
> {
  state = { failed: false };

  static getDerivedStateFromError() {
    return { failed: true };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("[AroundThisHomeMap] Map unavailable", error, info);
  }

  render() {
    if (this.state.failed) {
      return (
        <div className="nearby-plate__empty-map" role="status">
          <p>Map unavailable</p>
        </div>
      );
    }
    return this.props.children;
  }
}

type AroundThisHomePlateProps = {
  context: PropertyMapContext;
};

const DEFAULT_WATER_SCOPE_RADIUS_KM = 3;

export function AroundThisHomePlate({ context }: AroundThisHomePlateProps) {
  const layers = availableLayers(context);
  const home = resolveHomeAnchor(context);
  const [scale, setScale] = useState<PlateScaleMode>("nearby");
  const [story, setStory] = useState<PlateStory>({ kind: "essentials" });
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [openedClusterId, setOpenedClusterId] = useState<string | null>(null);

  const storyPlaces = useMemo(() => {
    const forStory = placesForStory(context, story);
    const filtered = filterPlacesByScale(forStory, scale);
    if (story.kind === "layer" && story.layer === "metro" && home) {
      return metroStationsAroundHome(filtered, home, context.metro_lines ?? []);
    }
    return filtered;
  }, [context, home, story, scale]);

  const numbered = useMemo(() => buildNumberedPlaces(storyPlaces), [storyPlaces]);

  const { singles, clusters } = useMemo(() => {
    if (openedClusterId) {
      return { singles: numbered, clusters: [] as PlaceCluster[] };
    }
    return clusterClosePlaces(numbered, scale);
  }, [numbered, openedClusterId, scale]);

  const showMetroLines = (context.metro_lines?.length ?? 0) > 0;
  const metroFocused = story.kind === "layer" && story.layer === "metro";
  const waterFocused = story.kind === "water";
  const nearestMetroDistanceKm = useMemo(
    () => context.places
      .filter((place) => place.layer === "metro" && typeof place.distance_km === "number")
      .reduce<number | undefined>(
        (nearest, place) => nearest == null
          ? place.distance_km
          : Math.min(nearest, place.distance_km ?? nearest),
        undefined,
      ),
    [context.places],
  );

  const viewport = useMemo(() => {
    if (!home) {
      return {
        center: { latitude: 12.97, longitude: 77.59 },
        radiusKm: 0.8,
        zoom: 14.6,
        paddingFactor: 0.2,
      };
    }
    if (waterFocused) {
      const persistentMetroViewport = buildPlateViewport(
        home,
        [],
        "area",
        context.metro_lines ?? [],
        "nearest",
      );
      const radiusKm = Math.max(
        context.water?.scope_radius_km ?? DEFAULT_WATER_SCOPE_RADIUS_KM,
        showMetroLines ? persistentMetroViewport.radiusKm : 0,
      );
      return {
        center: { latitude: home.latitude, longitude: home.longitude },
        radiusKm,
        zoom: zoomForRadiusKm(radiusKm),
        paddingFactor: 0.24,
      };
    }
    return buildPlateViewport(
      home,
      numbered,
      showMetroLines ? "area" : scale,
      context.metro_lines ?? [],
      "nearest",
    );
  }, [context.metro_lines, context.water?.scope_radius_km, home, numbered, scale, showMetroLines, waterFocused]);

  const selected =
    numbered.find((place) => place.id === selectedId)
    ?? numbered[0]
    ?? null;

  function selectStory(next: PlateStory) {
    setStory(next);
    setSelectedId(null);
    setOpenedClusterId(null);
    if (next.kind === "water") {
      setScale("area");
    } else if (next.kind === "layer" && next.layer === "metro") {
      setScale("area");
    } else {
      setScale("nearby");
    }
  }

  function selectPlace(id: string) {
    setSelectedId(id);
  }

  function selectCluster(cluster: PlaceCluster) {
    setOpenedClusterId(cluster.id);
    const first = cluster.placeIds[0];
    if (first) setSelectedId(first);
  }

  const showWater = Boolean(context.water && waterFocused);
  const canRenderMap = Boolean(home);

  return (
    <section className="nearby-plate" aria-label="Around this home">
      <div className="nearby-plate__head">
        <div>
          <p className="nearby-plate__kicker">Around this home</p>
          <h2 className="nearby-plate__title">{context.home.name}</h2>
          {context.home.area && (
            <p className="nearby-plate__sub">{context.home.area}</p>
          )}
        </div>
      </div>

      <div className="nearby-plate__layers" role="toolbar" aria-label="Nearby story">
        <button
          type="button"
          className={`nearby-plate__chip${story.kind === "essentials" ? " is-active" : ""}`}
          aria-pressed={story.kind === "essentials"}
          onClick={() => selectStory({ kind: "essentials" })}
        >
          <SoftNearbyIcon kind="essentials" />
          Essentials
        </button>
        {layers.map((layer) => {
          const on = story.kind === "layer" && story.layer === layer;
          const label = layerLabel(layer);
          const iconOnly = layer === "metro"
            || layer === "schools"
            || layer === "hospitals";
          return (
            <button
              key={layer}
              type="button"
              className={`nearby-plate__chip nearby-plate__chip--${layer}${iconOnly ? " nearby-plate__chip--icon" : ""}${on ? " is-active" : ""}`}
              aria-label={label}
              aria-pressed={on}
              title={label}
              onClick={() => selectStory({ kind: "layer", layer })}
            >
              <SoftNearbyIcon kind={layer} />
              {!iconOnly && label}
            </button>
          );
        })}
        {context.water && (
          <button
            type="button"
            className={`nearby-plate__chip nearby-plate__chip--water nearby-plate__chip--icon${waterFocused ? " is-active" : ""}`}
            aria-label="Water"
            aria-pressed={waterFocused}
            title="Water"
            onClick={() => selectStory({ kind: "water" })}
          >
            <SoftNearbyIcon kind="water" />
          </button>
        )}
      </div>

      <div className="nearby-plate__body">
        <div className="nearby-plate__canvas">
          {canRenderMap && home ? (
            <NearbyMapBoundary
              key={`${home.latitude.toFixed(5)}-${home.longitude.toFixed(5)}`}
            >
              <Suspense
                fallback={(
                  <div className="nearby-plate__empty-map">
                    <p>Loading neighborhood map…</p>
                  </div>
                )}
              >
                <AroundThisHomeMap
                  home={{
                    latitude: home.latitude,
                    longitude: home.longitude,
                    name: context.home.name,
                  }}
                  places={singles}
                  clusters={clusters}
                  selectedId={selected?.id ?? null}
                  viewport={viewport}
                  metroLines={context.metro_lines ?? []}
                  showMetroLines={showMetroLines}
                  nearestMetroDistanceKm={metroFocused ? nearestMetroDistanceKm : undefined}
                  water={context.water}
                  waterTint={showWater}
                  onSelectPlace={selectPlace}
                  onSelectCluster={selectCluster}
                />
              </Suspense>
            </NearbyMapBoundary>
          ) : (
            <div className="nearby-plate__empty-map">
              <p>Map unavailable</p>
            </div>
          )}

          {!waterFocused && numbered.length > 0 && (
            <ol className="nearby-plate__nearest" aria-label="Nearby places">
              {numbered.map((place) => {
                const isSelected = selected?.id === place.id;
                return (
                  <li key={place.id} className="nearby-plate__nearest-item">
                    <button
                      type="button"
                      className={`nearby-plate__nearest-row${isSelected ? " is-selected" : ""}`}
                      onClick={() => selectPlace(place.id)}
                    >
                      <span className="nearby-plate__nearest-icon">
                        <SoftNearbyIcon kind={place.layer} size={28} />
                      </span>
                      <span className="nearby-plate__nearest-copy">
                        <span className="nearby-plate__nearest-name">
                          {compactPlaceLabel(place.name)}
                        </span>
                        <span className="nearby-plate__nearest-meta">
                          {layerLabel(place.layer)}
                          {typeof place.distance_km === "number"
                            ? ` · ${place.distance_km.toFixed(1)} km`
                            : ""}
                          {typeof place.rating === "number"
                            ? ` · ${place.rating.toFixed(1)}`
                            : ""}
                          {typeof place.review_count === "number"
                            ? ` · ${place.review_count} reviews`
                            : ""}
                        </span>
                      </span>
                    </button>
                  </li>
                );
              })}
            </ol>
          )}

          {waterFocused && context.water && (
            <div className="nearby-plate__water-card">
              <strong>{context.water.groundwater_class} groundwater potential</strong>
              <span>
                Area context for this society, not a borewell or water-supply reading.
              </span>
              <div className="nearby-plate__water-actions">
                <span>
                  Around {(context.water.scope_radius_km ?? DEFAULT_WATER_SCOPE_RADIUS_KM).toFixed(0)} km
                </span>
              </div>
            </div>
          )}
        </div>
      </div>
    </section>
  );
}
