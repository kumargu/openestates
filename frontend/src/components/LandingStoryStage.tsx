import { Fragment, useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { CSSProperties, FocusEvent, ReactNode } from "react";
import { Link } from "react-router-dom";
import { LivingEvidenceTile } from "./evidence/LivingEvidenceTile.tsx";
import { RailPageControls } from "./RailPageControls.tsx";
import { useFittedRailPage } from "../hooks/useFittedRailPage.ts";
import { propertyDetailPath, searchProperties } from "../lib/api.ts";
import { PUBLIC_BRAND_NAME } from "../lib/brand.ts";
import {
  composeLandingSearchRails,
  orderedLandingSearchResults,
} from "../lib/landing-search-rails.ts";
import {
  writeSearchJourneyContext,
} from "../lib/navigationContext.ts";
import { primaryProofFocus } from "../lib/proof-focus.ts";
import { friendlyMatchLabel, searchResultReasonLabels } from "../lib/search.ts";
import type { PropertyCard, SearchResponse, SearchResultItem } from "../lib/types.ts";
import {
  LANDING_RESOLVE_QUERY,
  LANDING_STORY_CHAPTERS,
  LANDING_STORY_SCENE_IDS,
  type LandingStoryChapter,
  type LandingStorySceneId,
} from "../lib/landing-story.ts";
import { filterListableProperties, uniqueSocietiesForDiscovery } from "../lib/property-filters.ts";
import { useLandingSceneController } from "../hooks/useLandingSceneController.ts";
import { useLandingStoryMotion } from "../hooks/useLandingStoryMotion.ts";

const FEATURED_LIMIT = 6;
const ACTIVE_CARD_GROWTH_REM = 6;

type FeaturedLensId = "metro" | "family" | "township" | "feedback";

type FeaturedLens = {
  id: FeaturedLensId;
  label: string;
  query: string;
};

const FEATURED_LENSES: FeaturedLens[] = [
  { id: "metro", label: "Near metro", query: "Homes near metro with low commute pain" },
  { id: "family", label: "Family-friendly", query: "Family-friendly 3BHK near good schools" },
  { id: "township", label: "Large townships", query: "Large townships with generous open space" },
  { id: "feedback", label: "Resident feedback", query: "Homes with strong resident feedback" },
];

type LandingStoryStageProps = {
  properties: PropertyCard[];
  onSearch: (query: string) => void;
  searchQuery?: string;
  onSearchReady?: (resultCount?: number) => void;
};

function useDesktopStory(): boolean {
  const [isDesktop, setIsDesktop] = useState(
    () => typeof window !== "undefined" && window.matchMedia("(min-width: 901px)").matches,
  );

  useEffect(() => {
    const media = window.matchMedia("(min-width: 901px)");
    const handleChange = () => setIsDesktop(media.matches);
    media.addEventListener("change", handleChange);
    return () => media.removeEventListener("change", handleChange);
  }, []);

  return isDesktop;
}

function hasKnownNumber(value: number | null | undefined): value is number {
  return typeof value === "number" && Number.isFinite(value) && value > 0;
}

function isKnownText(value: string | null | undefined): value is string {
  if (!value) return false;
  const normalized = value.trim().toLowerCase();
  return normalized.length > 0
    && normalized !== "unknown"
    && normalized !== "not specified"
    && normalized !== "n/a";
}

function rankHomesForLens(properties: PropertyCard[], lensId: FeaturedLensId): PropertyCard[] {
  const homes = uniqueSocietiesForDiscovery(properties);
  const stableIndex = new Map(homes.map((home, index) => [home.id, index]));

  return [...homes].sort((left, right) => {
    let difference = 0;
    if (lensId === "metro") {
      const leftDistance = hasKnownNumber(left.metro_distance_mins) ? left.metro_distance_mins : Number.POSITIVE_INFINITY;
      const rightDistance = hasKnownNumber(right.metro_distance_mins) ? right.metro_distance_mins : Number.POSITIVE_INFINITY;
      difference = leftDistance - rightDistance;
    } else if (lensId === "family") {
      const familyScore = (home: PropertyCard) => (
        (home.bhk === 3 ? 4 : home.bhk > 3 ? 2 : 0)
        + (hasKnownNumber(home.google_rating) && home.google_rating >= 4 ? 1 : 0)
      );
      difference = familyScore(right) - familyScore(left)
        || (hasKnownNumber(left.price) ? left.price : Number.POSITIVE_INFINITY)
          - (hasKnownNumber(right.price) ? right.price : Number.POSITIVE_INFINITY);
    } else if (lensId === "township") {
      difference = (right.society_land_acres ?? 0) - (left.society_land_acres ?? 0)
        || (right.open_space_pct ?? 0) - (left.open_space_pct ?? 0);
    } else {
      difference = (right.google_rating ?? 0) - (left.google_rating ?? 0)
        || (right.google_review_count ?? 0) - (left.google_review_count ?? 0);
    }

    return difference || (stableIndex.get(left.id) ?? 0) - (stableIndex.get(right.id) ?? 0);
  });
}

function matchLabels(property: PropertyCard, lensId: FeaturedLensId): string[] {
  const labels: string[] = [];

  if (lensId === "metro" && hasKnownNumber(property.metro_distance_mins)) {
    labels.push(`${property.metro_distance_mins} min metro`);
  }
  if (lensId === "family") {
    if (hasKnownNumber(property.open_space_pct)) labels.push(`${Math.round(property.open_space_pct)}% open space`);
    if (isKnownText(property.home_state_display)) labels.push(property.home_state_display);
  }
  if (lensId === "township") {
    if (hasKnownNumber(property.society_land_acres)) labels.push(`${Math.round(property.society_land_acres)} acres`);
    if (hasKnownNumber(property.open_space_pct)) labels.push(`${Math.round(property.open_space_pct)}% open space`);
  }
  return labels.slice(0, 2);
}

function LandingPagedRail({
  label,
  controlsLabel,
  items,
  plusAfterCount = 0,
  spatial = false,
  renderCard,
}: {
  label?: string;
  controlsLabel: string;
  items: PropertyCard[];
  plusAfterCount?: number;
  spatial?: boolean;
  renderCard: (item: PropertyCard, active: boolean) => ReactNode;
}) {
  const scrollerRef = useRef<HTMLDivElement | null>(null);
  const scrollTimerRef = useRef<number | null>(null);
  const pendingTouchPreviewRef = useRef<{
    id: string;
    startX: number;
    startY: number;
  } | null>(null);
  const [leadingIndex, setLeadingIndex] = useState(0);
  const [activeId, setActiveId] = useState<string | null>(null);
  const { viewportRef, pageSize, pageCount } = useFittedRailPage(items.length);
  const lastPageStart = Math.max(0, (pageCount - 1) * pageSize);
  const safeLeadingIndex = Math.min(leadingIndex, lastPageStart);
  const trailingSlots = pageCount * pageSize - items.length;
  const showHead = Boolean(label) || pageCount > 1;
  const activeCardGrowthRem = pageSize > 1 ? ACTIVE_CARD_GROWTH_REM : 0;
  const inactiveCardShrinkRem = pageSize > 1
    ? activeCardGrowthRem / (pageSize - 1)
    : 0;

  const setScroller = useCallback((node: HTMLDivElement | null) => {
    scrollerRef.current = node;
    viewportRef(node);
  }, [viewportRef]);

  const scrollToIndex = useCallback((index: number, behavior: ScrollBehavior = "smooth") => {
    const scroller = scrollerRef.current;
    const target = scroller?.querySelector<HTMLElement>(`[data-rail-item-index="${index}"]`);
    if (!scroller || !target) return;

    const left = target.getBoundingClientRect().left
      - scroller.getBoundingClientRect().left
      + scroller.scrollLeft;
    const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    scroller.scrollTo({ left, behavior: reduceMotion ? "auto" : behavior });
  }, []);

  const syncLeadingItem = useCallback(() => {
    const scroller = scrollerRef.current;
    if (!scroller) return;

    const scrollerLeft = scroller.getBoundingClientRect().left;
    let closestIndex = 0;
    let closestDistance = Number.POSITIVE_INFINITY;
    scroller.querySelectorAll<HTMLElement>("[data-rail-item-index]").forEach((card) => {
      const distance = Math.abs(card.getBoundingClientRect().left - scrollerLeft);
      if (distance < closestDistance) {
        closestDistance = distance;
        closestIndex = Number(card.dataset.railItemIndex ?? 0);
      }
    });
    setLeadingIndex(Math.min(closestIndex, lastPageStart));
  }, [lastPageStart]);

  const handleScroll = useCallback(() => {
    if (scrollTimerRef.current !== null) window.clearTimeout(scrollTimerRef.current);
    scrollTimerRef.current = window.setTimeout(() => {
      scrollTimerRef.current = null;
      syncLeadingItem();
    }, 80);
  }, [syncLeadingItem]);

  useEffect(() => () => {
    if (scrollTimerRef.current !== null) window.clearTimeout(scrollTimerRef.current);
  }, []);

  if (items.length === 0) return null;

  return (
    <div
      className={`landing-featured__rail${spatial ? " landing-featured__rail--spatial" : ""}`}
      style={{
        "--landing-rail-cols": String(pageSize),
        "--landing-active-card-growth": `${activeCardGrowthRem}rem`,
        "--landing-inactive-card-shrink": `${inactiveCardShrinkRem}rem`,
      } as CSSProperties}
    >
      {showHead ? (
        <div className="landing-featured__rail-head">
          {label ? <p className="landing-featured__rail-label">{label}</p> : null}
          <RailPageControls
            canPrevious={safeLeadingIndex > 0}
            canNext={safeLeadingIndex < lastPageStart}
            rangeStart={safeLeadingIndex + 1}
            rangeEnd={Math.min(safeLeadingIndex + pageSize, items.length)}
            total={items.length}
            onPrevious={() => scrollToIndex(Math.max(0, safeLeadingIndex - pageSize))}
            onNext={() => scrollToIndex(Math.min(lastPageStart, safeLeadingIndex + pageSize))}
            label={controlsLabel}
          />
        </div>
      ) : null}
      <div
        ref={setScroller}
        className={[
          "landing-stage__featured",
          "landing-stage__featured--scroll",
          spatial ? "landing-stage__featured--spatial" : "",
          activeId ? "has-active-card" : "",
        ].filter(Boolean).join(" ")}
        role="region"
        aria-label={label ? `${label} homes` : "Matching homes"}
        tabIndex={pageCount > 1 ? 0 : -1}
        onScroll={handleScroll}
        onPointerLeave={(event) => {
          if (event.pointerType !== "touch") setActiveId(null);
        }}
        onBlur={(event) => {
          if (!event.currentTarget.contains(event.relatedTarget)) setActiveId(null);
        }}
        onKeyDown={(event) => {
          if (event.key === "Escape") setActiveId(null);
        }}
      >
        {items.map((item, itemIndex) => {
          const active = activeId === item.id;
          return (
            <Fragment key={item.id}>
              {plusAfterCount > 0 && itemIndex === plusAfterCount ? (
                <div
                  className="landing-featured__plus"
                  role="separator"
                  aria-label="Also available at this project"
                >
                  <span aria-hidden="true">+</span>
                </div>
              ) : null}
              <div
                className={`landing-stage__feature-card${active ? " is-active" : ""}`}
                data-rail-item-index={itemIndex}
                onPointerEnter={(event) => {
                  if (event.pointerType !== "touch") setActiveId(item.id);
                }}
                onFocusCapture={() => setActiveId(item.id)}
                onPointerDownCapture={(event) => {
                  if (event.pointerType !== "touch" || active) return;
                  if (event.target instanceof Element && event.target.closest("button")) return;
                  pendingTouchPreviewRef.current = {
                    id: item.id,
                    startX: event.clientX,
                    startY: event.clientY,
                  };
                }}
                onPointerMoveCapture={(event) => {
                  const pending = pendingTouchPreviewRef.current;
                  if (!pending || pending.id !== item.id) return;
                  if (
                    Math.hypot(
                      event.clientX - pending.startX,
                      event.clientY - pending.startY,
                    ) > 8
                  ) {
                    pendingTouchPreviewRef.current = null;
                  }
                }}
                onPointerCancelCapture={() => {
                  if (pendingTouchPreviewRef.current?.id === item.id) {
                    pendingTouchPreviewRef.current = null;
                  }
                }}
                onClickCapture={(event) => {
                  if (pendingTouchPreviewRef.current?.id !== item.id) return;
                  pendingTouchPreviewRef.current = null;
                  event.preventDefault();
                  setActiveId(item.id);
                  const card = event.currentTarget;
                  window.requestAnimationFrame(() => {
                    const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
                    card.scrollIntoView({
                      behavior: reduceMotion ? "auto" : "smooth",
                      block: "start",
                      inline: "nearest",
                    });
                  });
                }}
              >
                {renderCard(item, active)}
              </div>
            </Fragment>
          );
        })}
        {Array.from({ length: trailingSlots }, (_, index) => (
          <span
            key={`trailing-slot-${index}`}
            className="landing-stage__feature-card landing-stage__feature-card--spacer"
            aria-hidden="true"
          />
        ))}
      </div>
    </div>
  );
}

function LandingResultRail({
  results,
  siblings = [],
  label,
  query,
  discoveryContextId,
  discoveryQueryFingerprint,
}: {
  results: SearchResultItem[];
  siblings?: SearchResultItem[];
  label?: string;
  query?: string;
  discoveryContextId: string | null;
  discoveryQueryFingerprint: string | null;
}) {
  const items = [...results, ...siblings];
  if (items.length === 0) return null;

  return (
    <LandingPagedRail
      label={label}
      controlsLabel={label ? `${label} pages` : "Matching homes pages"}
      items={items}
      plusAfterCount={siblings.length > 0 ? results.length : 0}
      spatial
      renderCard={(item, active) => {
        const result = item as SearchResultItem;
        const labels = searchResultReasonLabels(result);
        const proofFocus = primaryProofFocus(result, query);
        const expandedSignal = result.tradeoff_label
          ?? labels[1]
          ?? friendlyMatchLabel(result.match_label);
        return (
          <LivingEvidenceTile
            property={result}
            variant="browse"
            matchLabels={labels.slice(0, 1)}
            previewActive={active}
            previewSignals={expandedSignal ? [expandedSignal] : []}
            spatial
            proofFocus={proofFocus}
            discoveryContextId={discoveryContextId}
            discoveryQueryFingerprint={discoveryQueryFingerprint}
            allowSave
          />
        );
      }}
    />
  );
}

function LandingSearchResults({
  query,
  onReady,
}: {
  query: string;
  onReady?: (resultCount?: number) => void;
}) {
  const [searchState, setSearchState] = useState<{
    response: SearchResponse;
    contextId: string | null;
    queryFingerprint: string | null;
  } | null>(null);
  const [failed, setFailed] = useState(false);
  const response = searchState?.response ?? null;

  useEffect(() => {
    const controller = new AbortController();

    searchProperties(query, { signal: controller.signal })
      .then((data) => {
        if (controller.signal.aborted) return;
        const results = orderedLandingSearchResults(data);
        const focusForResult = (result: SearchResultItem) =>
          primaryProofFocus(result, query);
        const searchSpan = writeSearchJourneyContext(
          query,
          `${window.location.pathname}${window.location.search}`,
          results,
          data.runtimeVersion,
          focusForResult,
        );
        setSearchState({
          response: data,
          contextId: searchSpan?.id ?? null,
          queryFingerprint: searchSpan?.queryFingerprint ?? null,
        });
      })
      .catch((error: unknown) => {
        if (error instanceof DOMException && error.name === "AbortError") return;
        if (!controller.signal.aborted) setFailed(true);
      });

    return () => controller.abort();
  }, [query]);

  useEffect(() => {
    if (!response && !failed) return;
    onReady?.(response?.totalMatches);
  }, [failed, onReady, response]);

  if (failed) {
    return <p className="landing-featured__empty">Search did not come through. Try the sentence again.</p>;
  }

  if (!response) {
    return (
      <div className="landing-stage__featured" aria-busy="true" aria-label="Loading matching homes">
        {["one", "two", "three", "four"].map((card) => (
          <span key={card} className="landing-loading__card">
            <span className="landing-loading__image" />
            <span className="landing-loading__line" />
            <span className="landing-loading__line landing-loading__line--short" />
          </span>
        ))}
      </div>
    );
  }

  const rails = composeLandingSearchRails(response);
  if (rails.length === 0) {
    return <p className="landing-featured__empty">No homes matched. Try a broader sentence.</p>;
  }

  return (
    <div key={query} className="landing-featured__results">
      {rails.map((rail) => (
        <LandingResultRail
          key={rail.id}
          results={rail.results}
          siblings={rail.siblings}
          label={rail.label}
          query={query}
          discoveryContextId={searchState?.contextId ?? null}
          discoveryQueryFingerprint={searchState?.queryFingerprint ?? null}
        />
      ))}
    </div>
  );
}

function FeaturedSuggestions({
  properties,
  onSearch,
  searchQuery,
  onSearchReady,
}: {
  properties: PropertyCard[];
  onSearch: (query: string) => void;
  searchQuery?: string;
  onSearchReady?: (resultCount?: number) => void;
}) {
  const [activeLensId, setActiveLensId] = useState<FeaturedLensId>("metro");
  const activeLens = FEATURED_LENSES.find((lens) => lens.id === activeLensId) ?? FEATURED_LENSES[0];
  const suggestions = useMemo(
    () => rankHomesForLens(properties, activeLensId).slice(0, FEATURED_LIMIT),
    [activeLensId, properties],
  );

  const searching = Boolean(searchQuery?.trim());
  if (!searching && suggestions.length === 0) return null;

  return (
    <section
      className="landing-featured"
      aria-labelledby={searching ? undefined : "landing-featured-title"}
      aria-label={searching ? "Matching homes" : undefined}
    >
      {!searching ? (
        <div className="landing-featured__head">
          <h2 id="landing-featured-title">A few homes with reasons</h2>
          <div className="landing-featured__lenses" aria-label="Ways to browse">
            {FEATURED_LENSES.map((lens) => (
              <button
                key={lens.id}
                type="button"
                className={lens.id === activeLensId ? "is-active" : ""}
                aria-pressed={lens.id === activeLensId}
                onClick={() => setActiveLensId(lens.id)}
              >
                {lens.label}
              </button>
            ))}
          </div>
          <button
            type="button"
            className="landing-featured__search"
            onClick={() => onSearch(activeLens.query)}
          >
            See matching homes
          </button>
        </div>
      ) : null}

      {searching && searchQuery ? (
        <LandingSearchResults key={searchQuery.trim()} query={searchQuery.trim()} onReady={onSearchReady} />
      ) : (
        <LandingPagedRail
          key={activeLensId}
          controlsLabel="Featured homes pages"
          items={suggestions}
          renderCard={(property, active) => {
            const labels = matchLabels(property, activeLensId);
            return (
              <LivingEvidenceTile
                property={property}
                variant="browse"
                matchLabels={labels}
                previewActive={active}
                previewSignals={labels.slice(1)}
                allowSave
              />
            );
          }}
        />
      )}
    </section>
  );
}

function StoryTileImage({
  chapter,
  priority = false,
}: {
  chapter: LandingStoryChapter;
  priority?: boolean;
}) {
  if (!chapter.image) return null;

  const sizes = chapter.presentation === "tile"
    ? "(max-width: 900px) calc(100vw - 2rem), min(38rem, 42vw)"
    : "(max-width: 900px) calc(100vw - 2rem), min(1280px, calc(100vw - 2.5rem))";

  return (
    <div className="landing-story-tile">
      <img
        className="landing-story-tile__image"
        src={chapter.image.src}
        srcSet={`${chapter.image.srcNarrow} 960w, ${chapter.image.src} 1600w`}
        sizes={sizes}
        width={chapter.image.width}
        height={chapter.image.height}
        alt={chapter.imageAlt ?? ""}
        loading={priority ? "eager" : "lazy"}
        decoding="async"
        fetchPriority={priority ? "high" : "low"}
      />
    </div>
  );
}

function chapterAction(
  id: LandingStorySceneId,
  onSearch: (query: string) => void,
  featuredHome?: PropertyCard,
): ReactNode {
  if (id === "resolve") {
    return (
      <button type="button" onClick={() => onSearch(LANDING_RESOLVE_QUERY)}>
        Try this search <span aria-hidden="true">→</span>
      </button>
    );
  }
  if (id === "reveal" && featuredHome) {
    return (
      <Link to={propertyDetailPath(featuredHome.id)}>
        Open this home <span aria-hidden="true">→</span>
      </Link>
    );
  }
  if (id === "remember") {
    return (
      <Link to="/workspace">
        Open notebook <span aria-hidden="true">→</span>
      </Link>
    );
  }
  if (id === "record" && featuredHome) {
    return (
      <Link to={`${propertyDetailPath(featuredHome.id)}/rera`}>
        See the record <span aria-hidden="true">→</span>
      </Link>
    );
  }
  if (id === "converge") {
    return (
      <Link to="/workspace/compare">
        Compare homes <span aria-hidden="true">→</span>
      </Link>
    );
  }
  return null;
}

type StorySceneProps = {
  chapter: LandingStoryChapter;
  action: ReactNode;
  controller: ReturnType<typeof useLandingSceneController>;
  groupActive?: boolean;
  priority?: boolean;
};

function StoryScene({
  chapter,
  action,
  controller,
  groupActive = false,
  priority = false,
}: StorySceneProps) {
  const isActive = controller.activeSceneId === chapter.id || groupActive;
  const hasEntered = controller.hasEntered(chapter.id) || groupActive;
  const isPaused = controller.isPaused(chapter.id);

  const handleBlur = (event: FocusEvent<HTMLElement>) => {
    if (!event.currentTarget.contains(event.relatedTarget as Node | null)) {
      controller.resumeScene(chapter.id);
    }
  };

  const sceneClassName = [
    "landing-scene",
    `landing-scene--${chapter.id}`,
    `landing-scene--canvas-${chapter.side}`,
    `landing-scene--${chapter.presentation}`,
    chapter.image ? "" : "landing-scene--copy-only",
    isActive ? "is-active" : "",
    hasEntered ? "has-entered" : "",
    isPaused ? "is-paused" : "",
  ].filter(Boolean).join(" ");

  return (
    <article
      ref={controller.sceneRef(chapter.id)}
      className={sceneClassName}
      data-scene-id={chapter.id}
      onPointerEnter={() => controller.pauseScene(chapter.id)}
      onPointerLeave={() => controller.resumeScene(chapter.id)}
      onFocusCapture={() => controller.pauseScene(chapter.id)}
      onBlurCapture={handleBlur}
    >
      <div className="landing-scene__copy">
        {priority ? <p className="landing-stage__story-note">Example walkthrough</p> : null}
        <h2>
          {chapter.title.split(" ").map((word, index) => (
            <span key={`${word}-${index}`}>{word}</span>
          ))}
        </h2>
        <p>{chapter.description}</p>
        {action ? <div className="landing-scene__action">{action}</div> : null}
      </div>
      {chapter.image ? (
        <div className="landing-scene__canvas-wrap">
          <div className="landing-canvas">
            <StoryTileImage chapter={chapter} priority={priority} />
          </div>
        </div>
      ) : null}
    </article>
  );
}

export function LandingStoryStage({
  properties,
  onSearch,
  searchQuery,
  onSearchReady,
}: LandingStoryStageProps) {
  const listable = filterListableProperties(properties);
  const uniqueHomes = uniqueSocietiesForDiscovery(listable);
  const isDesktopStory = useDesktopStory();
  const controller = useLandingSceneController(LANDING_STORY_SCENE_IDS, isDesktopStory);
  const storyRef = useLandingStoryMotion(controller.isReducedMotion);
  const featuredHome = uniqueHomes[0];
  const middleChaptersActive = isDesktopStory
    && (["reveal", "remember"] as LandingStorySceneId[]).includes(
      (controller.activeSceneId ?? "resolve") as LandingStorySceneId,
    );

  const chapters = LANDING_STORY_CHAPTERS.map((chapter) => ({
    chapter,
    action: chapterAction(chapter.id, onSearch, featuredHome),
  }));
  const opening = chapters.find(({ chapter }) => chapter.id === "resolve");
  const middle = chapters.filter(({ chapter }) => chapter.presentation === "tile");
  const closing = chapters.filter(({ chapter }) => (
    chapter.presentation === "wide" && chapter.id !== "resolve"
  ));

  return (
    <section
      className="landing-stage"
      aria-label={`How ${PUBLIC_BRAND_NAME} works`}
      data-reduced-motion={controller.isReducedMotion ? "true" : "false"}
    >
      <FeaturedSuggestions
        properties={uniqueHomes}
        onSearch={onSearch}
        searchQuery={searchQuery}
        onSearchReady={onSearchReady}
      />

      <div ref={storyRef} className="landing-stage__story">
        {isDesktopStory ? (
          <div className="landing-story__desktop">
            {opening ? (
              <StoryScene
                chapter={opening.chapter}
                action={opening.action}
                controller={controller}
              />
            ) : null}

            <div className="landing-story__chapter-grid">
              {middle.map(({ chapter, action }) => (
                <StoryScene
                  key={chapter.id}
                  chapter={chapter}
                  action={action}
                  controller={controller}
                  groupActive={middleChaptersActive}
                  priority={chapter.id === "reveal"}
                />
              ))}
            </div>

            {closing.map(({ chapter, action }) => (
              <StoryScene
                key={chapter.id}
                chapter={chapter}
                action={action}
                controller={controller}
              />
            ))}
          </div>
        ) : (
          <div className="landing-story__mobile">
            {chapters.map(({ chapter, action }) => (
              <StoryScene
                key={chapter.id}
                chapter={chapter}
                action={action}
                controller={controller}
                priority={chapter.id === "reveal"}
              />
            ))}
          </div>
        )}
      </div>
    </section>
  );
}
