import { Fragment, useCallback, useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import { Link } from "react-router-dom";
import { propertyDetailPath } from "../lib/api.ts";
import { ImageWithFallback } from "./ImageWithFallback.tsx";
import { SaveHeartButton } from "./SaveHeartButton.tsx";
import { RailPageControls } from "./RailPageControls.tsx";
import { useFittedRailPage } from "../hooks/useFittedRailPage.ts";
import {
  orderedLandingSearchResults,
  partitionLandingResultSet,
} from "../lib/landing-search-rails.ts";
import { primaryProofFocus } from "../lib/proof-focus.ts";
import { searchResultReasonLabels } from "../lib/search.ts";
import { hrefWithSearchSpan, type SearchSpanReference } from "../lib/navigationContext.ts";
import { formatListingPrice } from "../lib/listing-price.ts";
import type { BrowsePropertyCard, DiscoveryProductStory, DiscoveryResponse, JourneyCollection, ProofFocus, SearchResponse, SearchResultItem } from "../lib/types.ts";

type LandingStoryStageProps = {
  discovery?: DiscoveryResponse | null;
  searchQuery?: string;
  searchResponse?: SearchResponse | null;
  onSearchReady?: (resultCount?: number) => void;
};


function LandingPagedRail<T extends { id: string }>({
  label,
  controlsLabel,
  items,
  plusAfterCount = 0,
  renderCard,
}: {
  label?: string;
  controlsLabel: string;
  items: T[];
  plusAfterCount?: number;
  renderCard: (item: T) => ReactNode;
}) {
  const scrollerRef = useRef<HTMLDivElement | null>(null);
  const [activeId, setActiveId] = useState<string | null>(null);
  const { viewportRef, pageSize, pageCount, leadingIndex, canPrevious, canNext } = useFittedRailPage(items.length);
  const lastPageStart = Math.max(0, items.length - pageSize);
  const safeLeadingIndex = Math.min(leadingIndex, lastPageStart);
  const singlePage = pageCount === 1;
  const columnCount = singlePage ? items.length : pageSize;
  const showHead = Boolean(label) || pageCount > 1;

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

  if (items.length === 0) return null;

  return (
    <div
      className="landing-featured__rail"
      style={{ "--landing-rail-cols": String(columnCount) } as React.CSSProperties}
    >
      {showHead ? (
        <div className="landing-featured__rail-head">
          {label ? <h2 className="landing-featured__rail-label">{label}</h2> : null}
          <RailPageControls
            canPrevious={canPrevious}
            canNext={canNext}
            rangeStart={safeLeadingIndex + 1}
            rangeEnd={Math.min(safeLeadingIndex + pageSize, items.length)}
            total={items.length}
            onPrevious={() => {
              const next = Math.max(0, safeLeadingIndex - pageSize);
              scrollToIndex(next);
            }}
            onNext={() => {
              const next = Math.min(lastPageStart, safeLeadingIndex + pageSize);
              scrollToIndex(next);
            }}
            label={controlsLabel}
          />
        </div>
      ) : null}
      <div
        ref={setScroller}
        className={`landing-stage__featured landing-stage__featured--scroll${activeId ? " has-active-card" : ""}`}
        role="region"
        aria-label={label ? `${label} homes` : "Homes"}
        tabIndex={pageCount > 1 ? 0 : -1}
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
              >
                {renderCard(item)}
              </div>
            </Fragment>
          );
        })}

      </div>
    </div>
  );
}

function LandingResultRail({
  results,
  siblings = [],
  label,
  discoveryContextId,
  discoveryQueryFingerprint,
}: {
  results: SearchResultItem[];
  siblings?: SearchResultItem[];
  label?: string;
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
      renderCard={(item) => {
        const result = item as SearchResultItem;
        const labels = searchResultReasonLabels(result);
        const proofFocus = primaryProofFocus(result);
        return (
          <BrowsePropertyTile
            property={result}
            matchLabels={labels.slice(0, 1)}
            proofFocus={proofFocus}
            searchSpan={discoveryContextId && discoveryQueryFingerprint ? {
              id: discoveryContextId, queryFingerprint: discoveryQueryFingerprint,
            } : undefined}
          />
        );
      }}
    />
  );
}

function LandingSearchResults({
  query,
  response,
  onReady,
}: {
  query: string;
  response?: SearchResponse | null;
  onReady?: (resultCount?: number) => void;
}) {
  useEffect(() => {
    if (!response) return;
    onReady?.(response?.totalMatches);
  }, [onReady, response]);

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

  const resultSets = response.resultSets.filter((set) => set.results.length > 0);
  if (resultSets.length === 0) {
    return <p className="landing-featured__empty">No homes matched. Try a broader sentence.</p>;
  }

  return (
    <div key={query} className="landing-featured__results">
      {resultSets.map((set) => {
        const partition = partitionLandingResultSet(set.results);
        return (
          <section key={set.branchId} className="landing-journey-section" data-landing-reveal>
            <LandingResultRail
              results={partition.exact}
              siblings={partition.siblings}
              label={set.label}
              discoveryContextId={response.navigationContext?.id ?? null}
              discoveryQueryFingerprint={response.navigationContext?.queryFingerprint ?? null}
            />
          </section>
        );
      })}
    </div>
  );
}

