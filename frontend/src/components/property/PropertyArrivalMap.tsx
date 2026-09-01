import {
  Component,
  createRef,
  lazy,
  Suspense,
  useCallback,
  useEffect,
  useMemo,
  useState,
  type ErrorInfo,
  type ReactNode,
} from "react";
import { Link } from "react-router-dom";
import type {
  ArrivalSearchSociety,
  MapOverlayLine,
  PropertyMapContext,
} from "../../lib/types.ts";
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
import {
  arrivalMissingState,
  arrivalSearchSocietiesForView,
  arrivalViewOptions,
  societyPlaybackAction,
  type ArrivalView,
} from "../../lib/arrivalViewState.ts";

const GoogleArrivalMap = lazy(async () => {
  const module = await import("./PropertyArrivalGoogle3DMap.tsx");
  return { default: module.PropertyArrivalGoogle3DMap };
});

type Props = {
  context: PropertyMapContext;
  searchContextSocieties?: ArrivalSearchSociety[];
  onUnavailable?: () => void;
};

const SOCIETY_VIEW_RADIUS_KM = 0.8;
const DEFAULT_APPROACH_DWELL_MS = 3_600;
const EMPTY_ARRIVAL_LINES: MapOverlayLine[] = [];

function compactPrice(price: number): string | null {
  if (!Number.isFinite(price) || price <= 0) return null;
  if (price >= 10_000_000) return `₹${(price / 10_000_000).toFixed(1).replace(/\.0$/, "")} Cr`;
  if (price >= 100_000) return `₹${(price / 100_000).toFixed(1).replace(/\.0$/, "")} L`;
  return `₹${Math.round(price).toLocaleString("en-IN")}`;
}

class ArrivalMapBoundary extends Component<
  { children: ReactNode; onUnavailable?: () => void; unavailableLabel?: string },
  { failed: boolean }
