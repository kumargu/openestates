import {
  Component,
  createRef,
  lazy,
  Suspense,
  useEffect,
  useMemo,
  useState,
  type ErrorInfo,
  type ReactNode,
} from "react";
import type { PropertyMapContext } from "../../lib/types.ts";
import {
  buildNumberedPlaces,
  metroStationsAroundHome,
  resolveHomeAnchor,
} from "../../lib/nearbyPlateProjection.ts";
import {
  arrivalEvidenceViewport,
  metroLinesNearArrival,
  type ArrivalCameraMode,
} from "../../lib/arrivalMapProjection.ts";

const ArrivalGoogle3DMap = lazy(async () => {
  const module = await import("./PropertyArrivalGoogle3DMap.tsx");
  return { default: module.PropertyArrivalGoogle3DMap };
});

type ArrivalView = "society" | "metro" | "approach";

type Props = {
  context: PropertyMapContext;
};

const SOCIETY_VIEW_RADIUS_KM = 0.8;
const DEFAULT_APPROACH_DWELL_MS = 3_600;

class ArrivalMapBoundary extends Component<
  { children: ReactNode },
  { failed: boolean }
> {
  state = { failed: false };
  fallbackRef = createRef<HTMLDivElement>();

  static getDerivedStateFromError() {
    return { failed: true };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("[PropertyArrivalMap] Map unavailable", error, info);
  }

  componentDidUpdate(
    _previousProps: Readonly<{ children: ReactNode }>,
    previousState: Readonly<{ failed: boolean }>,
  ) {
    if (this.state.failed && !previousState.failed) this.fallbackRef.current?.focus();
  }

  render() {
    if (this.state.failed) {
      return (
        <div
          ref={this.fallbackRef}
          className="property-arrival-map__unavailable"
          role="status"
          tabIndex={-1}
        >
          Map unavailable
        </div>
      );
    }
    return this.props.children;
  }
}

export function PropertyArrivalMap({ context }: Props) {
  const home = useMemo(() => resolveHomeAnchor(context), [context]);
  const roadLayer = context.layers?.find((layer) => layer.renderKind === "terrain_corridor");
  const metroLayer = context.layers?.find((layer) => layer.id === "metro");
  const roadLines = useMemo(
    () => roadLayer
      ? context.layer_lines?.[roadLayer.id] ?? context.access_lines ?? []
      : [],
    [context.access_lines, context.layer_lines, roadLayer],
  );
  const roadExperience = roadLayer?.experience?.kind === "street_view_tour"
    ? roadLayer.experience
    : undefined;
  const metroPlaces = useMemo(() => {
    if (!home) return [];
    return buildNumberedPlaces(metroStationsAroundHome(
      context.places.filter((place) => place.layer === (metroLayer?.id ?? "metro")),
      home,
      context.metro_lines ?? [],
    ));
  }, [context.metro_lines, context.places, home, metroLayer?.id]);
  const metroLines = useMemo(
    () => home
      ? metroLinesNearArrival(home, metroPlaces, context.metro_lines ?? [])
      : [],
    [context.metro_lines, home, metroPlaces],
  );
  const views = useMemo(() => [
    { id: "society" as const, label: "Society", available: Boolean(context.home.boundary) },
    { id: "metro" as const, label: metroLayer?.label ?? "Metro", available: metroLines.length > 0 },
    {
      id: "approach" as const,
      label: roadLayer?.label ?? "Approach road",
      available: roadLines.length > 0 && Boolean(roadExperience),
    },
  ].filter((view) => view.available), [
    context.home.boundary,
    metroLayer?.label,
    metroLines.length,
    roadExperience,
    roadLayer?.label,
    roadLines.length,
  ]);
  const [view, setView] = useState<ArrivalView>(() => views[0]?.id ?? "society");
  const [cameraMode, setCameraMode] = useState<ArrivalCameraMode>(() =>
    views[0]?.id === "metro" ? "evidence" : "home");
  const [expanded, setExpanded] = useState(false);
  const activeView = views.some((candidate) => candidate.id === view)
    ? view
    : views[0]?.id ?? "society";
  const activeCameraMode = activeView === view
    ? cameraMode
    : activeView === "metro"
    ? "evidence"
    : "home";

  useEffect(() => {
    if (activeView !== "approach" || activeCameraMode !== "home") return undefined;
    const timeout = window.setTimeout(
      () => setCameraMode("evidence"),
      roadExperience?.dwellMs ?? DEFAULT_APPROACH_DWELL_MS,
    );
    return () => window.clearTimeout(timeout);
  }, [activeCameraMode, activeView, roadExperience?.dwellMs]);

  useEffect(() => {
    if (!expanded) return undefined;
    const previousOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    return () => {
      document.body.style.overflow = previousOverflow;
    };
  }, [expanded]);

  if (!home || views.length === 0) return null;

  const visiblePlaces = activeView === "metro" ? metroPlaces : [];
  const visibleMetroLines = activeView === "metro" ? metroLines : [];
  const visibleRoadLines = activeView === "approach" ? roadLines : [];
  const viewport = activeView === "metro"
    ? arrivalEvidenceViewport(home, visiblePlaces, visibleMetroLines)
    : {
      center: home,
      radiusKm: SOCIETY_VIEW_RADIUS_KM,
      zoom: 14.6,
      paddingFactor: 0.2,
    };

  function selectView(next: ArrivalView) {
    setView(next);
    setCameraMode(next === "metro" ? "evidence" : "home");
  }

  return (
    <div className="property-arrival-map">
      <div className="property-arrival-map__switcher" role="toolbar" aria-label="Arrival view">
        {views.map((candidate) => (
          <button
            key={candidate.id}
            type="button"
            className={candidate.id === activeView ? "is-active" : undefined}
            aria-pressed={candidate.id === activeView}
            onClick={() => selectView(candidate.id)}
          >
            {candidate.label}
          </button>
        ))}
      </div>
      <ArrivalMapBoundary>
        <Suspense
          fallback={(
            <div
              className="property-arrival-map__loading"
              aria-label="Loading 3D map"
              aria-busy="true"
            />
          )}
        >
          <ArrivalGoogle3DMap
            home={{
              latitude: home.latitude,
              longitude: home.longitude,
              name: context.home.name,
              boundary: context.home.boundary,
            }}
            places={visiblePlaces}
            clusters={[]}
            selectedId={null}
            viewport={viewport}
            metroLines={visibleMetroLines}
            accessLines={visibleRoadLines}
            redFlagLines={[]}
            showMetroLines={activeView === "metro"}
            water={null}
            waterTint={false}
            expanded={expanded}
            cameraMode={activeCameraMode}
            terrainCorridor={activeView === "approach"}
            layerExperience={activeView === "approach" ? roadExperience : undefined}
            onSelectCluster={() => undefined}
            onSelectPlace={() => undefined}
            onSelectAccessLine={() => undefined}
            onSelectRedFlagLine={() => undefined}
            onBackToHome={() => selectView("society")}
            showBackToHome={false}
            onToggleExpanded={() => setExpanded((current) => !current)}
          />
        </Suspense>
      </ArrivalMapBoundary>
    </div>
  );
}
