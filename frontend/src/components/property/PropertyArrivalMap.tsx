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
import type { ArrivalSearchSociety, PropertyMapContext } from "../../lib/types.ts";
import { useArrivalPlaybackController } from "../../lib/arrivalPlayback.ts";
import {
  buildNumberedPlaces,
  metroStationsAroundHome,
  resolveHomeAnchor,
} from "../../lib/nearbyPlateProjection.ts";
import {
  arrivalEvidenceViewport,
  arrivalMarkerPlaces,
  metroLinesNearArrival,
  type ArrivalCameraMode,
} from "../../lib/arrivalMapProjection.ts";

const SocietyScene = lazy(async () => {
  const module = await import("./PropertyArrivalSocietyScene.tsx");
  return { default: module.PropertyArrivalSocietyScene };
});

const ApproachScene = lazy(async () => {
  const module = await import("./PropertyArrivalApproachScene.tsx");
  return { default: module.PropertyArrivalApproachScene };
});

type ArrivalView = "society" | "metro" | "approach";

type Props = {
  context: PropertyMapContext;
  searchContextSocieties?: ArrivalSearchSociety[];
};

const SOCIETY_VIEW_RADIUS_KM = 0.8;
const DEFAULT_APPROACH_DWELL_MS = 3_600;

function compactPrice(price: number): string | null {
  if (!Number.isFinite(price) || price <= 0) return null;
  if (price >= 10_000_000) return `₹${(price / 10_000_000).toFixed(1).replace(/\.0$/, "")} Cr`;
  if (price >= 100_000) return `₹${(price / 100_000).toFixed(1).replace(/\.0$/, "")} L`;
  return `₹${Math.round(price).toLocaleString("en-IN")}`;
}

class ArrivalMapBoundary extends Component<
  { children: ReactNode; unavailableLabel?: string },
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
          {this.props.unavailableLabel}
        </div>
      );
    }
    return this.props.children;
  }
}

