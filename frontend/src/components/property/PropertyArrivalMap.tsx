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
import type { NearbyDepth } from '../../lib/atlasNearbyScene.ts';
import type {
  NearbyTourChapter,
  NearbyTourRequest,
  NearbyTourSceneState,
} from './PropertyArrivalGoogle3DMap.tsx';
import type {
  ArrivalSearchSociety,
  MapOverlayLine,
  MapPlacePin,
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
import "../../styles/property-arrival.css";

const GoogleArrivalMap = lazy(async () => {
  const module = await import("./PropertyArrivalGoogle3DMap.tsx");
  return { default: module.PropertyArrivalGoogle3DMap };
});

type Props = {
  context: PropertyMapContext;
  searchContextSocieties?: ArrivalSearchSociety[];
  onUnavailable?: () => void;
  presentation?: "embedded" | "atlas";
  identity?: {
    location: string;
    title: string;
    facts: string[];
    actions?: ReactNode;
  };
};

const SOCIETY_VIEW_RADIUS_KM = 0.8;
const EMPTY_ARRIVAL_LINES: MapOverlayLine[] = [];
const EMPTY_MAP_PLACES: MapPlacePin[] = [];

function compactPrice(price: number): string | null {
  if (!Number.isFinite(price) || price <= 0) return null;
  if (price >= 10_000_000) return `₹${(price / 10_000_000).toFixed(1).replace(/\.0$/, "")} Cr`;
  if (price >= 100_000) return `₹${(price / 100_000).toFixed(1).replace(/\.0$/, "")} L`;
  return `₹${Math.round(price).toLocaleString("en-IN")}`;
}

function atlasPlaceMeta(place: ReturnType<typeof buildNumberedPlaces>[number]): string {
  return [
    typeof place.rating === "number" ? `${place.rating.toFixed(1)} rating` : null,
    typeof place.review_count === "number" ? `${place.review_count.toLocaleString("en-IN")} reviews` : null,
  ].filter(Boolean).join(" · ");
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
  presentation = "embedded",
  identity,
}: Props) {
  const { controller: playbackController, state: playbackState } = useArrivalPlaybackController();
  const [societyAutoPlay, setSocietyAutoPlay] = useState(true);
  const [societyPlaybackVersion, setSocietyPlaybackVersion] = useState(0);
  const [approachAutoPlay, setApproachAutoPlay] = useState(true);
  const [selectedSearchSocietyId, setSelectedSearchSocietyId] = useState<string | null>(null);
  const places = context.places ?? EMPTY_MAP_PLACES;
  const normalizedContext = useMemo(
    () => context.places ? context : { ...context, places },
    [context, places],
  );
  const home = useMemo(() => resolveHomeAnchor(normalizedContext), [normalizedContext]);
  const roadLayer = context.layers?.find((layer) => layer.renderKind === "terrain_corridor");
  const entranceLayer = context.layers?.find((layer) => layer.renderKind === "arrival_marker");
  const nearbyLayers = useMemo(() => (context.layers ?? []).filter(layer => layer.id !== 'metro'
    && layer.renderKind !== 'arrival_marker' && layer.renderKind !== 'terrain_corridor'
    && places.some(place => place.layer === layer.id)), [context.layers, places]);
  const [nearbyLayerId, setNearbyLayerId] = useState<string | null>(null);
  const currentNearbyLayer = nearbyLayers.find(layer => layer.id === nearbyLayerId) ?? nearbyLayers[0];
  const nearbyPlaces = useMemo(() => buildNumberedPlaces(places.filter(place =>
    place.layer === currentNearbyLayer?.id)), [places, currentNearbyLayer]);
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
  const hasApproachLayer = Boolean(roadLayer);
  const approachLabel = roadLayer?.label;
  const metroLabel = metroLayer?.label;
  const metroPlaces = useMemo(() => {
    if (!home) return [];
    return buildNumberedPlaces(metroStationsAroundHome(
      places.filter((place) => place.layer === (metroLayer?.id ?? "metro")),
      home,
      context.metro_lines ?? [],
    ));
  }, [context.metro_lines, places, home, metroLayer?.id]);
  const metroLines = useMemo(
    () => home
      ? context.layer_lines?.[metroLayer?.id ?? 'metro'] ?? metroLinesNearArrival(home, metroPlaces, context.metro_lines ?? [])
      : [],
    [context.layer_lines, context.metro_lines, home, metroPlaces, metroLayer?.id],
  );
  const entrancePlaces = useMemo(
    () => arrivalMarkerPlaces(normalizedContext, entranceLayer),
    [normalizedContext, entranceLayer],
  );
  const views = useMemo(() => [...arrivalViewOptions({
    approachLabel,
    hasApproachLayer,
    hasMetroEvidence: metroLines.length > 0 || metroPlaces.length > 0,
    metroLabel,
  }), ...(nearbyLayers.length ? [{id: 'nearby' as const, label: 'Nearby'}] : [])], [
    approachLabel,
    hasApproachLayer,
    metroLabel,
    metroLines.length,
    metroPlaces.length,
    nearbyLayers.length,
  ]);
  const [view, setView] = useState<ArrivalView>(() => views[0]?.id ?? "society");
  const [cameraMode, setCameraMode] = useState<ArrivalCameraMode>(() =>
    views[0]?.id === "metro" ? "evidence" : "home");
  const [expanded, setExpanded] = useState(false);
  const [above, setAbove] = useState(false);
  const [quiet, setQuiet] = useState(false);
  const [showBoundary, setShowBoundary] = useState(true);
  const [selectedPlaceId, setSelectedPlaceId] = useState<string | null>(null);
  const [nearbyDepth, setNearbyDepth] = useState<NearbyDepth>('overview');
  const [tourScope, setTourScope] = useState<'category' | 'neighborhood'>('category');
  const [atlasDrawerOpen, setAtlasDrawerOpen] = useState(false);
  const [nearbyTourRequest, setNearbyTourRequest] = useState<NearbyTourRequest | null>(null);
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
    setSelectedPlaceId(null);
    setNearbyDepth('overview');
    setAbove(false);
    if (next === 'approach') setApproachAutoPlay(true);
    setCameraMode(next === "metro" || next === 'nearby' || next === 'approach'
      ? "evidence"
      : "home");
  }, [activeView, playbackController]);

  const selectAtlasCategory = useCallback((
    next: { view: ArrivalView; layerId?: string },
  ) => {
    if (next.layerId) setNearbyLayerId(next.layerId);
    selectView(next.view);
    setAtlasDrawerOpen(next.view === "metro" || next.view === "nearby");
  }, [selectView]);

  const selectSearchSociety = useCallback((societyId: string) => {
    playbackController.cancel("settled");
    setSocietyAutoPlay(false);
    setApproachAutoPlay(false);
    setView("society");
    setCameraMode("home");
    setSelectedSearchSocietyId(societyId);
  }, [playbackController]);
  const mapHome = useMemo(() => home ? ({
    latitude: home.latitude,
    longitude: home.longitude,
    name: context.home.name,
    boundary: context.home.boundary,
  }) : null, [context.home.boundary, context.home.name, home]);
  const allTourChapters = useMemo<NearbyTourChapter[]>(() => [
    ...nearbyLayers.map((layer) => ({
      categoryId: `nearby:${layer.id}`,
      view: 'nearby' as const,
      layerId: layer.id,
      places: buildNumberedPlaces(places.filter((place) => place.layer === layer.id)),
      polygons: context.layer_polygons?.[layer.id] ?? [],
      lines: context.layer_lines?.[layer.id] ?? [],
    })),
    ...(metroPlaces.length ? [{
      categoryId: 'metro',
      view: 'metro' as const,
      places: metroPlaces,
      polygons: [],
      lines: metroLines,
    }] : []),
  ], [context.layer_lines, context.layer_polygons, metroLines, metroPlaces, nearbyLayers, places]);

  if (!home || views.length === 0) return null;

  const visiblePlaces = activeView === "metro" ? metroPlaces : activeView === 'nearby' ? nearbyPlaces : entrancePlaces;
  const selectedPlace = visiblePlaces.find((place) =>
    (place.feature_id ?? place.name) === selectedPlaceId) ?? null;
  const atlasCategories = [
    { id: "society", label: "Society", view: "society" as ArrivalView },
    ...nearbyLayers.map((layer) => ({
      id: `nearby:${layer.id}`,
      label: layer.label,
      view: "nearby" as ArrivalView,
      layerId: layer.id,
    })),
    ...(metroLines.length > 0 || metroPlaces.length > 0
      ? [{ id: "metro", label: metroLabel ?? "Metro", view: "metro" as ArrivalView }]
      : []),
  ];
  const activeAtlasCategoryId = activeView === "nearby"
    ? `nearby:${currentNearbyLayer?.id ?? ""}`
    : activeView;
  const activeAtlasCategoryLabel = activeView === "nearby"
    ? currentNearbyLayer?.label
    : activeView === "metro"
    ? metroLabel ?? "Metro"
    : "Society";
  const selectPlace = (id: string | null) => {
    playbackController.cancel('settled'); setSelectedPlaceId(id);
    setNearbyDepth(id ? 'pair' : 'overview');
  };
  const tourPlaces = () => {
    if (playbackState === 'playing') { playbackController.pause(); return; }
    if (playbackState === 'paused') { playbackController.resume(); return; }
    setQuiet(true);
    const chapters = tourScope === 'category'
      ? allTourChapters.filter((chapter) => chapter.view === activeView
        && (chapter.view !== 'nearby' || chapter.layerId === currentNearbyLayer?.id))
      : allTourChapters;
    if (chapters.length === 0) return;
    setNearbyTourRequest((current) => ({id: (current?.id ?? 0) + 1, chapters}));
  };
  const applyNearbyTourScene = (scene: NearbyTourSceneState) => {
    setView(scene.view);
    setCameraMode('evidence');
    if (scene.layerId) setNearbyLayerId(scene.layerId);
    setSelectedPlaceId(scene.selectedPlaceId);
    setNearbyDepth(scene.depth);
  };
  const visibleMetroLines = activeView === "metro" ? metroLines : EMPTY_ARRIVAL_LINES;
  const visibleRoadLines = activeView === "approach" ? roadLines : EMPTY_ARRIVAL_LINES;
  const viewport = activeView === "metro" || activeView === 'nearby'
    ? arrivalEvidenceViewport(home, visiblePlaces, visibleMetroLines)
    : {
      center: home,
      radiusKm: SOCIETY_VIEW_RADIUS_KM,
      zoom: 14.6,
      paddingFactor: 0.2,
    };
  const mapSurface = (
    <ArrivalMapBoundary
      unavailableLabel={arrivalExperience?.googleUnavailableState
        ?? "3D view is unavailable in this browser."}
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
          home={mapHome!}
          places={visiblePlaces}
          viewport={viewport}
          metroLines={visibleMetroLines}
          accessLines={visibleRoadLines}
          showMetroLines={activeView === "metro"}
          expanded={expanded}
          above={above}
          quiet={quiet || activeView === 'approach' || activeView === 'metro' || activeView === 'nearby'}
          showBoundary={showBoundary}
          polygons={activeView === 'nearby' ? context.layer_polygons?.[currentNearbyLayer?.id ?? ''] : undefined}
          contextLines={activeView === 'nearby' ? context.layer_lines?.[currentNearbyLayer?.id ?? ''] : undefined}
          selectedPlaceId={selectedPlaceId}
          nearbyDepth={selectedPlaceId ? nearbyDepth : nearbyDepth === 'home' ? 'home' : 'overview'}
          onSelectPlace={(id) => {
            selectPlace(id);
            if (presentation === "atlas") setAtlasDrawerOpen(true);
          }}
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
          showExpandAction={presentation !== "atlas"}
          drawerOpen={presentation === 'atlas' && atlasDrawerOpen}
          nearbyTourRequest={nearbyTourRequest}
          onNearbyTourScene={applyNearbyTourScene}
        />
      </Suspense>
    </ArrivalMapBoundary>
  );

  if (presentation === "atlas") {
    return (
      <section className="property-arrival-map property-arrival-map--atlas" aria-label={`Explore ${identity?.title ?? context.home.name}`}>
        <div className="property-atlas__shade" aria-hidden="true" />
        {mapSurface}

        {identity ? (
          <header className="property-atlas__identity">
            <span>{identity.location}</span>
            <h1>{identity.title}</h1>
            <p>{identity.facts.join(" · ")}</p>
          </header>
        ) : null}

        {identity?.actions ? (
          <div className="property-atlas__actions" aria-label="Property actions">
            {identity.actions}
          </div>
        ) : null}

        <nav className="property-atlas__categories" aria-label="Explore nearby">
          {atlasCategories.map((category) => (
            <button
              key={category.id}
              type="button"
              className={category.id === activeAtlasCategoryId ? "is-active" : undefined}
              aria-pressed={category.id === activeAtlasCategoryId}
              onClick={() => selectAtlasCategory(category)}
            >
              {category.label}
            </button>
          ))}
        </nav>

        <div className="property-atlas__view-tools" role="group" aria-label="Map view">
          <button
            type="button"
            aria-pressed={above}
            disabled={activeView === "approach"}
            onClick={() => {
              playbackController.cancel("settled");
              setSocietyAutoPlay(false);
              setAbove((current) => !current);
            }}
          >
            Above
          </button>
          <button
            type="button"
            aria-pressed={showBoundary}
            disabled={!context.home.boundary}
            onClick={() => setShowBoundary((current) => !current)}
          >
            OSM boundary
          </button>
          <button
            type="button"
            aria-pressed={quiet}
            disabled={!context.home.boundary}
            onClick={() => setQuiet((current) => !current)}
          >
            Quiet surroundings
          </button>
        </div>

        {missingArrivalState ? (
          <p className="property-atlas__status" role="status" aria-live="polite">
            {missingArrivalState}
          </p>
        ) : null}

        {atlasDrawerOpen && (activeView === "metro" || activeView === "nearby") ? (
          <aside className="property-atlas__drawer" aria-label={activeAtlasCategoryLabel}>
            <header>
              <div>
                <span>Nearby</span>
                <h2>{activeAtlasCategoryLabel}</h2>
              </div>
              <button type="button" aria-label="Close nearby places" onClick={() => setAtlasDrawerOpen(false)}>×</button>
            </header>
            <div className="property-atlas__drawer-summary">
              <span>{visiblePlaces.length} {visiblePlaces.length === 1 ? "place" : "places"}</span>
              <button type="button" aria-pressed={!selectedPlaceId && nearbyDepth === 'overview'} onClick={() => selectPlace(null)}>
                Show together
              </button>
            </div>
            <div className="property-atlas__place-list">
              {visiblePlaces.map((place) => {
                const id = place.feature_id ?? place.name;
                return (
                  <button
                    type="button"
                    key={id}
                    className={id === selectedPlaceId ? "is-active" : undefined}
                    aria-pressed={id === selectedPlaceId}
                    onClick={() => selectPlace(id)}
                  >
                    <span>{String(place.number).padStart(2, "0")}</span>
                    <strong>{place.name}</strong>
                    {typeof place.distance_km === "number" ? <small>{place.distance_km.toFixed(1)} km</small> : null}
                  </button>
                );
              })}
            </div>
            {selectedPlace ? (
              <div className="property-atlas__place-card">
                {atlasPlaceMeta(selectedPlace) ? <p>{atlasPlaceMeta(selectedPlace)}</p> : null}
                <div className="property-atlas__focus-actions">
                  <button type="button" aria-pressed={nearbyDepth === 'pair'} onClick={() => selectPlace(selectedPlaceId)}>With home</button>
                  <button type="button" aria-pressed={nearbyDepth === 'inspect'} onClick={() => {
                    playbackController.cancel('settled'); setNearbyDepth('inspect');
                  }}>Look closer</button>
                </div>
                <p>Arc: straight-line connection, not a travel route.</p>
                {selectedPlace.source_url ? (
                  <a href={selectedPlace.source_url} target="_blank" rel="noreferrer">Source ↗</a>
                ) : null}
              </div>
            ) : null}
            <button type="button" className="property-atlas__tour" onClick={tourPlaces}>
              {playbackState === "playing" ? "Pause tour" : playbackState === "paused" ? "Resume tour" : tourScope === 'neighborhood' ? 'Tour neighborhood' : `Tour ${activeAtlasCategoryLabel?.toLocaleLowerCase("en-IN") ?? "places"}`}
            </button>
            <select className="property-atlas__tour-scope" aria-label="Tour scope" value={tourScope}
              disabled={playbackState === 'playing' || playbackState === 'paused'}
              onChange={event => setTourScope(event.target.value as 'category' | 'neighborhood')}>
              <option value="category">This category</option>
              <option value="neighborhood">Whole neighborhood</option>
            </select>
          </aside>
        ) : null}

        <div className="property-atlas__dock" role="group" aria-label="Atlas journey">
          <button
            type="button"
            className={activeView === "society" ? "is-active" : undefined}
            onClick={() => selectAtlasCategory({ view: "society" })}
          >
            Home
          </button>
          <button
            type="button"
            disabled={!hasApproachLayer}
            className={activeView === "approach" ? "is-active" : undefined}
            onClick={() => selectAtlasCategory({ view: "approach" })}
          >
            Road journey
          </button>
          {navigationAction && navigationActionText ? (
            <button
              type="button"
              className="property-atlas__play"
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
              <span aria-hidden="true">{navigationAction === "pause" ? "Ⅱ" : "▶"}</span>
              {navigationActionText}
            </button>
          ) : null}
          <button
            type="button"
            onClick={() => {
              const next = atlasCategories.find((category) => category.view === "nearby")
                ?? atlasCategories.find((category) => category.view === "metro");
              if (next) selectAtlasCategory(next);
            }}
          >
            Nearby
          </button>
        </div>
      </section>
    );
  }

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
        <details className="atlas-view-settings">
          <summary>View</summary>
          <div>
            <label><input type="checkbox" checked={above} disabled={activeView === 'approach'}
              onChange={e => { playbackController.cancel('settled'); setSocietyAutoPlay(false); setAbove(e.target.checked); }} />From above</label>
            <label><input type="checkbox" checked={showBoundary} onChange={e => setShowBoundary(e.target.checked)} />Society boundary</label>
            <label><input type="checkbox" checked={quiet} disabled={activeView !== 'society' || !context.home.boundary}
              onChange={e => setQuiet(e.target.checked)} />Quiet surroundings</label>
          </div>
        </details>
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
      {activeView === 'nearby' && <label className="atlas-nearby-category">
        <span className="sr-only">Nearby category</span>
        <select aria-label="Nearby category" value={currentNearbyLayer?.id ?? ''} onChange={e => {
          playbackController.cancel('settled'); setSelectedPlaceId(null); setNearbyLayerId(e.target.value);
        }}>{nearbyLayers.map(layer => <option key={layer.id} value={layer.id}>{layer.label}</option>)}</select>
      </label>}
      {missingArrivalState && (
        <p className="property-arrival-map__status" role="status" aria-live="polite">
          {missingArrivalState}
        </p>
      )}
      {mapSurface}
      {(activeView === 'metro' || activeView === 'nearby') && visiblePlaces.length > 0 && <div className="atlas-place-list" aria-label="Nearby places">
        <button type="button" onClick={tourPlaces}>{playbackState === 'playing' ? 'Pause' : playbackState === 'paused' ? 'Resume' : 'Tour places'}</button>
        <button type="button" aria-pressed={!selectedPlaceId} onClick={() => selectPlace(null)}>Show all</button>
        {visiblePlaces.map((place, index) => <button type="button" key={place.feature_id ?? place.name}
          aria-pressed={selectedPlaceId === (place.feature_id ?? place.name)} onClick={() => selectPlace(place.feature_id ?? place.name)}>
          <span>{index + 1}</span>{place.name}
          {typeof place.distance_km === 'number' && <small>{place.distance_km.toFixed(1)} km straight-line</small>}
        </button>)}
      </div>}
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
