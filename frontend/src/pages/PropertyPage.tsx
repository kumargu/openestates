import { useEffect, useMemo, useState } from "react";
import {
  Link,
  useParams,
  useSearchParams,
} from "react-router-dom";
import { Helmet } from "react-helmet-async";
import type {
  PropertyCard,
  PropertyDetailResponse,
  ProofFocus,
  RecommendationResponse,
  ArrivalSearchSociety,
  SurfaceSceneResponse,
} from "../lib/types.ts";
import {
  getProperty,
  getPropertyRecommendations,
  getPropertySurface,
  getPropertySurfacesBatch,
  parseProofFocusParam,
} from "../lib/api.ts";
import {
  recommendationShelfItems,
  type RecommendationShelfItem,
} from "../lib/recommendations.ts";
import { PageState } from "../components/PageState.tsx";
import { AroundThisHomePlate } from "../components/evidence/AroundThisHomePlate.tsx";
import { ImageWithFallback } from "../components/ImageWithFallback.tsx";
import { NotebookCommentAnchor } from "../components/notebook/NotebookCommentAnchor.tsx";
import { SaveHeartButton } from "../components/SaveHeartButton.tsx";
import { useNotebook } from "../hooks/useNotebook.ts";
import { usePropertySceneImages } from "../hooks/usePropertySceneImages.ts";
import { PUBLIC_BRAND_NAME } from "../lib/brand.ts";
import { PropertyArrivalFilm } from "../components/property/PropertyArrivalFilm.tsx";
import { PropertyReraTeaser } from "../components/property/PropertyReraTeaser.tsx";
import { PropertyReviewsDeck } from "../components/property/PropertyReviewsDeck.tsx";
import {
  PropertySceneCard,
  PropertySceneFacts,
  PropertySceneIdentity,
} from "../components/property/PropertySceneCard.tsx";
import { PropertyShortCompare } from "../components/property/PropertyShortCompare.tsx";
import { propertySceneImageAt } from "../lib/propertyScene.ts";
import {
  projectPropertyStory,
} from "../lib/propertyStory.ts";
import { readShortlistIds } from "../lib/compare.ts";
import { formatGoogleRating } from "../lib/reviewFormatting.ts";
import { hasAroundThisHomePlate } from "../lib/nearbyPlateProjection.ts";
import { propertyMapContextFromSurfaceScene } from "../lib/surfaceSceneProjection.ts";
import {
  propertyExploreHref,
  readDiscoveryMapContext,
} from "../lib/navigationContext.ts";
import { formatListingPrice } from "../lib/listing-price.ts";
import {
  initialPropertySurfaceId,
  propertyProofMatch,
  propertySceneProofFocus,
} from "../lib/proof-focus.ts";
import { backendUrl, publicSiteUrl } from "../lib/runtimeConfig.ts";

function hasKnownNumber(value: number | null | undefined): value is number {
  return typeof value === "number" && Number.isFinite(value) && value > 0;
}

const MAX_EXPLICIT_COMPARISON_CANDIDATES = 4;
const ARRIVAL_STORY_SURFACE_ID = "arrival_story";

function focusedEvidenceSource(data: PropertyDetailResponse, focus: ProofFocus) {
  for (const section of data.evidence?.sections ?? []) {
    const item = section.items.find((candidate) =>
      candidate.key?.toLocaleLowerCase("en-IN")
        === focus.factKey.toLocaleLowerCase("en-IN")
    );
    if (item?.source_url) return item.source_url;
  }
  return undefined;
}

function PropertySearchMatch({
  data,
  focus,
}: {
  data: PropertyDetailResponse;
  focus?: ProofFocus;
}) {
  if (focus?.targetId !== "property-search-match") return null;
  const value = focus.matchedValue?.trim() || focus.matchedLabel?.trim();
  if (!value) return null;
  const sourceUrl = focusedEvidenceSource(data, focus);
  return (
    <section
      id="property-search-match"
      className="property-fact-deck property-search-match"
      aria-label="Matched your search"
      tabIndex={-1}
    >
      <span>Matched your search</span>
      <strong>{value}</strong>
      {sourceUrl && (
        <a href={sourceUrl} target="_blank" rel="noreferrer">Source ↗</a>
      )}
    </section>
  );
}

