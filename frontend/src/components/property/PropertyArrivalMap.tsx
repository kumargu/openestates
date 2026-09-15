import {
  Component,
  createRef,
  lazy,
  Suspense,
  useCallback,
  useEffect,
  useMemo,
  useReducer,
  useRef,
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
import {
  initialAtlasNearbyUiState,
  reduceAtlasNearbyUi,
} from "../../lib/atlasNearbyUi.ts";
import { atlasPolicy } from "../../lib/atlasUiPolicy.ts";
import { labelsForNearbyPlace } from "../../lib/notebook.ts";
import { useNotebook } from "../../hooks/useNotebook.ts";
import { SoftNearbyIcon } from "../ui/SoftIcons.tsx";
import "../../styles/property-arrival.css";

const GoogleArrivalMap = lazy(async () => {
  const module = await import("./PropertyArrivalGoogle3DMap.tsx");
  return { default: module.PropertyArrivalGoogle3DMap };
});

type Props = {
  propertyId?: string;
  context: PropertyMapContext;
  initialProofFocus?: PropertyMapContext["proof_focus"];
  searchContextSocieties?: ArrivalSearchSociety[];
  onUnavailable?: () => void;
  presentation?: "embedded" | "atlas" | "approach";
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

function atlasDirectionsUrl(
  place: ReturnType<typeof buildNumberedPlaces>[number],
): string | null {
  if (!Number.isFinite(place.latitude) || !Number.isFinite(place.longitude)) {
    return null;
  }
  return `https://www.google.com/maps/dir/?api=1&destination=${encodeURIComponent(
    `${place.latitude},${place.longitude}`,
  )}`;
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
  propertyId,
  context,
  initialProofFocus,
  searchContextSocieties = [],
  onUnavailable,
  presentation = "embedded",
}: Props) {
  const { notes, toggleFact } = useNotebook();
  const { controller: playbackController, state: playbackState } = useArrivalPlaybackController();
  const [societyAutoPlay, setSocietyAutoPlay] = useState(
    presentation !== "atlas",
  );
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
    && (
      places.some(place => place.layer === layer.id)
      || (context.layer_lines?.[layer.id]?.length ?? 0) > 0
      || (context.layer_polygons?.[layer.id]?.length ?? 0) > 0
    )), [context.layer_lines, context.layer_polygons, context.layers, places]);
  const metroLayer = context.layers?.find((layer) => layer.id === "metro");
  const initialNearbyFocus = useMemo(() => {
    const focus = initialProofFocus;
    if (!focus?.layerId) return null;
    const categoryId = focus.layerId === metroLayer?.id
      ? "metro"
      : nearbyLayers.some((layer) => layer.id === focus.layerId)
        ? `nearby:${focus.layerId}`
        : null;
    if (!categoryId) return null;
    const place = places.find((candidate) =>
      placeMatchesProofFocus(candidate, focus));
    return {
      categoryId,
      placeId: place ? place.feature_id ?? place.name : null,
    };
  }, [initialProofFocus, metroLayer?.id, nearbyLayers, places]);
  const [nearbyUi, dispatchNearbyUi] = useReducer(
    reduceAtlasNearbyUi,
    initialNearbyFocus,
    (focus) => {
      if (!focus) return initialAtlasNearbyUiState;
      const browsing = reduceAtlasNearbyUi(initialAtlasNearbyUiState, {
        type: "open_browse",
        categoryId: focus.categoryId,
      });
      return focus.placeId
        ? reduceAtlasNearbyUi(browsing, {
          type: "select_place",
          placeId: focus.placeId,
        })
        : browsing;
    },
  );
  const currentNearbyLayer = nearbyLayers.find(
    (layer) => `nearby:${layer.id}` === nearbyUi.categoryId,
  ) ?? nearbyLayers[0];
  const nearbyPlaces = useMemo(() => buildNumberedPlaces(places.filter(place =>
    place.layer === currentNearbyLayer?.id)), [places, currentNearbyLayer]);
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
    () => {
      if (!home) return [];
      const scopedLines = context.layer_lines?.[metroLayer?.id ?? "metro"];
      return scopedLines && scopedLines.length > 0
        ? scopedLines
        : metroLinesNearArrival(home, metroPlaces, context.metro_lines ?? []);
    },
    [context.layer_lines, context.metro_lines, home, metroPlaces, metroLayer?.id],
  );
  const entrancePlaces = useMemo(
    () => arrivalMarkerPlaces(normalizedContext, entranceLayer),
    [normalizedContext, entranceLayer],
  );
  const views = useMemo(() => {
    const availableViews = [...arrivalViewOptions({
    approachLabel,
    hasApproachLayer,
    hasMetroEvidence: metroLines.length > 0 || metroPlaces.length > 0,
    metroLabel,
    }), ...(nearbyLayers.length ? [{id: 'nearby' as const, label: 'Nearby'}] : [])];
    return presentation === "approach"
      ? availableViews.filter((candidate) => candidate.id === "approach")
      : availableViews;
  }, [
    approachLabel,
    hasApproachLayer,
    metroLabel,
    metroLines.length,
    metroPlaces.length,
    nearbyLayers.length,
    presentation,
  ]);
  const [view, setView] = useState<ArrivalView>(() => {
    const focusedView: ArrivalView | null = nearbyUi.mode === "rest"
      ? null
      : nearbyUi.categoryId === "metro"
        ? "metro"
        : "nearby";
    return focusedView && views.some((candidate) => candidate.id === focusedView)
      ? focusedView
      : views[0]?.id ?? "society";
  });
  const [cameraMode, setCameraMode] = useState<ArrivalCameraMode>(() =>
    nearbyUi.mode === "rest" && views[0]?.id === "society"
      ? "home"
      : "evidence");
  const [expanded, setExpanded] = useState(false);
  const [above, setAbove] = useState(false);
  const [quiet, setQuiet] = useState(false);
  const [showBoundary, setShowBoundary] = useState(true);
  const [nearbyTourDepth, setNearbyTourDepth] = useState<NearbyDepth>("overview");
  const [nearbyTourRequest, setNearbyTourRequest] = useState<NearbyTourRequest | null>(null);
  const [nearbyScopeId, setNearbyScopeId] = useState(
    atlasPolicy.nearby.scope.defaultId,
  );
  const layerButtonRefs = useRef(new Map<string, HTMLButtonElement>());
  const activeView = views.some((candidate) => candidate.id === view)
    ? view
    : views[0]?.id ?? "society";
  const activeCameraMode = activeView === view
    ? cameraMode
    : activeView === "society"
      ? "home"
      : "evidence";
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
      if (nearbyUi.mode === "tour") {
        dispatchNearbyUi({ type: "stop_tour" });
      }
      if (activeView === "approach") setCameraMode("evidence");
    };
    reducedMotion.addEventListener("change", stopForReducedMotion);
    return () => reducedMotion.removeEventListener("change", stopForReducedMotion);
  }, [activeView, nearbyUi.mode, playbackController]);

  const selectView = useCallback((next: ArrivalView) => {
    playbackController.cancel("settled");
    if (activeView === "society") setSocietyAutoPlay(false);
    if (activeView === "approach") setApproachAutoPlay(false);
    setView(next);
    if (next === "nearby") {
      dispatchNearbyUi({
        type: "open_browse",
        categoryId: `nearby:${currentNearbyLayer?.id ?? ""}`,
      });
    } else if (next === "metro") {
      dispatchNearbyUi({ type: "open_browse", categoryId: "metro" });
    } else {
      dispatchNearbyUi({ type: "close" });
    }
    setAbove(false);
    if (next === "society") setQuiet(false);
    if (next === 'approach') setApproachAutoPlay(true);
    setCameraMode(next === "metro" || next === 'nearby' || next === 'approach'
      ? "evidence"
      : "home");
  }, [activeView, currentNearbyLayer?.id, playbackController]);

  const selectAtlasCategory = useCallback((
    next: { id: string; view: ArrivalView; layerId?: string },
  ) => {
    playbackController.cancel("settled");
    setSocietyAutoPlay(false);
    setApproachAutoPlay(false);
    setView(next.view);
    setCameraMode("evidence");
    setAbove(false);
    dispatchNearbyUi({ type: "change_category", categoryId: next.id });
  }, [playbackController]);

  const selectSearchSociety = useCallback((societyId: string) => {
    playbackController.cancel("settled");
    setSocietyAutoPlay(false);
    setApproachAutoPlay(false);
    setView("society");
    setCameraMode("home");
    dispatchNearbyUi({ type: "close" });
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
  const atlasNearbyCategories = [
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
  const closeAtlasNearby = useCallback(() => {
    selectView("society");
  }, [selectView]);
  const clearSelectedPlace = useCallback(() => {
    const categoryId = nearbyUi.categoryId;
    dispatchNearbyUi({
      type: "change_category",
      categoryId: categoryId ?? "",
    });
    window.requestAnimationFrame(() => {
      if (categoryId) layerButtonRefs.current.get(categoryId)?.focus();
    });
  }, [nearbyUi.categoryId]);

  useEffect(() => {
    if (presentation !== "atlas" || nearbyUi.mode === "rest") return undefined;
    const onEscape = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      if (nearbyUi.cameraOwner === "user") {
        dispatchNearbyUi({ type: "reset_camera" });
        return;
      }
      if (nearbyUi.mode === "tour") {
        playbackController.cancel("settled");
        dispatchNearbyUi({ type: "stop_tour" });
        return;
      }
      if (nearbyUi.mode === "selected") {
        clearSelectedPlace();
        return;
      }
      closeAtlasNearby();
    };
    document.addEventListener("keydown", onEscape);
    return () => document.removeEventListener("keydown", onEscape);
  }, [
    clearSelectedPlace,
    closeAtlasNearby,
    nearbyUi.cameraOwner,
    nearbyUi.mode,
    playbackController,
    presentation,
  ]);

  if (!home || views.length === 0) return null;

  const selectedPlaceId = nearbyUi.selectedPlaceId;
  const nearbyScope = atlasPolicy.nearby.scope.options.find(
    (option) => option.id === nearbyScopeId,
  ) ?? atlasPolicy.nearby.scope.options[0];
  const placeIsInScope = (
    place: ReturnType<typeof buildNumberedPlaces>[number],
  ) => nearbyScope.radiusKm === null
    || typeof place.distance_km !== "number"
    || place.distance_km <= nearbyScope.radiusKm
    || (place.feature_id ?? place.name) === selectedPlaceId
    || placeMatchesProofFocus(place, context.proof_focus);
  const scopedNearbyPlaces = presentation === "atlas"
    ? nearbyPlaces.filter(placeIsInScope)
    : nearbyPlaces;
  const scopedMetroPlaces = presentation === "atlas"
    ? metroPlaces.filter(placeIsInScope)
    : metroPlaces;
  const visiblePlaces = activeView === "metro"
    ? scopedMetroPlaces
    : activeView === "nearby"
      ? buildNumberedPlaces(scopedNearbyPlaces)
      : entrancePlaces;
  const selectedPlace = visiblePlaces.find((place) =>
    (place.feature_id ?? place.name) === selectedPlaceId) ?? null;
  const selectedPlaceCatalogKey = propertyId && selectedPlace
    ? `nearby:${propertyId}:${selectedPlace.feature_id ?? selectedPlace.name}`
    : null;
  const selectedPlaceNoted = selectedPlaceCatalogKey
    ? notes.some((note) =>
      note.propertyId === propertyId && note.catalogKey === selectedPlaceCatalogKey)
    : false;
  const nearbyDepth: NearbyDepth = nearbyUi.mode === "tour"
    ? nearbyTourDepth
    : selectedPlaceId
      ? nearbyUi.framing === "closer" ? "inspect" : "pair"
      : "overview";
  const selectPlace = (id: string) => {
    playbackController.cancel("settled");
    dispatchNearbyUi({ type: "select_place", placeId: id });
  };
  const rememberSelectedPlace = () => {
    if (!propertyId || !selectedPlace || !selectedPlaceCatalogKey) return;
    toggleFact({
      propertyId,
      catalogKey: selectedPlaceCatalogKey,
      title: selectedPlace.name,
      labels: labelsForNearbyPlace(
        selectedPlace.layer,
        selectedPlace.distance_km,
      ),
      detail: [
        typeof selectedPlace.distance_km === "number"
          ? `${selectedPlace.distance_km.toFixed(1)} km`
          : null,
        atlasPlaceMeta(selectedPlace),
      ].filter(Boolean).join(" · "),
      source: "Around this home",
      kind: "fact",
    });
  };
  const changeNearbyScope = (scopeId: string) => {
    playbackController.cancel("settled");
    if (nearbyUi.mode === "tour") {
      dispatchNearbyUi({ type: "stop_tour" });
    }
    setNearbyScopeId(scopeId);
  };
  const tourPlaces = () => {
    if (playbackState === 'playing') { playbackController.pause(); return; }
    if (playbackState === 'paused') { playbackController.resume(); return; }
    setQuiet(true);
    const visiblePlaceIds = new Set(visiblePlaces.map(
      (place) => place.feature_id ?? place.name,
    ));
    const chapters = allTourChapters
      .filter((chapter) => chapter.categoryId === nearbyUi.categoryId)
      .map((chapter) => ({
        ...chapter,
        places: chapter.places.filter((place) =>
          visiblePlaceIds.has(place.feature_id ?? place.name)
        ),
      }))
      .filter((chapter) => chapter.places.length > 0);
    if (chapters.length === 0) return;
    dispatchNearbyUi({
      type: "start_tour",
      placeId: selectedPlaceId
        ?? (chapters[0].places[0]?.feature_id ?? chapters[0].places[0]?.name ?? null),
    });
    setNearbyTourRequest((current) => ({id: (current?.id ?? 0) + 1, chapters}));
  };
  const applyNearbyTourScene = (scene: NearbyTourSceneState) => {
    setView(scene.view);
    setCameraMode('evidence');
    setNearbyTourDepth(scene.depth);
    dispatchNearbyUi({
      type: "show_tour_place",
      categoryId: scene.view === "nearby"
        ? `nearby:${scene.layerId ?? ""}`
        : "metro",
      placeId: scene.selectedPlaceId,
    });
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
            if (id) selectPlace(id);
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
            : activeView === "society"
              ? cancelSocietyPlayback
              : undefined}
          onUserCameraGesture={() => {
            if (nearbyUi.mode !== "rest") {
              dispatchNearbyUi({ type: "user_gesture" });
            }
          }}
          onToggleExpanded={() => setExpanded((current) => !current)}
          showExpandAction={presentation !== "approach"}
          homeFirst={presentation === "atlas"}
          anchorNearbyOnHome={presentation === "atlas"}
          safeFrameRightPx={presentation === "atlas"
            ? atlasPolicy.stageLayout.cameraRailClearancePx
            : undefined}
          suspendNearbyCamera={nearbyUi.cameraOwner === "user"}
          nearbyCameraVersion={nearbyUi.cameraResetVersion}
          nearbyTourRequest={nearbyTourRequest}
          onNearbyTourScene={applyNearbyTourScene}
        />
      </Suspense>
    </ArrivalMapBoundary>
  );

  if (presentation === "atlas") {
    const directionsUrl = selectedPlace
      ? atlasDirectionsUrl(selectedPlace)
      : null;
    const nearbyLayerActive = nearbyUi.mode !== "rest"
      && (activeView === "metro" || activeView === "nearby");
    const canTourNearby = (activeView === "metro" || activeView === "nearby")
      && visiblePlaces.length > 0;
    const nearbyTourAction = playbackState === "playing"
      ? "Pause nearby tour"
      : playbackState === "paused"
        ? "Resume nearby tour"
        : "Tour nearby places";
    const visibleAtlasNearbyCategories = atlasNearbyCategories;
    const activeNearbyCategory = visibleAtlasNearbyCategories.find(
      (category) => category.id === nearbyUi.categoryId,
    );
    const activeLayerHasGeometry = activeView === "metro"
      ? metroLines.length > 0
      : activeView === "nearby" && currentNearbyLayer
        ? (context.layer_lines?.[currentNearbyLayer.id]?.length ?? 0) > 0
          || (context.layer_polygons?.[currentNearbyLayer.id]?.length ?? 0) > 0
        : false;

    return (
      <section
        className="property-arrival-map property-arrival-map--atlas nearby-plate"
        aria-label={`Explore ${context.home.name}`}
        data-nearby-mode={nearbyUi.mode}
        data-playback-state={playbackState}
      >
        <div className="nearby-plate__story-layout property-atlas__story-layout">
          <aside className="nearby-plate__story-rail property-atlas__layer-rail">
            <div className="nearby-plate__layers" role="toolbar" aria-label="Map layers">
              {visibleAtlasNearbyCategories.map((category) => {
                const active = nearbyLayerActive
                  && nearbyUi.categoryId === category.id;
                const iconKind = "layerId" in category
                  ? category.layerId
                  : category.id;
                return (
                  <button
                    ref={(node) => {
                      if (node) layerButtonRefs.current.set(category.id, node);
                      else layerButtonRefs.current.delete(category.id);
                    }}
                    key={category.id}
                    type="button"
                    className={`nearby-plate__chip nearby-plate__chip--${iconKind}${active ? " is-active" : ""}`}
                    aria-label={category.label}
                    aria-pressed={active}
                    onClick={() => {
                      if (active) {
                        closeAtlasNearby();
                        return;
                      }
                      selectAtlasCategory(category);
                    }}
                  >
                    <SoftNearbyIcon kind={iconKind} />
                    <span>{category.label}</span>
                  </button>
                );
              })}
            </div>

            <div
              className="property-atlas__scope-switch"
              role="group"
              aria-label="Nearby radius"
            >
              {atlasPolicy.nearby.scope.options.map((option) => (
                <button
                  key={option.id}
                  type="button"
                  className={nearbyLayerActive && nearbyScope.id === option.id
                    ? "is-active"
                    : undefined}
                  aria-pressed={nearbyLayerActive && nearbyScope.id === option.id}
                  disabled={!nearbyLayerActive}
                  onClick={() => changeNearbyScope(option.id)}
                >
                  {option.label}
                </button>
              ))}
            </div>
          </aside>

          <div className="nearby-plate__body">
            <div className="nearby-plate__canvas property-atlas__map-frame">
            <div className="property-atlas__shade" aria-hidden="true" />
            {mapSurface}

            {nearbyLayerActive
              && visiblePlaces.length === 0
              && !activeLayerHasGeometry ? (
              <p className="property-atlas__empty-layer" role="status">
                No {activeNearbyCategory?.label.toLowerCase() ?? "places"} within{" "}
                {nearbyScope.label}.
              </p>
            ) : null}

            {selectedPlace ? (
              <aside className="property-atlas__selection-card" aria-label="Selected place">
                <button
                  type="button"
                  aria-label="Close selected place"
                  onClick={clearSelectedPlace}
                >
                  ×
                </button>
                <strong>{selectedPlace.name}</strong>
                <small>
                  {[
                    typeof selectedPlace.distance_km === "number"
                      ? `${selectedPlace.distance_km.toFixed(1)} km straight-line`
                      : null,
                    atlasPlaceMeta(selectedPlace),
                  ].filter(Boolean).join(" · ")}
                </small>
                <div className="property-atlas__selection-actions">
                  {directionsUrl ? (
                    <a href={directionsUrl} target="_blank" rel="noreferrer">
                      Directions <span aria-hidden="true">↗</span>
                    </a>
                  ) : null}
                  {selectedPlace.source_url ? (
                    <a
                      href={selectedPlace.source_url}
                      target="_blank"
                      rel="noreferrer"
                    >
                      Source <span aria-hidden="true">↗</span>
                    </a>
                  ) : null}
                  {propertyId ? (
                    <button
                      type="button"
                      aria-pressed={selectedPlaceNoted}
                      onClick={rememberSelectedPlace}
                    >
                      {selectedPlaceNoted ? "Noted" : "Add note"}
                    </button>
                  ) : null}
                </div>
              </aside>
            ) : null}

            <nav className="property-atlas__camera-rail" aria-label="Map controls">
              <button
                type="button"
                aria-label="Top view"
                title="Top view"
                aria-pressed={above}
                disabled={activeView === "approach"}
                onClick={() => {
                  playbackController.cancel("settled");
                  setSocietyAutoPlay(false);
                  setAbove((current) => !current);
                }}
              >
                <span aria-hidden="true">Top</span>
              </button>
              {canTourNearby ? (
                <>
                  <i aria-hidden="true" />
                  <button
                    type="button"
                    aria-label={nearbyTourAction}
                    title={nearbyTourAction}
                    aria-pressed={playbackState === "playing"}
                    onClick={tourPlaces}
                  >
                    <span aria-hidden="true">
                      {playbackState === "playing" ? "Ⅱ" : "▶"}
                    </span>
                  </button>
                </>
              ) : null}
              {nearbyUi.cameraOwner === "user" ? (
                <>
                  <i aria-hidden="true" />
                  <button
                    type="button"
                    aria-label="Reset view"
                    title="Reset view"
                    onClick={() => dispatchNearbyUi({ type: "reset_camera" })}
                  >
                    <span aria-hidden="true">↺</span>
                  </button>
                </>
              ) : null}
            </nav>

            {missingArrivalState ? (
              <p className="property-atlas__status" role="status" aria-live="polite">
                {missingArrivalState}
              </p>
            ) : null}
            </div>
          </div>
        </div>
      </section>
    );
  }

  return (
    <div className={`property-arrival-map${presentation === "approach" ? " property-arrival-map--approach" : ""}`}>
      {presentation !== "approach" ? (
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
      ) : null}
      {activeView === 'nearby' && <label className="atlas-nearby-category">
        <span className="sr-only">Nearby category</span>
        <select aria-label="Nearby category" value={currentNearbyLayer?.id ?? ''} onChange={e => {
          playbackController.cancel("settled");
          dispatchNearbyUi({
            type: "change_category",
            categoryId: `nearby:${e.target.value}`,
          });
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
        <button type="button" aria-pressed={!selectedPlaceId} onClick={() =>
          dispatchNearbyUi({
            type: "change_category",
            categoryId: nearbyUi.categoryId
              ?? (activeView === "metro"
                ? "metro"
                : `nearby:${currentNearbyLayer?.id ?? ""}`),
          })
        }>Show all</button>
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