function BrowsePropertyTile({
  property,
  searchSpan,
  showArea = true,
  matchLabels = [],
  proofFocus,
}: {
  property: BrowsePropertyCard;
  searchSpan?: SearchSpanReference;
  showArea?: boolean;
  matchLabels?: string[];
  proofFocus?: ProofFocus;
}) {
  const meta = [
    showArea ? property.area : null,
    property.bhk !== undefined && property.bhk > 0 ? `${property.bhk} BHK` : null,
    property.sqft !== undefined && property.sqft > 0 ? `${property.sqft.toLocaleString("en-IN")} sqft` : null,
  ].filter(Boolean).join(" · ");
  const href = hrefWithSearchSpan(proofFocus ? propertyDetailPath(property.id, proofFocus) : property.detail_href, searchSpan ? {
    ...searchSpan,
    selectedId: property.id,
  } : undefined);
  return (
    <article className="catalog-card catalog-card--browse">
      <div className="catalog-card__media">
        <ImageWithFallback src={property.image} alt="" className="catalog-card__image"
          loading="lazy" fetchPriority="low" />
        <div className="catalog-card__actions" role="group" aria-label="Property actions">
          <SaveHeartButton propertyId={property.save_id} propertyName={property.society_name || property.title}
            className="catalog-card__action catalog-card__save" />
        </div>
      </div>
      <Link to={href} className="catalog-card__link">
        <div className="catalog-card__caption">
          <h3 className="catalog-card__title">{property.society_name || property.title}</h3>
          {meta ? <p className="catalog-card__meta">{meta}</p> : null}
          <div className="catalog-card__foot">
            <span className="catalog-card__price">{formatListingPrice(property)}</span>
            {property.google_rating ? (
              <span className="catalog-card__rating">Google {property.google_rating.toFixed(1)}</span>
            ) : null}
          </div>
          {matchLabels.length > 0 ? <div className="catalog-card__signals" aria-label="Search match">
            {matchLabels.map((label) => <span key={label} className="catalog-card__signal">{label}</span>)}
          </div> : null}
        </div>
      </Link>
    </article>
  );
}

function ContextualCollection({
  collection,
  searchSpan,
}: {
  collection: JourneyCollection;
  searchSpan?: SearchSpanReference;
}) {
  return (
    <section className="landing-catalog__shelf" aria-labelledby={`landing-collection-${collection.id}`} data-landing-reveal>
      <h2 id={`landing-collection-${collection.id}`}>{collection.title}</h2>
      <LandingPagedRail controlsLabel={`${collection.title} pages`} items={collection.cards}
        renderCard={(property) => <BrowsePropertyTile property={property} searchSpan={searchSpan} />} />
    </section>
  );
}

function LandingCatalog({
  discovery,
  searchQuery,
  searchResponse,
  onSearchReady,
}: {
  discovery?: DiscoveryResponse | null;
  searchQuery?: string;
  searchResponse?: SearchResponse | null;
  onSearchReady?: (resultCount?: number) => void;
}) {
  const searching = Boolean(searchQuery?.trim());
  const shelves = searching ? [] : discovery?.shelves.filter((shelf) => shelf.cards.length > 0) ?? [];
  const collections = searching ? searchResponse?.journey?.active.collections ?? [] : [];
  if (!searching && shelves.length === 0) return null;

  return (
    <section
      className={`landing-featured${searching ? "" : " landing-featured--catalog"}`}
      aria-label={searching ? "Matching homes" : "Explore homes"}
    >
      <div className="landing-catalog">
        {searching && searchQuery ? (
          <LandingSearchResults
            key={searchResponse?.journey?.active.revision.stateToken ?? searchQuery.trim()}
            query={searchQuery.trim()}
            response={searchResponse}
            onReady={onSearchReady}
          />
        ) : null}
        {collections.map((collection) => (
          <ContextualCollection key={`${searchResponse?.journey?.active.revision.id}:${collection.id}`} collection={collection}
            searchSpan={searchResponse?.navigationContext} />
        ))}
        {shelves.map((shelf) => (
            <section key={shelf.id} className="landing-catalog__shelf" aria-labelledby={`landing-shelf-${shelf.id}`} data-landing-reveal>
              <h2 id={`landing-shelf-${shelf.id}`}>{shelf.title}</h2>
              <LandingPagedRail
                controlsLabel={`${shelf.title} pages`}
                items={shelf.cards.map((card) => card.property)}
                renderCard={(property) => (
                  <BrowsePropertyTile property={property} showArea={shelf.show_card_area} />
                )}
              />
            </section>
        ))}
      </div>
    </section>
  );
}