> {
  state = { failed: false };
  fallbackRef = createRef<HTMLDivElement>();

  static getDerivedStateFromError() {
    return { failed: true };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("[PropertyArrivalMap] Map unavailable", error, info);
    this.props.onUnavailable?.();
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

export function PropertyArrivalMap({
  context,
  searchContextSocieties = [],
  onUnavailable,
}: Props) {
  const { controller: playbackController, state: playbackState } = useArrivalPlaybackController();
  const [societyAutoPlay, setSocietyAutoPlay] = useState(true);
  const [societyPlaybackVersion, setSocietyPlaybackVersion] = useState(0);
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
  const approachOverviewDwellMs = roadExperience?.overviewDwellMs
    ?? roadExperience?.dwellMs
    ?? DEFAULT_APPROACH_DWELL_MS;
  const hasRoadExperience = Boolean(roadExperience);
  const hasApproachLayer = Boolean(roadLayer);
  const approachLabel = roadLayer?.label;
  const metroLabel = metroLayer?.label;
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
  const views = useMemo(() => arrivalViewOptions({
    approachLabel,
    hasApproachLayer,
    hasMetroEvidence: metroLines.length > 0,
    metroLabel,
  }), [
    approachLabel,
    hasApproachLayer,
    metroLabel,
    metroLines.length,
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
  const missingArrivalState = arrivalMissingState(activeView, {
    hasApproachRoad: roadLines.length > 0,
    hasBoundary: Boolean(context.home.boundary),
    hasEntrance: entrancePlaces.length > 0,
    missingApproachRoadState: roadLayer?.emptyState,
    missingBoundaryState: arrivalExperience?.missingBoundaryState,
    missingEntranceState: entranceLayer?.emptyState,
  });
  const visibleSearchContextSocieties = arrivalSearchSocietiesForView(
    activeView,
    searchContextSocieties,
  );
  const societyAction = activeView === "society"
    ? societyPlaybackAction(playbackState)
    : null;
  const societyActionLabel = societyAction === "pause"
    ? arrivalExperience?.societyPauseLabel
    : societyAction === "resume"
    ? arrivalExperience?.societyResumeLabel
    : societyAction === "play"
    ? arrivalExperience?.societyPlayLabel
    : null;
  const societyActionText = societyAction === "pause"
    ? "Pause"
    : societyAction === "resume"
    ? "Resume"
    : societyAction === "play"
    ? "Replay"
    : null;
  const approachReplayAvailable = activeView === "approach"
    && activeCameraMode === "home"
    && !approachAutoPlay;
  const navigationAction = societyAction ?? (approachReplayAvailable ? "play" : null);
  const navigationActionLabel = approachReplayAvailable
    ? roadExperience?.replayLabel
    : societyActionLabel;
  const navigationActionText = approachReplayAvailable ? "Replay" : societyActionText;
  const cancelSocietyPlayback = useCallback(() => setSocietyAutoPlay(false), []);
  const cancelApproachPlayback = useCallback(() => setApproachAutoPlay(false), []);

  useEffect(() => {
    if (
      activeView !== "approach"
      || activeCameraMode !== "home"
      || !approachAutoPlay
      || roadLines.length === 0
      || !hasRoadExperience
    ) return undefined;
    const run = playbackController.begin("playing");
    if (!run.activate()) return undefined;
    void run.wait(approachOverviewDwellMs).then((completed) => {
      if (completed && run.isCurrent()) setCameraMode("evidence");
    });
    return () => {
      if (run.isCurrent()) playbackController.cancel("settled");
    };
  }, [
    activeCameraMode,
    activeView,
    approachAutoPlay,
    approachOverviewDwellMs,
    hasRoadExperience,
    roadLines.length,
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

  useEffect(() => {
    const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)");
    const stopForReducedMotion = (event: MediaQueryListEvent) => {
      if (!event.matches) return;
      playbackController.cancel("settled");
      setSocietyAutoPlay(false);
      setApproachAutoPlay(false);
      if (activeView === "approach") setCameraMode("evidence");
    };
    reducedMotion.addEventListener("change", stopForReducedMotion);
    return () => reducedMotion.removeEventListener("change", stopForReducedMotion);
  }, [activeView, playbackController]);

  const selectView = useCallback((next: ArrivalView) => {
    playbackController.cancel("settled");
    if (activeView === "society") setSocietyAutoPlay(false);
    if (activeView === "approach") setApproachAutoPlay(false);
    setView(next);
    const reducedApproach = next === "approach"
      && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    setCameraMode(next === "metro" || reducedApproach ? "evidence" : "home");
  }, [activeView, playbackController]);

  const selectSearchSociety = useCallback((societyId: string) => {
    playbackController.cancel("settled");
    setSocietyAutoPlay(false);
    setApproachAutoPlay(false);
    setView("society");
    setCameraMode("home");
    setSelectedSearchSocietyId(societyId);
  }, [playbackController]);

  if (!home || views.length === 0) return null;

  const visiblePlaces = activeView === "metro" ? metroPlaces : entrancePlaces;
  const visibleMetroLines = activeView === "metro" ? metroLines : EMPTY_ARRIVAL_LINES;
  const visibleRoadLines = activeView === "approach" ? roadLines : EMPTY_ARRIVAL_LINES;
  const viewport = activeView === "metro"
    ? arrivalEvidenceViewport(home, visiblePlaces, visibleMetroLines)
    : {
      center: home,
      radiusKm: SOCIETY_VIEW_RADIUS_KM,
      zoom: 14.6,
      paddingFactor: 0.2,
    };

  return (
    <div className="property-arrival-map">
      <div className="property-arrival-map__nav">
        <div className="property-arrival-map__switcher" role="group" aria-label="Arrival view">
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
        {navigationAction && navigationActionText ? (
          <button
            type="button"
            className="property-arrival-map__playback"
            aria-label={navigationActionLabel ?? navigationActionText}
            onClick={() => {
              if (navigationAction === "pause") playbackController.pause();
              else if (navigationAction === "resume") playbackController.resume();
              else if (approachReplayAvailable) setApproachAutoPlay(true);
              else {
                setSocietyAutoPlay(true);
                setSocietyPlaybackVersion((current) => current + 1);
              }
            }}
          >
            <span aria-hidden="true">{navigationAction === "pause" ? "Ⅱ" : navigationAction === "resume" ? "▶" : "↻"}</span>
            {navigationActionText}
          </button>
        ) : null}
      </div>
      {missingArrivalState && (
        <p className="property-arrival-map__status" role="status" aria-live="polite">
          {missingArrivalState}
        </p>
      )}
      <ArrivalMapBoundary
        unavailableLabel={arrivalExperience?.googleUnavailableState}
        onUnavailable={onUnavailable}
      >
        <Suspense
          fallback={(
            <div
              className="property-arrival-map__loading"
              aria-label="Loading 3D map"
              aria-busy="true"
            />
          )}
        >
          <GoogleArrivalMap
            key={activeView === "approach" ? "approach" : "society"}
            home={{
              latitude: home.latitude,
              longitude: home.longitude,
              name: context.home.name,
              boundary: context.home.boundary,
            }}
            places={visiblePlaces}
            viewport={viewport}
            metroLines={visibleMetroLines}
            accessLines={visibleRoadLines}
            showMetroLines={activeView === "metro"}
            expanded={expanded}
            cameraMode={activeCameraMode}
            terrainCorridor={activeView === "approach"}
            layerExperience={activeView === "approach" ? roadExperience : undefined}
            arrivalExperience={context.arrivalExperience}
            playbackController={playbackController}
            autoPlaySociety={activeView === "society" && societyAutoPlay}
            societyPlaybackVersion={societyPlaybackVersion}
            autoPlayApproach={approachAutoPlay}
            secondarySocieties={visibleSearchContextSocieties}
            selectedSecondarySocietyId={activeView === "society" ? selectedSearchSocietyId : null}
            onSelectSecondarySociety={activeView === "society" ? selectSearchSociety : undefined}
            onPlaybackCancelled={activeView === "approach"
              ? cancelApproachPlayback
              : cancelSocietyPlayback}
            onToggleExpanded={() => setExpanded((current) => !current)}
          />
        </Suspense>
      </ArrivalMapBoundary>
      {activeView === "society"
        && searchContextSocieties.length > 0
        && arrivalExperience?.searchContextLabel ? (
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
              {arrivalExperience.searchContextViewHomeLabel ? (
                <Link to={selectedSearchSociety.href}>
                  {arrivalExperience.searchContextViewHomeLabel}
                </Link>
              ) : null}
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
      ) : null}
    </div>
  );
}
