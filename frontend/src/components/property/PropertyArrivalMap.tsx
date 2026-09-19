import {
  Component,
  lazy,
  Suspense,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ErrorInfo,
  type ReactNode,
} from "react";
import { Link } from "react-router-dom";
import { AtlasIcon } from "./AtlasIcon.tsx";
import atlasPolicy from '../../lib/atlasPolicy.ts';
import { distanceMetres } from '../../lib/atlas/geometry.ts';
import type { NearbyDepth } from '../../lib/atlasNearbyScene.ts';
import type {
  AtlasCameraRequest,
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
  placeMatchesProofFocus,
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
  photos?: ReactNode;
  reviewsTargetId?: string;
  pageScrollable?: boolean;
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
  { children: ReactNode; onUnavailable?: () => void; onRetry?: () => void; unavailableLabel?: string },
  { failed: boolean }
> {
  state = { failed: false };

  static getDerivedStateFromError() {
    return { failed: true };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("[PropertyArrivalMap] Map unavailable", error, info);
    this.props.onUnavailable?.();
  }

  render() {
    if (this.state.failed) {
      return (
        <div
          className="property-arrival-map__unavailable"
          role="status"
        >
          <p>{this.props.unavailableLabel}</p>
          <button type="button" onClick={() => { this.props.onRetry?.(); this.setState({ failed: false }); }}>Retry map</button>
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
  photos,
  reviewsTargetId,
  pageScrollable = false,
}: Props) {
  const { controller: playbackController, state: playbackState } = useArrivalPlaybackController();
  const [societyAutoPlay, setSocietyAutoPlay] = useState(presentation !== "atlas");
  const [societyPlaybackVersion, setSocietyPlaybackVersion] = useState(0);
  const [approachAutoPlay, setApproachAutoPlay] = useState(true);
  const [selectedSearchSocietyId, setSelectedSearchSocietyId] = useState<string | null>(null);
  const [mapStatus, setMapStatus] = useState<"loading" | "ready" | "unavailable">("loading");
  const atlasRef = useRef<HTMLElement>(null);
  const atlasVisibleRef = useRef(true);
  const handleMapReady = useCallback(() => {
    setMapStatus("ready");
    if (presentation === "atlas" && atlasVisibleRef.current && !document.hidden) setSocietyAutoPlay(true);
  }, [presentation]);
  useEffect(() => {
    if (presentation !== "atlas") return;
    const pauseWhenHidden = () => {
      if (!atlasVisibleRef.current || document.hidden) playbackController.pause();
    };
    const observer = new IntersectionObserver(([entry]) => {
      atlasVisibleRef.current = entry.isIntersecting;
      pauseWhenHidden();
    });
    if (atlasRef.current) observer.observe(atlasRef.current);
    document.addEventListener("visibilitychange", pauseWhenHidden);
    return () => {
      observer.disconnect();
      document.removeEventListener("visibilitychange", pauseWhenHidden);
    };
  }, [presentation, playbackController]);
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
    const selected = buildNumberedPlaces(metroStationsAroundHome(
      places.filter((place) => place.layer === (metroLayer?.id ?? "metro")),
      home,
      context.metro_lines ?? [],
    )).sort((left, right) => distanceMetres(
      {lat: home.latitude, lng: home.longitude},
      {lat: left.latitude, lng: left.longitude},
    ) - distanceMetres(
      {lat: home.latitude, lng: home.longitude},
      {lat: right.latitude, lng: right.longitude},
    ));
    return selected.map((place, index) => ({...place, number: index + 1}));
  }, [context.metro_lines, places, home, metroLayer?.id]);
  const metroSourceLines = useMemo(
    () => context.layer_lines?.[metroLayer?.id ?? 'metro'] ?? context.metro_lines ?? [],
    [context.layer_lines, context.metro_lines, metroLayer?.id],
  );
  const restingMetroPlaces = useMemo(
    () => metroPlaces.slice(0, atlasPolicy.metro.restingPlaceCount),
    [metroPlaces],
  );
  const restingMetroLines = useMemo(
    () => home ? metroLinesNearArrival(
      home,
      restingMetroPlaces,
      metroSourceLines,
      atlasPolicy.metro.vicinityRadiusM,
    ) : [],
    [home, metroSourceLines, restingMetroPlaces],
  );
  const entrancePlaces = useMemo(
    () => arrivalMarkerPlaces(normalizedContext, entranceLayer),
    [normalizedContext, entranceLayer],
  );
  const views = useMemo(() => [...arrivalViewOptions({
    approachLabel,
    hasApproachLayer,
    hasMetroEvidence: metroSourceLines.length > 0 || metroPlaces.length > 0,
    metroLabel,
  }), ...(nearbyLayers.length ? [{id: 'nearby' as const, label: 'Nearby'}] : [])], [
    approachLabel,
    hasApproachLayer,
    metroLabel,
    metroSourceLines.length,
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
  const [nearbySelectionVersion, setNearbySelectionVersion] = useState(0);
  const [tourScope, setTourScope] = useState<'category' | 'neighborhood'>('category');
  const [panel, setPanel] = useState<"nearby" | "photos" | null>(null);
  const atlasDrawerOpen = panel !== null;
  const panelRef = useRef<HTMLElement>(null);
  const panelTriggerRef = useRef<HTMLElement | null>(null);
  const [cameraRequest, setCameraRequest] = useState<AtlasCameraRequest | null>(null);
  const appliedProofRef = useRef<string | null>(null);
  useEffect(() => {
    if (panel !== 'nearby' || !selectedPlaceId) return;
    const list = panelRef.current?.querySelector<HTMLElement>('.property-atlas__place-list');
    const selected = list?.querySelector<HTMLElement>('.is-selected');
    if (!list || !selected) return;
    const listRect = list.getBoundingClientRect();
    const selectedRect = selected.getBoundingClientRect();
    // Keep tour selection within its own list, without pulling the document
    // away from reviews when a pending scene completes below the fold.
    if (selectedRect.top < listRect.top) list.scrollTop += selectedRect.top - listRect.top;
    else if (selectedRect.bottom > listRect.bottom) list.scrollTop += selectedRect.bottom - listRect.bottom;
  }, [panel, selectedPlaceId]);
  const openPanel = useCallback((next: typeof panel) => {
    if (document.activeElement instanceof HTMLElement && !panelRef.current?.contains(document.activeElement)) {
      panelTriggerRef.current = document.activeElement;
    }
    setPanel(next);
  }, []);
  const closePanel = useCallback(() => {
    setPanel(null);
    panelTriggerRef.current?.focus({ preventScroll: true });
  }, []);

  useEffect(() => {
    if (!panel) return;
    panelRef.current?.focus({ preventScroll: true });
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !document.querySelector('[aria-modal="true"], dialog[open]')) {
        event.preventDefault();
        closePanel();
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [panel, closePanel]);

  useEffect(() => {
    const focus = context.proof_focus;
    if (!focus || focus.destinationKind === "section") return;
    const place = places.find((candidate) => placeMatchesProofFocus(candidate, focus));
    if (!place) return;
    const proofKey = JSON.stringify(focus);
    if (appliedProofRef.current === proofKey) return;
    const frame = requestAnimationFrame(() => {
      appliedProofRef.current = proofKey;
      playbackController.cancel("settled");
      setSocietyAutoPlay(false);
      setNearbyLayerId(place.layer);
      setView(place.layer === metroLayer?.id ? "metro" : "nearby");
      setCameraMode("evidence");
      setSelectedPlaceId(place.feature_id ?? place.name);
      setNearbyDepth("inspect");
      setPanel("nearby");
    });
    return () => cancelAnimationFrame(frame);
  }, [context.proof_focus, places, metroLayer?.id, playbackController]);
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
    setSelectedSearchSocietyId(null);
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
    if (next.view === "metro" || next.view === "nearby") openPanel("nearby");
    else setPanel(null);
  }, [selectView, openPanel]);

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
  const mapMetroPlaces = useMemo(() => {
    if (!selectedPlaceId) return restingMetroPlaces;
    const selected = metroPlaces.find((place) => (place.feature_id ?? place.name) === selectedPlaceId);
    return selected ? [selected] : restingMetroPlaces;
  }, [metroPlaces, restingMetroPlaces, selectedPlaceId]);
  const metroLines = useMemo(() => home ? metroLinesNearArrival(
    home,
    mapMetroPlaces,
    metroSourceLines,
    atlasPolicy.metro.vicinityRadiusM,
  ) : [], [home, mapMetroPlaces, metroSourceLines]);
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
      lines: restingMetroLines,
    }] : []),
  ], [context.layer_lines, context.layer_polygons, restingMetroLines, metroPlaces, nearbyLayers, places]);

  const selectPlace = useCallback((id: string | null) => {
    playbackController.cancel('settled');
    setSelectedPlaceId(id);
    setNearbyDepth(id ? 'inspect' : 'overview');
    setNearbySelectionVersion((version) => version + 1);
  }, [playbackController]);
  const selectMapPlace = useCallback((id: string | null) => {
    selectPlace(id);
    if (presentation === "atlas") openPanel("nearby");
  }, [selectPlace, presentation, openPanel]);

  if (!home || views.length === 0) return null;

  const visiblePlaces = activeView === "metro" ? metroPlaces : activeView === 'nearby' ? nearbyPlaces : entrancePlaces;
  const mapPlaces = activeView === "metro" ? mapMetroPlaces : visiblePlaces;
  const selectedPlace = visiblePlaces.find((place) =>
    (place.feature_id ?? place.name) === selectedPlaceId) ?? null;
  const selectedDistanceM = selectedPlace && home ? distanceMetres(
    {lat: home.latitude, lng: home.longitude},
    {lat: selectedPlace.latitude, lng: selectedPlace.longitude},
  ) : 0;
  const atlasQuiet = atlasPolicy.spotlight.enabled && (
    activeView === "society"
    || activeView === "approach"
    || selectedDistanceM >= atlasPolicy.spotlight.distantFocusThresholdM
  );
  const atlasCategories = [
    { id: "society", label: "Home", view: "society" as ArrivalView },
    ...nearbyLayers.map((layer) => ({
      id: `nearby:${layer.id}`,
      label: layer.label,
      view: "nearby" as ArrivalView,
      layerId: layer.id,
    })),
    ...(metroLines.length > 0 || metroPlaces.length > 0
      ? [{ id: "metro", label: metroLabel ?? "Metro", view: "metro" as ArrivalView }]
      : []),
    ...(hasApproachLayer ? [{ id: "approach", label: approachLabel ?? "The way in", view: "approach" as ArrivalView }] : []),
  ];
  const activeAtlasCategoryId = activeView === "nearby"
    ? `nearby:${currentNearbyLayer?.id ?? ""}`
    : activeView;
  const activeAtlasCategoryLabel = activeView === "nearby"
    ? currentNearbyLayer?.label
    : activeView === "metro"
    ? metroLabel ?? "Metro"
    : "Society";
  const tourPlaces = () => {
    if (playbackState === 'playing') { playbackController.pause(); return; }
    if (playbackState === 'paused') { playbackController.resume(); return; }
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
    ? arrivalEvidenceViewport(home, mapPlaces, visibleMetroLines)
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
      onUnavailable={() => { setMapStatus("unavailable"); onUnavailable?.(); }}
      onRetry={() => setMapStatus("loading")}
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
          places={mapPlaces}
          viewport={viewport}
          metroLines={visibleMetroLines}
          accessLines={visibleRoadLines}
          showMetroLines={activeView === "metro"}
          expanded={expanded}
          above={above}
          quiet={presentation === "atlas" ? atlasQuiet : quiet}
          showBoundary={showBoundary}
          polygons={activeView === 'nearby' ? context.layer_polygons?.[currentNearbyLayer?.id ?? ''] : undefined}
          contextLines={activeView === 'nearby' ? context.layer_lines?.[currentNearbyLayer?.id ?? ''] : undefined}
          selectedPlaceId={selectedPlaceId}
          nearbySelectionVersion={nearbySelectionVersion}
          nearbyDepth={selectedPlaceId ? nearbyDepth : nearbyDepth === 'home' ? 'home' : 'overview'}
          nearbyCameraOrientation={activeView === 'nearby'
            ? 'selected-home-foreground'
            : 'category-stable'}
          onSelectPlace={selectMapPlace}
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
          cameraRequest={cameraRequest}
          immersive={presentation === "atlas"}
          pageScrollable={pageScrollable || Boolean(reviewsTargetId)}
          onReady={handleMapReady}
          nearbyTourRequest={nearbyTourRequest}
          onNearbyTourScene={applyNearbyTourScene}
        />
      </Suspense>
    </ArrivalMapBoundary>
  );

  if (presentation === "atlas") {
    const cameraAction = (action: AtlasCameraRequest["action"]) => {
      playbackController.cancel("settled");
      setSocietyAutoPlay(false);
      setApproachAutoPlay(false);
      setCameraRequest((current) => ({ action, id: (current?.id ?? 0) + 1 }));
    };
    return (
      <section id="property-atlas" ref={atlasRef} tabIndex={-1} className={`property-arrival-map property-arrival-map--atlas${panel ? " has-panel" : ""}`} aria-label="Property atlas">
        {mapSurface}
        <div className="property-atlas__shade" aria-hidden="true" />
        {activeView === "approach" && missingArrivalState && (
          <p className="property-atlas__status" role="status">{missingArrivalState}</p>
        )}
        {identity && (
          <header className="property-atlas__identity">
            <span>{identity.location}</span>
            <h1>{identity.title}</h1>
            <p>{identity.facts.join(" · ")}</p>
          </header>
        )}
        <div className="property-atlas__actions" aria-label="Property actions">
          {photos && <button type="button" aria-expanded={panel === "photos"} aria-controls="property-atlas-panel" onClick={() => panel === "photos" ? closePanel() : openPanel("photos")}><AtlasIcon name="photos" />Photos</button>}
          {reviewsTargetId && <button type="button" aria-controls={reviewsTargetId} onClick={() => {
            playbackController.pause();
            const target = document.getElementById(reviewsTargetId);
            target?.scrollIntoView({ behavior: window.matchMedia("(prefers-reduced-motion: reduce)").matches ? "instant" : "smooth", block: "start" });
            target?.focus({ preventScroll: true });
          }}><AtlasIcon name="reviews" />Reviews</button>}
          {identity?.actions}
        </div>

        {panel && (
          <aside id="property-atlas-panel" ref={panelRef} tabIndex={-1} className={`property-atlas__drawer property-atlas__drawer--${panel}`} aria-label={panel === "nearby" ? "Nearby places" : "Photos"}>
            <header>
              <h2>{panel === "nearby" ? activeAtlasCategoryLabel : "Photos"}</h2>
              <button type="button" aria-label="Close panel" onClick={closePanel}><AtlasIcon name="close" /></button>
            </header>
            <div className="property-atlas__panel-body">
            {panel === "photos" ? photos : (
              <>
                <div className="property-atlas__drawer-summary">
                  <span>Straight-line distances</span>
                  <button type="button" aria-pressed={!selectedPlaceId && nearbyDepth === "overview"} onClick={() => selectPlace(null)}>Show together</button>
                </div>
                <div className="property-atlas__place-list">
                  {visiblePlaces.map((place) => {
                    const id = place.feature_id ?? place.name;
                    return (
                      <div className={id === selectedPlaceId ? "is-selected" : undefined} key={id}>
                        <button type="button" className={id === selectedPlaceId ? "is-active" : undefined} aria-pressed={id === selectedPlaceId} onClick={() => selectPlace(id)}>
                          <span className="property-atlas__place-number">{String(place.number).padStart(2, "0")}</span>
                          <span className="property-atlas__place-copy"><strong title={place.name}>{place.name}</strong>
                            {atlasPlaceMeta(place) && <small>{atlasPlaceMeta(place)}</small>}
                          </span>
                          {typeof place.distance_km === "number" && <small className="property-atlas__place-distance">{place.distance_km < 1 ? `${Math.round(place.distance_km * 1000)} m` : `${place.distance_km.toFixed(1)} km`}</small>}
                        </button>
                        {id === selectedPlaceId && selectedPlace && (
                          <div className="property-atlas__place-card">
                            {placeMatchesProofFocus(selectedPlace, context.proof_focus) && <p>Matched your search</p>}
                            <div className="property-atlas__focus-actions">
                              <button type="button" disabled={mapStatus !== "ready"} onClick={() => selectPlace(selectedPlaceId)}><AtlasIcon name="rotate" />Replay view</button>
                              {selectedPlace.source_url && <a href={selectedPlace.source_url} target="_blank" rel="noreferrer">Source ↗</a>}
                            </div>
                          </div>
                        )}
                      </div>
                    );
                  })}
                </div>
                <div className="property-atlas__tour-controls">
                  <button type="button" className="property-atlas__tour" disabled={mapStatus !== "ready" || visiblePlaces.length === 0} onClick={tourPlaces}>
                    {playbackState === "playing" ? "Pause tour" : playbackState === "paused" ? "Resume tour" : tourScope === "neighborhood" ? "Tour neighborhood" : `Tour ${activeAtlasCategoryLabel?.toLocaleLowerCase("en-IN") ?? "places"}`}
                  </button>
                  {(playbackState === "playing" || playbackState === "paused") ? (
                    <button type="button" onClick={() => playbackController.cancel("settled")}>End tour</button>
                  ) : (
                    <select className="property-atlas__tour-scope" aria-label="Tour scope" value={tourScope} onChange={(event) => setTourScope(event.target.value as "category" | "neighborhood")}>
                      <option value="category">This category</option>
                      <option value="neighborhood">Neighborhood</option>
                    </select>
                  )}
                </div>
              </>
            )}
            </div>
          </aside>
        )}

        <div className="property-atlas__view-tools" role="group" aria-label="Camera controls">
          <button type="button" aria-label="Zoom in" title="Zoom in" disabled={mapStatus !== "ready" || activeView === "approach"} onClick={() => cameraAction("zoom-in")}><AtlasIcon name="plus" /></button>
          <button type="button" aria-label="Zoom out" title="Zoom out" disabled={mapStatus !== "ready" || activeView === "approach"} onClick={() => cameraAction("zoom-out")}><AtlasIcon name="minus" /></button>
          <button type="button" aria-label="View another side" title="View another side" disabled={mapStatus !== "ready" || activeView === "approach"} onClick={() => cameraAction("rotate")}><AtlasIcon name="rotate" /></button>
          <button type="button" aria-label="Top view" title="Top view" aria-pressed={above} disabled={mapStatus !== "ready" || activeView === "approach"} onClick={() => { playbackController.cancel("settled"); setSocietyAutoPlay(false); setAbove((current) => !current); }}><AtlasIcon name="top" /></button>
          <button type="button" aria-label="Site outline" title="Site outline" aria-pressed={showBoundary} disabled={mapStatus !== "ready" || !context.home.boundary} onClick={() => setShowBoundary((current) => !current)}>
            <AtlasIcon name="outline" />
          </button>
        </div>

        <nav className="property-atlas__dock" aria-label="Explore this home">
          <div className="property-atlas__category-track">
            {atlasCategories.map((category) => <button key={category.id} type="button" className={activeAtlasCategoryId === category.id ? "is-active" : undefined} aria-pressed={activeAtlasCategoryId === category.id} onClick={() => {
              selectAtlasCategory(category);
              if (category.view === "society") { setSocietyAutoPlay(true); setSocietyPlaybackVersion((current) => current + 1); }
            }}>{category.label}</button>)}
          </div>
          {navigationAction && navigationActionText && <button type="button" className="property-atlas__play" aria-label={navigationAction === "play" ? "Tour this home" : navigationActionLabel ?? navigationActionText} onClick={() => {
            if (navigationAction === "pause") playbackController.pause();
            else if (navigationAction === "resume") playbackController.resume();
            else if (approachReplayAvailable) setApproachAutoPlay(true);
            else { setSocietyAutoPlay(true); setSocietyPlaybackVersion((current) => current + 1); }
          }}><AtlasIcon name={navigationAction === "pause" ? "pause" : "play"} />{navigationAction === "play" ? "Tour" : navigationActionText}</button>}
        </nav>
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
              onChange={e => { playbackController.cancel('settled'); setSocietyAutoPlay(false); setAbove(e.target.checked); }} />Aerial</label>
            <label><input type="checkbox" checked={showBoundary} onChange={e => setShowBoundary(e.target.checked)} />Site outline</label>
            <label><input type="checkbox" checked={quiet} disabled={activeView !== 'society' || !context.home.boundary}
              onChange={e => setQuiet(e.target.checked)} />Focus</label>
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