function propertyToCard(data: PropertyDetailResponse): PropertyCard {
  const { property: p, society } = data;
  return {
    id: p.id,
    kg_entity_refs: data.entity_refs,
    title: p.title,
    area: p.area,
    price: p.price,
    price_min: p.price_min,
    price_max: p.price_max,
    price_per_sqft: p.price_per_sqft,
    bhk: p.bhk,
    sqft: p.super_builtup_sqft || p.carpet_area_sqft || 0,
    carpet_area_sqft: p.carpet_area_sqft,
    super_builtup_sqft: p.super_builtup_sqft,
    society_name: society?.name ?? "",
    builder_name: p.builder_name,
    images: p.images,
    hero_image: p.hero_image,
    transparency_tags: p.transparency_tags,
    description_summary: p.description_summary,
    possession_status: p.possession_status,
    metro_distance_mins: p.metro_distance_mins,
    floor: p.floor,
    total_floors: p.total_floors,
    facing: p.facing,
    google_rating: data.external_reviews?.google_rating,
    google_review_count: data.external_reviews?.google_review_count,
    google_reviews_url:
      data.external_reviews?.google_reviews_url ?? society?.google_reviews_url,
    root_source: data.root_source,
    project_status: data.project_status,
    project_status_display: data.project_status_display,
    home_state_display: data.home_state_display,
    builder_delivery_display: data.builder_trust?.delivery_display,
    data_freshness: data.data_freshness,
    decision_labels: data.decision_labels,
    decision_check_summary: data.decision_check_summary,
  };
}

function compactRecommendationArea(area: string): string {
  const parts = area.split(",").map((part) => part.trim()).filter(Boolean);
  if (area.length <= 32 || parts.length < 2) return area;
  return parts.at(-1) ?? area;
}

function NearbyHomeCard({
  item,
  sceneIndex,
}: {
  item: RecommendationShelfItem;
  sceneIndex: number;
}) {
  const property = item.property;
  const { images } = usePropertySceneImages({
    heroImage: property.hero_image,
    images: property.images,
  });
  const image = propertySceneImageAt(images, sceneIndex, property.hero_image);
  const title = property.society_name.trim() || property.title.trim();
  const area = compactRecommendationArea(property.area);
  const note = property.society_name
    ? [area, hasKnownNumber(property.bhk) ? `${property.bhk} BHK` : null]
        .filter(Boolean)
        .join(" · ")
    : area;
  const price = formatListingPrice(property);
  const rating = formatGoogleRating(property.google_rating);

  return (
    <article className="property-nearby-card">
      <Link to={`/property/${property.id}`}>
        <span className="property-nearby-card__image">
          <ImageWithFallback
            src={image}
            alt=""
            loading="lazy"
            fetchPriority="low"
          />
        </span>
        <em>
          {price}
          {rating
            ? ` · ★ ${rating}`
            : ""}
        </em>
        <strong>{title}</strong>
        <span>{note}</span>
      </Link>
    </article>
  );
}

function NearbyHomesRail({
  items,
  exploreHref,
}: {
  items: RecommendationShelfItem[];
  exploreHref: string;
}) {
  if (items.length === 0) return null;

  return (
    <section
      id="more-homes"
      className="property-nearby-rail"
      aria-labelledby="property-nearby-title"
    >
      <div className="property-section-line">
        <h2 id="property-nearby-title">More homes</h2>
      </div>
      <div className="property-nearby-rail__scroller">
        {items.map((item, index) => (
          <NearbyHomeCard key={item.id} item={item} sceneIndex={index} />
        ))}
      </div>
      <Link
        className="property-nearby-rail__all"
        to={exploreHref}
      >
        Explore all homes
      </Link>
    </section>
  );
}

