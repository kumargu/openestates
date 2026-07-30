import {
  Component,
  createRef,
  useEffect,
  useMemo,
  useState,
  type ComponentType,
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
  placeMatchesProofFocus,
  placesForStory,
  resolveHomeAnchor,
  scaleForStory,
  zoomForRadiusKm,
  type PlateScaleMode,
  type PlateStory,
  type PlaceCluster,
} from "../../lib/nearbyPlateProjection.ts";
import { SoftNearbyIcon } from "../ui/SoftIcons.tsx";
import type { AroundThisHomeMapProps } from "./AroundThisHomeMap.tsx";
import { NotebookPinButton } from "../notebook/NotebookPinButton.tsx";
import { labelsForNearbyPlace, labelsForRedFlagLine } from "../../lib/notebook.ts";
import { useNotebook } from "../../hooks/useNotebook.ts";

const loadAroundThisHomeMap = async () => {
  const module = await import("./AroundThisHomeMap.tsx");
  return { default: module.AroundThisHomeMap };
};

function RetryableAroundThisHomeMap({
  ...props
}: AroundThisHomeMapProps) {
  const [MapComponent, setMapComponent] =
    useState<ComponentType<AroundThisHomeMapProps> | null>(null);
  const [loadError, setLoadError] = useState<unknown>(null);

  useEffect(() => {
    let cancelled = false;
    loadAroundThisHomeMap()
      .then((module) => {
        if (!cancelled) setMapComponent(() => module.default);
      })
      .catch((error: unknown) => {
        if (!cancelled) setLoadError(error);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  if (loadError) throw loadError;
  if (!MapComponent) {
    return (
      <div className="nearby-plate__empty-map">
        <p>Loading neighborhood map…</p>
      </div>
    );
  }
  return <MapComponent {...props} />;
}

class NearbyMapBoundary extends Component<
  { children: ReactNode },
  { failed: boolean; retries: number }
> {
  state = { failed: false, retries: 0 };
  retryButtonRef = createRef<HTMLButtonElement>();

  retry = () => {
    this.setState((state) => ({
      failed: false,
      retries: state.retries + 1,
    }));
  };

  static getDerivedStateFromError() {
    return { failed: true };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("[AroundThisHomeMap] Map unavailable", error, info);
  }

  componentDidUpdate(
    _previousProps: Readonly<{ children: ReactNode }>,
    previousState: Readonly<{ failed: boolean; retries: number }>,
  ) {
    if (
      this.state.failed
      && !previousState.failed
      && this.state.retries > 0
    ) {
      this.retryButtonRef.current?.focus();
    }
  }

  render() {
    if (this.state.failed) {
      return (
        <div className="nearby-plate__empty-map" role="status">
          <p>Map unavailable</p>
          <button
            ref={this.retryButtonRef}
            type="button"
            className="nearby-plate__map-retry"
            onClick={this.retry}
          >
            Retry map
          </button>
        </div>
      );
    }
    return this.props.children;
  }
}

type AroundThisHomePlateProps = {
  propertyId: string;
  context: PropertyMapContext;
};

type AroundThisHomePlateInnerProps = {
  propertyId: string;
  context: PropertyMapContext;
  layers: string[];
};

const DEFAULT_WATER_SCOPE_RADIUS_KM = 3;

function redFlagLineTitle(name: string): string {
  const normalized = name.toLowerCase();
  if (normalized.includes("transmission") || normalized.includes("voltage")) {
    return "High-voltage transmission line";
  }
  if (normalized.includes("drain") || normalized.includes("stormwater")) {
    return "Stormwater drain";
  }
  return "Red flag";
}

type RedFlagLineSummary = {
  id: string;
  title: string;
  sourceType: string;
  count: number;
};

export function AroundThisHomePlate({ propertyId, context }: AroundThisHomePlateProps) {
  const layers = useMemo(() => availableLayers(context), [context]);
  const focus = context.proof_focus;
  const focusKey = focus
    ? [
      focus.surfaceId,
      focus.layerId,
      focus.factKey,
      focus.featureId,
      focus.entityId,
      focus.matchedLabel,
      focus.distanceM,
    ].filter(Boolean).join("|")
    : null;

  return (
    <AroundThisHomePlateInner
      key={`${context.home.name}:${focusKey ?? "default"}`}
      propertyId={propertyId}
      context={context}
      layers={layers}
    />
  );
}

function AroundThisHomePlateInner({
  propertyId,
  context,
  layers,
}: AroundThisHomePlateInnerProps) {
  const { notes, toggleFact } = useNotebook();
  const home = resolveHomeAnchor(context);
  const focus = context.proof_focus;
  const focusedStory = useMemo(
    () => focus && layers.includes(focus.layerId)
      ? { kind: "layer" as const, layer: focus.layerId }
      : null,
    [focus, layers],
  );
  const defaultStory: PlateStory = layers[0]
    ? { kind: "layer", layer: layers[0] }
    : { kind: "water" };
  const [scale, setScale] = useState<PlateScaleMode>(() =>
    scaleForStory(focusedStory ?? defaultStory, focus, context.places));
  const [story, setStory] = useState<PlateStory>(() =>
    focusedStory ?? defaultStory);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [openedClusterId, setOpenedClusterId] = useState<string | null>(null);

  const storyPlaces = useMemo(() => {
    const forStory = placesForStory(context, story);
    const filtered = filterPlacesByScale(forStory, scale, focus);
    if (story.kind === "layer" && story.layer === "metro" && home) {
      return metroStationsAroundHome(filtered, home, context.metro_lines ?? [], focus);
    }
    return filtered;
  }, [context, home, story, scale, focus]);

  const numbered = useMemo(() => buildNumberedPlaces(storyPlaces), [storyPlaces]);
  const focusedPlace = useMemo(
    () => focus
      ? numbered.find((place) => placeMatchesProofFocus(place, focus)) ?? null
      : null,
    [focus, numbered],
  );

  const { singles, clusters } = useMemo(() => {
    if (openedClusterId) {
      return { singles: numbered, clusters: [] as PlaceCluster[] };
    }
    return clusterClosePlaces(numbered, scale);
  }, [numbered, openedClusterId, scale]);

  const metroFocused = story.kind === "layer" && story.layer === "metro";
  const redFlagsFocused = story.kind === "layer" && story.layer === "red_flags";
  const activeRedFlagLines = useMemo(
    () => redFlagsFocused ? context.red_flag_lines ?? [] : [],
    [context.red_flag_lines, redFlagsFocused],
  );
  const waterFocused = story.kind === "water";
  const activeMetroLines = useMemo(
    () => context.metro_lines ?? [],
    [context.metro_lines],
  );
  const showMetroLines = activeMetroLines.length > 0;
  const redFlagLineSummaries = useMemo(() => {
    const summaries = new Map<string, RedFlagLineSummary>();
    for (const line of activeRedFlagLines) {
      const title = redFlagLineTitle(line.name);
      const key = `${title}:${line.source_type}`;
      const current = summaries.get(key);
      if (current) {
        current.count += 1;
      } else {
        summaries.set(key, {
          id: key,
          title,
          sourceType: line.source_type,
          count: 1,
        });
      }
    }
    return [...summaries.values()];
  }, [activeRedFlagLines]);
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
        activeMetroLines,
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
      activeMetroLines,
      "nearest",
      activeRedFlagLines,
      focus,
    );
  }, [activeMetroLines, activeRedFlagLines, context.water?.scope_radius_km, focus, home, numbered, scale, showMetroLines, waterFocused]);

  const selected =
    numbered.find((place) => place.id === selectedId)
    ?? focusedPlace
    ?? numbered[0]
    ?? null;
  function selectStory(next: PlateStory) {
    setStory(next);
    setSelectedId(null);
    setOpenedClusterId(null);
    setScale(scaleForStory(next, focus, context.places));
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
  const pinnedPlaceIds = useMemo(
    () => notes
      .filter((note) => note.propertyId === propertyId && note.catalogKey.startsWith(`nearby:${propertyId}:`))
      .map((note) => note.catalogKey.slice(`nearby:${propertyId}:`.length)),
    [notes, propertyId],
  );

  function rememberPlace(place: (typeof numbered)[number]) {
    toggleFact({
      propertyId,
      catalogKey: `nearby:${propertyId}:${place.id}`,
      title: compactPlaceLabel(place.name),
      labels: labelsForNearbyPlace(place.layer, place.distance_km),
      detail: [
        layerLabel(place.layer, context),
        typeof place.distance_km === "number" ? `${place.distance_km.toFixed(1)} km` : null,
      ].filter(Boolean).join(" · "),
      source: "Around this home",
      kind: "fact",
    });
  }

  return (
    <section className="nearby-plate" aria-label="Around this home">
      <div className="nearby-plate__head">
        <div>
          <h2 className="nearby-plate__title">Around this home</h2>
        </div>
      </div>

      <div className="nearby-plate__layers" role="toolbar" aria-label="Nearby story">
        {layers.map((layer) => {
          const on = story.kind === "layer" && story.layer === layer;
          const label = layerLabel(layer, context);
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
              <RetryableAroundThisHomeMap
                home={{
                  latitude: home.latitude,
                  longitude: home.longitude,
                  name: context.home.name,
                }}
                places={singles}
                clusters={clusters}
                selectedId={selected?.id ?? null}
                viewport={viewport}
                metroLines={activeMetroLines}
                redFlagLines={activeRedFlagLines}
                showMetroLines={showMetroLines}
                nearestMetroDistanceKm={metroFocused ? nearestMetroDistanceKm : undefined}
                water={context.water}
                waterTint={showWater}
                pinnedPlaceIds={pinnedPlaceIds}
                onSelectPlace={selectPlace}
                onSelectCluster={selectCluster}
                onRememberPlace={rememberPlace}
              />
            </NearbyMapBoundary>
          ) : (
            <div className="nearby-plate__empty-map">
              <p>Map unavailable</p>
            </div>
          )}

          {redFlagsFocused && (redFlagLineSummaries.length > 0 || numbered.length > 0) && (
            <ol className="nearby-plate__nearest" aria-label="Nearby places">
              {redFlagLineSummaries.map((summary) => (
                <li key={summary.id} className="nearby-plate__nearest-item">
                  <div className="nearby-plate__nearest-row nearby-plate__nearest-row--static">
                    <span className="nearby-plate__nearest-icon">
                      <SoftNearbyIcon kind="red_flags" size={28} />
                    </span>
                    <span className="nearby-plate__nearest-copy">
                      <span className="nearby-plate__nearest-name">
                        {summary.title}
                      </span>
                      <span className="nearby-plate__nearest-meta">
                        {summary.sourceType}
                        {summary.count > 1 ? ` · ${summary.count} segments` : ""}
                      </span>
                    </span>
                  </div>
                  <NotebookPinButton
                    propertyId={propertyId}
                    catalogKey={`nearby-line:${propertyId}:${summary.id}`}
                    title={summary.title}
                    labels={labelsForRedFlagLine(summary.title)}
                    detail={[
                      summary.sourceType,
                      summary.count > 1 ? `${summary.count} segments` : null,
                    ].filter(Boolean).join(" · ")}
                    source="Around this home"
                    kind="fact"
                  />
                </li>
              ))}
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
                          {layerLabel(place.layer, context)}
                          {typeof place.distance_km === "number"
                            ? ` · ${place.distance_km.toFixed(1)} km`
                            : ""}
                          {place.layer !== "red_flags" && typeof place.rating === "number"
                            ? ` · ${place.rating.toFixed(1)}`
                            : ""}
                          {place.layer !== "red_flags" && typeof place.review_count === "number"
                            ? ` · ${place.review_count} reviews`
                            : ""}
                        </span>
                      </span>
                    </button>
                    <NotebookPinButton
                      propertyId={propertyId}
                      catalogKey={`nearby:${propertyId}:${place.id}`}
                      title={compactPlaceLabel(place.name)}
                      labels={labelsForNearbyPlace(place.layer, place.distance_km)}
                      detail={[
                        layerLabel(place.layer, context),
                        typeof place.distance_km === "number"
                          ? `${place.distance_km.toFixed(1)} km`
                          : null,
                      ].filter(Boolean).join(" · ")}
                      source="Around this home"
                      kind="fact"
                    />
                  </li>
                );
              })}
            </ol>
          )}

          {waterFocused && context.water && (
            <div className="nearby-plate__water-card">
              <div className="nearby-plate__water-card-main">
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
              <NotebookPinButton
                propertyId={propertyId}
                catalogKey={`water:${propertyId}:${context.water.groundwater_class}`}
                title={`${context.water.groundwater_class} groundwater`}
                labels={["water"]}
                detail={`Around ${(context.water.scope_radius_km ?? DEFAULT_WATER_SCOPE_RADIUS_KM).toFixed(0)} km`}
                source="Around this home"
                kind="fact"
              />
            </div>
          )}
        </div>
      </div>
    </section>
  );
}