function LandingProductStory({
  story,
  featuredPropertyId,
  searchSpan,
}: {
  story: DiscoveryProductStory;
  featuredPropertyId?: string;
  searchSpan?: SearchSpanReference;
}) {
  const items = story.items.flatMap((item) => {
    const selectsProperty = item.href.includes("{property_id}");
    if (selectsProperty && !featuredPropertyId) return [];
    const href = item.href.replace(
      "{property_id}",
      encodeURIComponent(featuredPropertyId ?? ""),
    );
    return [{
      ...item,
      href: hrefWithSearchSpan(href, searchSpan ? {
        ...searchSpan,
        selectedId: selectsProperty ? featuredPropertyId : searchSpan.selectedId,
      } : undefined),
    }];
  });
  if (items.length === 0) return null;

  return (
    <section className="landing-product-story" aria-labelledby="landing-about-title">
      <div className="landing-product-story__marker" data-landing-reveal>
        <h2 id="landing-about-title">About us</h2>
      </div>
      <div className="landing-product-story__rows">
        {items.map((item, index) => (
          <article
            key={item.id}
            className={`landing-product-story__row${index % 2 === 1 ? " landing-product-story__row--reverse" : ""}`}
            data-landing-reveal
            style={{ "--landing-reveal-delay": `${Math.min(index, 2) * 45}ms` } as React.CSSProperties}
          >
            <div className="landing-product-story__visual">
              <Link to={item.href} className="landing-product-story__media" aria-label={item.action_label}>
                <img
                  src={item.image_src}
                  srcSet={`${item.image_src_narrow} 960w, ${item.image_src} 1600w`}
                  sizes="(max-width: 720px) 100vw, (max-width: 1100px) 62vw, 820px"
                  width="960"
                  height="600"
                  alt={item.image_alt}
                  loading="lazy"
                  decoding="async"
                />
              </Link>
            </div>
            <div className="landing-product-story__copy">
              <h3>{item.title}</h3>
              <p>{item.description}</p>
              <Link to={item.href} className="landing-product-story__action">
                {item.action_label} <span aria-hidden="true">→</span>
              </Link>
            </div>
          </article>
        ))}
      </div>
    </section>
  );
}

function useLandingRevealObserver(contentKey: string) {
  const rootRef = useRef<HTMLElement | null>(null);
  useEffect(() => {
    const root = rootRef.current;
    if (!root) return undefined;
    const targets = [...root.querySelectorAll<HTMLElement>("[data-landing-reveal]")];
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
      targets.forEach((target) => target.classList.add("is-revealed"));
      return undefined;
    }
    const observer = new IntersectionObserver((entries) => {
      for (const entry of entries) {
        if (!entry.isIntersecting) continue;
        entry.target.classList.add("is-revealed");
        observer.unobserve(entry.target);
      }
    }, { rootMargin: "0px 0px -8%", threshold: 0.08 });
    targets.forEach((target) => observer.observe(target));
    return () => observer.disconnect();
  }, [contentKey]);
  return rootRef;
}

export function LandingStoryStage({
  discovery,
  searchQuery,
  searchResponse,
  onSearchReady,
}: LandingStoryStageProps) {
  const featuredPropertyId = searchResponse
    ? orderedLandingSearchResults(searchResponse)[0]?.id ?? searchResponse.journey?.active.collections[0]?.cards[0]?.id
    : discovery?.shelves.flatMap((shelf) => shelf.cards)[0]?.property.id;
  const searching = Boolean(searchQuery?.trim());
  const revealKey = [
    searchResponse?.journey?.active.revision.stateToken ?? searchQuery?.trim() ?? "",
    discovery?.shelves.map((shelf) => shelf.id).join("\u0000") ?? "",
    discovery?.product_story?.title ?? "",
  ].join("\u0001");
  const revealRef = useLandingRevealObserver(revealKey);

  return (
    <section ref={revealRef} className="landing-stage" aria-label={searching ? "Your search journey" : "Explore homes"}>
      <LandingCatalog discovery={discovery} searchQuery={searchQuery}
        searchResponse={searchResponse} onSearchReady={onSearchReady} />
      {discovery?.product_story ? (
        <LandingProductStory
          story={discovery.product_story}
          featuredPropertyId={featuredPropertyId}
          searchSpan={searchResponse?.navigationContext}
        />
      ) : null}
    </section>
  );
}