function buildPropertyJsonLd(p: PropertyDetailResponse["property"]) {
  const sizeDescription = hasKnownNumber(p.carpet_area_sqft)
    ? `${p.carpet_area_sqft} sqft`
    : "available configuration";
  const jsonLd: Record<string, unknown> = {
    "@context": "https://schema.org",
    "@type": "RealEstateListing",
    name: p.title,
    description: p.description_summary || [
      hasKnownNumber(p.bhk) ? `${p.bhk} BHK` : null,
      sizeDescription,
      `in ${p.area}, ${p.city}`,
    ].filter(Boolean).join(", "),
    url: publicSiteUrl(`/property/${encodeURIComponent(p.id)}`),
    offers: {
      "@type": "Offer",
      price: p.price,
      priceCurrency: "INR",
    },
    address: {
      "@type": "PostalAddress",
      addressLocality: p.area,
      addressRegion: p.city,
    },
  };
  if (hasKnownNumber(p.bhk)) {
    jsonLd.numberOfRooms = p.bhk;
  }
  if (hasKnownNumber(p.carpet_area_sqft)) {
    jsonLd.floorSize = {
      "@type": "QuantitativeValue",
      value: p.carpet_area_sqft,
      unitCode: "FTK",
    };
  }
  if (p.hero_image) {
    jsonLd.image = backendUrl(p.hero_image);
  }
  return jsonLd;
}

export function PropertyPage() {
  const { id } = useParams<{ id: string }>();
  const [searchParams] = useSearchParams();

  if (!id)
    return (
      <PageState
        variant="not_found"
        context="property"
        message="Property was not found."
      />
    );

  const focusParam = searchParams.get("focus");
  const contextId = searchParams.get("context");
  return (
    <PropertyPageBody
      key={`${id}:${focusParam ?? ""}:${contextId ?? ""}`}
      id={id}
      focusParam={focusParam}
      contextId={contextId}
    />
  );
}