export function PropertyArrivalMap({ context, searchContextSocieties = [] }: Props) {
  const { controller: playbackController, state: playbackState } = useArrivalPlaybackController();
  const [societyAutoPlay, setSocietyAutoPlay] = useState(true);
  const [approachAutoPlay, setApproachAutoPlay] = useState(true);
  const [selectedSearchSocietyId, setSelectedSearchSocietyId] = useState<string | null>(null);
  const home = useMemo(() => resolveHomeAnchor(context), [context]);
  const roadLayer = context.layers?.find((layer) => layer.renderKind === "terrain_corridor");
  const entranceLayer = context.layers?.find((layer) => layer.renderKind === "arrival_marker");
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
  const entrancePlaces = useMemo(
    () => arrivalMarkerPlaces(context, entranceLayer),
    [context, entranceLayer],
  );
  const views = useMemo(() => [
    { id: "society" as const, label: "Society", available: true },
    { id: "metro" as const, label: metroLayer?.label ?? "Metro", available: metroLines.length > 0 },
    {
      id: "approach" as const,
      label: roadLayer?.label ?? "Approach road",
      available: roadLines.length > 0 && Boolean(roadExperience),
    },
  ].filter((view) => view.available), [
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
  const selectedSearchSociety = searchContextSocieties.find((candidate) =>
    candidate.societyId === selectedSearchSocietyId) ?? null;
  const arrivalExperience = context.arrivalExperience;
  const missingArrivalState = activeView === "society" && !context.home.boundary
    ? arrivalExperience?.missingBoundaryState
    : (activeView === "society" || activeView === "approach") && entrancePlaces.length === 0
    ? entranceLayer?.emptyState
    : activeView === "society" && roadLayer && roadLines.length === 0
    ? roadLayer.emptyState
    : null;

  useEffect(() => {
    if (activeView !== "approach" || activeCameraMode !== "home") return undefined;
    const run = playbackController.begin("playing");
    if (!run.activate()) return undefined;
    void run.wait(
      roadExperience?.overviewDwellMs
        ?? roadExperience?.dwellMs
        ?? DEFAULT_APPROACH_DWELL_MS,
    ).then((completed) => {
      if (completed && run.isCurrent()) setCameraMode("evidence");
    });
    return () => {
      if (run.isCurrent()) playbackController.cancel("settled");
    };
  }, [
    activeCameraMode,
    activeView,
    roadExperience?.dwellMs,
    roadExperience?.overviewDwellMs,
    playbackController,
  ]);

  useEffect(() => {
    if (!expanded) return undefined;
    const previousOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    return () => {
      document.body.style.overflow = previousOverflow;
    };
  }, [expanded]);

  if (!home || views.length === 0) return null;

  const visiblePlaces = activeView === "metro" ? metroPlaces : entrancePlaces;
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
    playbackController.cancel("settled");
    if (activeView === "society") setSocietyAutoPlay(false);
    if (activeView === "approach") setApproachAutoPlay(false);
    setView(next);
    setCameraMode(next === "metro" ? "evidence" : "home");
  }

  function selectSearchSociety(societyId: string) {
    playbackController.cancel("settled");
    setSocietyAutoPlay(false);
    setApproachAutoPlay(false);
    setView("society");
    setCameraMode("home");
    setSelectedSearchSocietyId(societyId);
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
      {missingArrivalState && (
        <p className="property-arrival-map__status">{missingArrivalState}</p>
      )}
      {activeView === "society"
        && (playbackState === "paused" || (!societyAutoPlay && playbackState === "settled"))
        && arrivalExperience?.societyPlayLabel && (
        <button
          type="button"
          className="property-arrival-map__society-play"
          onClick={() => playbackState === "paused"
            ? playbackController.resume()
            : setSocietyAutoPlay(true)}
        >
          {arrivalExperience.societyPlayLabel}
        </button>
      )}
      <ArrivalMapBoundary unavailableLabel={arrivalExperience?.googleUnavailableState}>
        <Suspense
          fallback={(
            <div
              className="property-arrival-map__loading"
              aria-label="Loading 3D map"
              aria-busy="true"
            />
          )}
        >
          {activeView === "approach" && roadExperience ? (
            <ApproachScene
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
              showMetroLines={false}
              water={null}
              waterTint={false}
              expanded={expanded}
              cameraMode={activeCameraMode}
              experience={roadExperience}
              arrivalExperience={context.arrivalExperience}
              playbackController={playbackController}
              autoPlaySociety={false}
              autoPlayApproach={approachAutoPlay}
              secondarySocieties={searchContextSocieties}
              selectedSecondarySocietyId={selectedSearchSocietyId}
              onSelectSecondarySociety={selectSearchSociety}
              onSocietyPlaybackCancelled={() => setSocietyAutoPlay(false)}
              onSelectCluster={() => undefined}
              onSelectPlace={() => undefined}
              onSelectAccessLine={() => undefined}
              onSelectRedFlagLine={() => undefined}
              onBackToHome={() => selectView("society")}
              showBackToHome={false}
              onToggleExpanded={() => setExpanded((current) => !current)}
            />
          ) : (
            <SocietyScene
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
            accessLines={[]}
            redFlagLines={[]}
            showMetroLines={activeView === "metro"}
            water={null}
            waterTint={false}
            expanded={expanded}
            cameraMode={activeCameraMode}
            arrivalExperience={context.arrivalExperience}
            playbackController={playbackController}
            autoPlaySociety={activeView === "society" && societyAutoPlay}
            secondarySocieties={searchContextSocieties}
            selectedSecondarySocietyId={selectedSearchSocietyId}
            onSelectSecondarySociety={selectSearchSociety}
            onSocietyPlaybackCancelled={() => setSocietyAutoPlay(false)}
            onSelectCluster={() => undefined}
            onSelectPlace={() => undefined}
            onSelectAccessLine={() => undefined}
            onSelectRedFlagLine={() => undefined}
            onBackToHome={() => selectView("society")}
            showBackToHome={false}
            onToggleExpanded={() => setExpanded((current) => !current)}
            />
          )}
        </Suspense>
      </ArrivalMapBoundary>
      {searchContextSocieties.length > 0 && arrivalExperience?.searchContextLabel && (
        <aside className="property-arrival-map__search-context" aria-label={arrivalExperience.searchContextLabel}>
          <span>{arrivalExperience.searchContextLabel}</span>
          <div>
            {searchContextSocieties.map((candidate) => (
              <button
                key={candidate.societyId}
                type="button"
                aria-pressed={candidate.societyId === selectedSearchSocietyId}
                onClick={() => selectSearchSociety(candidate.societyId)}
              >
                {candidate.home.name}
              </button>
            ))}
          </div>
          {selectedSearchSociety && (
            <div className="property-arrival-map__search-preview">
              <strong>{selectedSearchSociety.preview.title}</strong>
              <span>
                {[
                  Number.isFinite(selectedSearchSociety.preview.bhk)
                    ? `${selectedSearchSociety.preview.bhk} BHK`
                    : null,
                  selectedSearchSociety.preview.area,
                  compactPrice(selectedSearchSociety.preview.price),
                ].filter(Boolean).join(" · ")}
              </span>
              {arrivalExperience.backToSocietyLabel && (
                <button
                  type="button"
                  onClick={() => setSelectedSearchSocietyId(null)}
                >
                  {arrivalExperience.backToSocietyLabel}
                </button>
              )}
            </div>
          )}
        </aside>
      )}
    </div>
  );
}
