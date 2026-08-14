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
import type { MapOverlayLine, PropertyMapContext } from "../../lib/types.ts";
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
import { labelsForNearbyPlace, labelsForRedFlagLine } from "../../lib/notebook.ts";
import {
  MapEvidenceTray,
  type MapEvidenceSelection,
} from "./MapEvidenceTray.tsx";
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

function redFlagLineTitle(line: MapOverlayLine): string {
  if (line.label?.trim()) return line.label.trim();
  const normalized = line.name.toLowerCase();
  if (normalized.includes("transmission") || normalized.includes("voltage")) {
    return "Transmission line";
  }
  if (normalized.includes("drain") || normalized.includes("stormwater")) {
    return "Stormwater drain";
  }
  return "Red flag";
}

function redFlagSelection(
  propertyId: string,
  line: MapOverlayLine,
): MapEvidenceSelection {
  const title = redFlagLineTitle(line);
  const distance = typeof line.distance_km === "number"
    ? line.distance_km < 1
      ? `${Math.round(line.distance_km * 1000)} m away`
      : `${line.distance_km.toFixed(1)} km away`
    : null;
  const operator = line.name.trim() && line.name.trim() !== title
    ? line.name.trim()
    : null;
  return {
    id: `line:${line.id}`,
    catalogKey: `nearby-line:${propertyId}:red_flags:${line.id}`,
    title,
    layerLabel: "Red flag",
    meta: [
      ...(line.details ?? []),
      distance,
      operator,
    ].filter((value): value is string => Boolean(value)),
    sourceType: line.source_type,
    sourceUrl: line.source_url,
    labels: labelsForRedFlagLine(title),
  };
}

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
  const [selectedId, setSelectedId] = useState<string | null>(() => null);
  const [selectedLineId, setSelectedLineId] = useState<string | null>(() =>
    focus?.featureId && context.red_flag_lines?.some((line) => line.id === focus.featureId)
      ? focus.featureId
      : null);
  const [openedClusterId, setOpenedClusterId] = useState<string | null>(null);
  const [expanded, setExpanded] = useState(false);
  const [selectionDismissed, setSelectionDismissed] = useState(false);

  useEffect(() => {
    if (!expanded) return undefined;
    const previousOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    return () => {
      document.body.style.overflow = previousOverflow;
    };
  }, [expanded]);

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
  const showMetroLines = metroFocused && activeMetroLines.length > 0;

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
      const radiusKm = context.water?.scope_radius_km ?? DEFAULT_WATER_SCOPE_RADIUS_KM;
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
      showMetroLines ? activeMetroLines : [],
      "nearest",
      activeRedFlagLines,
      focus,
    );
  }, [activeMetroLines, activeRedFlagLines, context.water?.scope_radius_km, focus, home, numbered, scale, showMetroLines, waterFocused]);

  const selected = numbered.find((place) => place.id === selectedId) ?? focusedPlace ?? null;
  const selectedLine = activeRedFlagLines.find((line) => line.id === selectedLineId)
    ?? (selected ? null : activeRedFlagLines[0])
    ?? null;

  const mapSelection = useMemo<MapEvidenceSelection | null>(() => {
    if (selectionDismissed) return null;
    if (waterFocused && context.water) {
      const radiusKm = context.water.scope_radius_km ?? DEFAULT_WATER_SCOPE_RADIUS_KM;
      return {
        id: `water:${context.water.groundwater_class}`,
        catalogKey: `water:${propertyId}:water:${context.water.groundwater_class}`,
        title: `${context.water.groundwater_class} groundwater potential`,
        layerLabel: "Water",
        meta: [`Around ${radiusKm.toFixed(0)} km`],
        summary: context.water.summary,
        sourceType: context.water.source_type,
        sourceUrl: context.water.source_url,
        labels: ["water"],
      };
    }
    if (selectedLine) {
      return redFlagSelection(propertyId, selectedLine);
    }
    if (!selected) return null;
    return {
      id: `place:${selected.id}`,
      catalogKey: `nearby:${propertyId}:${selected.id}`,
      title: selected.name,
      layerLabel: layerLabel(selected.layer, context),
      meta: [
        typeof selected.distance_km === "number" ? `${selected.distance_km.toFixed(1)} km` : null,
        selected.layer !== "red_flags" && typeof selected.rating === "number"
          ? `${selected.rating.toFixed(1)} rating`
          : null,
        selected.layer !== "red_flags" && typeof selected.review_count === "number"
          ? `${selected.review_count} reviews`
          : null,
        ...(selected.lines ?? []),
      ].filter((value): value is string => Boolean(value)),
      summary: selected.note,
      sourceType: selected.source_type,
      sourceUrl: selected.source_url,
      labels: labelsForNearbyPlace(selected.layer, selected.distance_km),
    };
  }, [context, propertyId, selected, selectedLine, selectionDismissed, waterFocused]);

  function selectStory(next: PlateStory) {
    setStory(next);
    setSelectedId(null);
    setSelectedLineId(null);
    setOpenedClusterId(null);
    setSelectionDismissed(false);
    setScale(scaleForStory(next, focus, context.places));
  }

  function selectPlace(id: string) {
    setSelectedId(id);
    setSelectedLineId(null);
    setSelectionDismissed(false);
  }

  function selectCluster(cluster: PlaceCluster) {
    setOpenedClusterId(cluster.id);
    const first = cluster.placeIds[0];
    if (first) setSelectedId(first);
    setSelectedLineId(null);
    setSelectionDismissed(false);
  }

  function selectRedFlagLine(id: string) {
    setSelectedLineId(id);
    setSelectedId(null);
    setSelectionDismissed(false);
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
    <section
      className={`nearby-plate${expanded ? " is-expanded" : ""}`}
      aria-labelledby="around-this-home-title"
    >
      <div className="nearby-plate__head">
        <div>
          <h2 id="around-this-home-title" className="nearby-plate__title">Around this home</h2>
          {focus ? (
            <p className="nearby-plate__match-context">Matched your search</p>
          ) : null}
        </div>
      </div>

      <div className="nearby-plate__layers" role="toolbar" aria-label="Map layers">
        {layers.map((layer) => {
          const on = story.kind === "layer" && story.layer === layer;
          const label = layerLabel(layer, context);
          return (
            <button
              key={layer}
              type="button"
              className={`nearby-plate__chip nearby-plate__chip--${layer}${on ? " is-active" : ""}`}
              aria-pressed={on}
              onClick={() => selectStory({ kind: "layer", layer })}
            >
              <SoftNearbyIcon kind={layer} />
              <span>{label}</span>
            </button>
          );
        })}
        {context.water && (
          <button
            type="button"
            className={`nearby-plate__chip nearby-plate__chip--water${waterFocused ? " is-active" : ""}`}
            aria-pressed={waterFocused}
            onClick={() => selectStory({ kind: "water" })}
          >
            <SoftNearbyIcon kind="water" />
            <span>Water</span>
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
                water={context.water}
                waterTint={showWater}
                expanded={expanded}
                pinnedPlaceIds={pinnedPlaceIds}
                onSelectPlace={selectPlace}
                onSelectCluster={selectCluster}
                onSelectRedFlagLine={selectRedFlagLine}
                onRememberPlace={rememberPlace}
                onToggleExpanded={() => setExpanded((current) => !current)}
              />
            </NearbyMapBoundary>
          ) : (
            <div className="nearby-plate__empty-map">
              <p>Map unavailable</p>
            </div>
          )}

          {mapSelection && (
            <MapEvidenceTray
              key={mapSelection.id}
              propertyId={propertyId}
              selection={mapSelection}
              onClose={() => {
                setSelectedId(null);
                setSelectedLineId(null);
                setSelectionDismissed(true);
              }}
            />
          )}
        </div>
      </div>
    </section>
  );
}