function PropertyPageBody({
  id,
  focusParam,
  contextId,
}: {
  id: string;
  focusParam: string | null;
  contextId: string | null;
}) {
  const proofFocus = useMemo(
    () => parseProofFocusParam(focusParam),
    [focusParam],
  );
  const { compareIds } = useNotebook();
  const shortlistKey = typeof window === "undefined"
    ? ""
    : readShortlistIds().join("\u001f");
  const explicitComparisonKey = useMemo(() => {
    const selectedIds = compareIds.length > 0
      ? compareIds
      : shortlistKey.split("\u001f").filter(Boolean);
    return [...new Set(selectedIds)]
      .filter((propertyId) => propertyId !== id)
      .slice(0, MAX_EXPLICIT_COMPARISON_CANDIDATES)
      .join("\u001f");
  }, [compareIds, id, shortlistKey]);
  const [storyPlaying, setStoryPlaying] = useState(true);
  const [data, setData] = useState<PropertyDetailResponse | null>(null);
  const [recommendations, setRecommendations] =
    useState<RecommendationResponse | null>(null);
  const [aroundThisHomeScene, setAroundThisHomeScene] =
    useState<SurfaceSceneResponse | null>(null);
  const [arrivalScene, setArrivalScene] =
    useState<SurfaceSceneResponse | null>(null);
  const [searchContextSocieties, setSearchContextSocieties] =
    useState<ArrivalSearchSociety[]>([]);
  const [comparisonResolution, setComparisonResolution] = useState<{
    key: string;
    properties: PropertyCard[];
  }>({ key: "", properties: [] });
  const [status, setStatus] = useState<
    "loading" | "error" | "not_found" | "ok"
  >("loading");
  const [retryKey, setRetryKey] = useState(0);
  const discoveryMapContext = useMemo(
    () => readDiscoveryMapContext(contextId),
    [contextId],
  );

  useEffect(() => {
    let cancelled = false;

    getProperty(id)
      .then((d) => {
        if (cancelled) return;
        setData(d);
        setStatus("ok");
      })
      .catch((err: Error) => {
        if (cancelled) return;
        setStatus(err.message.includes("404") ? "not_found" : "error");
      });

    return () => {
      cancelled = true;
    };
  }, [id, retryKey]);

  useEffect(() => {
    const propertyId = data?.property?.id;
    if (!propertyId) return;
    let cancelled = false;

    getPropertyRecommendations(propertyId)
      .then((response) => {
        if (cancelled) return;
        setRecommendations(response);
      })
      .catch(() => {
        if (cancelled) return;
        setRecommendations(null);
      });

    return () => {
      cancelled = true;
    };
  }, [data?.property?.id]);

  useEffect(() => {
    const currentSocietyId = data?.entity_refs.society_entity_id;
    const candidates = discoveryMapContext?.candidates
      .filter((candidate) => candidate.societyId !== currentSocietyId) ?? [];
    if (candidates.length === 0) {
      let cancelled = false;
      void Promise.resolve().then(() => {
        if (!cancelled) setSearchContextSocieties([]);
      });
      return () => { cancelled = true; };
    }
    const controller = new AbortController();
    void getPropertySurfacesBatch(
      candidates.map((candidate) => candidate.propertyId),
      [ARRIVAL_STORY_SURFACE_ID],
    ).then((response) => {
      if (controller.signal.aborted) return;
      const scenesByPropertyId = new Map(response.items.map((item) => [
        item.propertyId,
        item.scenes.find((scene) => scene.surfaceId === ARRIVAL_STORY_SURFACE_ID),
      ]));
      const resolved = candidates.flatMap((candidate) => {
        const scene = scenesByPropertyId.get(candidate.propertyId);
        const mapContext = propertyMapContextFromSurfaceScene(scene);
        const latitude = mapContext?.home.latitude;
        const longitude = mapContext?.home.longitude;
        if (
          !Number.isFinite(latitude)
          || !Number.isFinite(longitude)
          || !latitude
          || !longitude
          || latitude < -90
          || latitude > 90
          || longitude < -180
          || longitude > 180
        ) return [];
        return [{
          propertyId: candidate.propertyId,
          societyId: candidate.societyId,
          proofFocus: candidate.proofFocus,
          preview: candidate.preview,
          home: {
            latitude,
            longitude,
            name: mapContext.home.name,
            boundary: mapContext.home.boundary,
          },
        } satisfies ArrivalSearchSociety];
      });
      setSearchContextSocieties(resolved.slice(0, 3));
    }).catch(() => {
      if (!controller.signal.aborted) setSearchContextSocieties([]);
    });
    return () => controller.abort();
  }, [data?.entity_refs.society_entity_id, discoveryMapContext]);

  useEffect(() => {
    const propertyId = data?.property?.id;
    if (!propertyId) return;
    let cancelled = false;

    getPropertySurface(
      propertyId,
      initialPropertySurfaceId(proofFocus),
      propertySceneProofFocus(proofFocus),
    )
      .then((scene) => {
        if (!cancelled) setAroundThisHomeScene(scene);
      })
      .catch(() => {
        if (!cancelled) setAroundThisHomeScene(null);
      });

    return () => {
      cancelled = true;
    };
  }, [data?.property?.id, proofFocus]);

  useEffect(() => {
    const propertyId = data?.property?.id;
    if (!propertyId) return;
    let cancelled = false;

    getPropertySurface(propertyId, ARRIVAL_STORY_SURFACE_ID)
      .then((scene) => {
        if (!cancelled) setArrivalScene(scene);
      })
      .catch(() => {
        if (!cancelled) setArrivalScene(null);
      });

    return () => {
      cancelled = true;
    };
  }, [data?.property?.id]);

  useEffect(() => {
    const propertyId = data?.property?.id;
    const requestedIds = explicitComparisonKey.split("\u001f").filter(Boolean);
    if (!propertyId || requestedIds.length === 0) return undefined;

    const controller = new AbortController();
    void Promise.allSettled(
      requestedIds.map((requestedId) =>
        getProperty(requestedId, { signal: controller.signal })
      ),
    ).then((results) => {
      if (controller.signal.aborted) return;
      setComparisonResolution({
        key: explicitComparisonKey,
        properties: results.flatMap((result) =>
          result.status === "fulfilled"
            ? [propertyToCard(result.value)]
            : []),
      });
    });

    return () => controller.abort();
  }, [data?.property?.id, explicitComparisonKey]);

  useEffect(() => {
    if (status !== "ok" || !proofFocus?.targetId) return undefined;
    let secondFrame = 0;
    const firstFrame = window.requestAnimationFrame(() => {
      secondFrame = window.requestAnimationFrame(() => {
        const target = document.getElementById(proofFocus.targetId ?? "");
        if (!target) return;
        target.scrollIntoView({ block: "start" });
        target.focus({ preventScroll: true });
      });
    });
    return () => {
      window.cancelAnimationFrame(firstFrame);
      if (secondFrame) window.cancelAnimationFrame(secondFrame);
    };
  }, [status, data?.property?.id, proofFocus, aroundThisHomeScene, arrivalScene]);

  if (status === "loading")
    return (
      <div
        className="property-story-page property-story-loading"
        aria-label="Loading property"
        aria-busy="true"
      >
        <section className="property-scene">
          <div className="property-story-loading__identity">
            <div>
              <div className="skeleton-bar property-story-loading__location" />
              <div className="skeleton-bar property-story-loading__title" />
            </div>
            <div className="property-story-loading__summary">
              <div className="skeleton-bar property-story-loading__facts" />
              <div className="skeleton-bar property-story-loading__actions" />
            </div>
          </div>
          <div className="skeleton-bar property-story-loading__media" />
        </section>
      </div>
    );
  if (status === "not_found")
    return (
      <PageState
        variant="not_found"
        context="property"
        message={`Property "${id}" was not found.`}
      />
    );
  if (status === "error")
    return (
      <PageState
        variant="error"
        context="property"
        onRetry={() => {
          setData(null);
          setStatus("loading");
          setRetryKey((current) => current + 1);
        }}
      />
    );
  if (!data) return null;

  const { property: p, society } = data;

  const pageTitle = hasKnownNumber(p.bhk)
    ? `${p.title} — ${p.bhk} BHK in ${p.area} | ${PUBLIC_BRAND_NAME}`
    : `${p.title} in ${p.area} | ${PUBLIC_BRAND_NAME}`;
  const pricePerSqftLabel = hasKnownNumber(p.price_per_sqft)
    ? `${p.price_per_sqft.toLocaleString("en-IN")} /sqft`
    : null;
  const sizeLabel = hasKnownNumber(p.carpet_area_sqft)
    ? `${p.carpet_area_sqft.toLocaleString("en-IN")} sqft`
    : null;
  const pageDescription = [
    hasKnownNumber(p.bhk) ? `${p.bhk} BHK` : null,
    sizeLabel,
    `in ${society?.name ? society.name + ", " : ""}${p.area}`,
    hasKnownNumber(p.price) ? formatListingPrice(p) : null,
    pricePerSqftLabel,
    `${p.area}, ${p.city}`,
  ]
    .filter(Boolean)
    .join(". ");
  const canonicalUrl = publicSiteUrl(`/property/${encodeURIComponent(p.id)}`);
  const socialImageUrl = p.hero_image ? backendUrl(p.hero_image) : null;
  const aroundThisHomeContext = propertyMapContextFromSurfaceScene(
    aroundThisHomeScene,
    data.map_context,
  );
  const arrivalContext = propertyMapContextFromSurfaceScene(
    arrivalScene,
    data.map_context,
  );
  const showNearbyPlate = hasAroundThisHomePlate(aroundThisHomeContext);
  const recommendationBranches =
    recommendations?.items ?? data.recommendation_branches ?? [];
  const currentCard = propertyToCard(data);
  const explicitComparisonProperties = comparisonResolution.key
      === explicitComparisonKey
    ? comparisonResolution.properties
    : [];
  const displayTitle = p.title.trim();
  const story = projectPropertyStory(data, {
    mapAvailable: showNearbyPlate,
    comparisonProperties: explicitComparisonProperties,
    recommendationProperties: recommendationBranches.map(
      (branch) => branch.property,
    ),
  });
  const proofSourceUrl = proofFocus
    ? focusedEvidenceSource(data, proofFocus)
    : undefined;
  const officialRecordMatch = propertyProofMatch(
    proofFocus,
    "official-record",
    proofSourceUrl,
  );
  const residentVoiceMatch = propertyProofMatch(
    proofFocus,
    "resident-voice",
    proofSourceUrl,
  );
  const comparisonIds = new Set(story.comparisons.map((home) => home.id));
  const moreNearbyItems = recommendationShelfItems(
    recommendationBranches,
    currentCard,
    comparisonIds,
  );
  const exploreHref = propertyExploreHref(p.area);

  return (
    <div className="property-decision-page property-story-page">
      <Helmet>
        <title>{pageTitle}</title>
        <meta name="description" content={pageDescription} />
        <meta property="og:title" content={pageTitle} />
        <meta property="og:description" content={pageDescription} />
        <meta property="og:type" content="website" />
        <meta property="og:site_name" content={PUBLIC_BRAND_NAME} />
        <meta property="og:url" content={canonicalUrl} />
        <link rel="canonical" href={canonicalUrl} />
        {socialImageUrl && <meta property="og:image" content={socialImageUrl} />}
        <script type="application/ld+json">
          {JSON.stringify(buildPropertyJsonLd(p))}
        </script>
      </Helmet>
      <div className="property-scene property-scene--identity-only">
        <PropertySceneIdentity
          story={story}
          showFacts={false}
          actions={(
            <>
              <SaveHeartButton
                propertyId={p.id}
                className="property-action-link property-action-save"
                label="Save"
              />
              <NotebookCommentAnchor
                propertyId={p.id}
                labels={[]}
                detail={displayTitle}
                source="Property detail"
                label="Note"
              />
            </>
          )}
        />
      </div>
      <PropertySceneFacts story={story} pageScoped />
      <PropertySceneCard
        sectionId="property-cinema"
        story={story}
        showIdentity={false}
        playback={{
          playing: storyPlaying,
          onPlayingChange: setStoryPlaying,
        }}
      />

      <main className="property-clean-flow">
        <PropertySearchMatch data={data} focus={proofFocus} />

        {story.map.available && aroundThisHomeContext && (
          <section
            id="around-this-home"
            className="property-map-section"
            aria-label="Around this home"
            tabIndex={-1}
          >
            <AroundThisHomePlate
              propertyId={id}
              context={aroundThisHomeContext}
            />
          </section>
        )}

        <PropertyArrivalFilm
          propertyId={p.id}
          title={story.identity.title}
          frames={story.arrival.frames}
          mapContext={arrivalContext}
          searchContextSocieties={searchContextSocieties}
          playback={{
            playing: storyPlaying,
            onPlayingChange: setStoryPlaying,
          }}
        />

        <PropertyReviewsDeck
          model={story.reviews}
          reviews={data.external_reviews}
          signals={data.detail_signals}
          focusedMatch={residentVoiceMatch}
        />

        <PropertyReraTeaser
          cards={story.recordCards}
          focusedMatch={officialRecordMatch}
        />

        <PropertyShortCompare
          homes={story.comparisons}
          compareHref={story.compareHref}
        />

        <NearbyHomesRail
          items={moreNearbyItems}
          exploreHref={exploreHref}
        />
      </main>

    </div>
  );
}
