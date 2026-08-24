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
  RecommendationStatus,
  SurfaceSceneResponse,
} from "../lib/types.ts";
import {
  getProperties,
  getProperty,
  getPropertyRecommendations,
  getPropertySurface,
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
  projectStoryComparison,
  type StoryComparison,
} from "../lib/propertyStory.ts";
import { readShortlistIds } from "../lib/compare.ts";
import { formatGoogleRating } from "../lib/reviewFormatting.ts";
import { hasAroundThisHomePlate } from "../lib/nearbyPlateProjection.ts";
import { propertyMapContextFromSurfaceScene } from "../lib/surfaceSceneProjection.ts";
import { workspaceCompareHref } from "../lib/workspaceNav.ts";
import { formatListingPrice } from "../lib/listing-price.ts";
import {
  initialPropertySurfaceId,
  propertyProofMatch,
  propertySceneProofFocus,
} from "../lib/proof-focus.ts";

function hasKnownNumber(value: number | null | undefined): value is number {
  return typeof value === "number" && Number.isFinite(value) && value > 0;
}

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

function societyKey(
  property: Pick<PropertyCard, "kg_entity_refs" | "society_name">,
): string {
  return (
    property.kg_entity_refs?.society_entity_id ||
    property.society_name.trim().toLowerCase()
  );
}

function savedComparisonHomes(
  requestedIds: string[],
  propertiesById: Map<string, PropertyCard>,
  currentPropertyId: string,
): StoryComparison[] {
  const orderedIds = requestedIds.includes(currentPropertyId)
    ? [
        currentPropertyId,
        ...requestedIds.filter((propertyId) => propertyId !== currentPropertyId),
      ]
    : requestedIds;
  const usedSocieties = new Set<string>();
  const homes: StoryComparison[] = [];

  for (const propertyId of orderedIds) {
    const property = propertiesById.get(propertyId);
    if (!property) continue;
    const key = societyKey(property) || property.title.trim().toLocaleLowerCase();
    if (usedSocieties.has(key)) continue;
    usedSocieties.add(key);
    homes.push(projectStoryComparison(property, currentPropertyId));
    if (homes.length === 4) break;
  }

  return homes;
}

function distinctSocietyIds(
  requestedIds: string[],
  propertiesById: Map<string, PropertyCard>,
): string[] {
  const usedSocieties = new Set<string>();
  const ids: string[] = [];
  for (const propertyId of requestedIds) {
    const property = propertiesById.get(propertyId);
    if (!property) continue;
    const key = societyKey(property) || property.title.trim().toLocaleLowerCase();
    if (usedSocieties.has(key)) continue;
    usedSocieties.add(key);
    ids.push(propertyId);
    if (ids.length === 4) break;
  }
  return ids;
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
  const accessibleLabel = [
    title,
    note.replace(" · ", ", "),
    price,
    rating ? `Google ${rating}` : null,
  ].filter(Boolean).join(", ");

  return (
    <article className="property-nearby-card">
      <Link to={`/property/${property.id}`} aria-label={accessibleLabel}>
        <span className="property-nearby-card__image">
          {image ? (
            <ImageWithFallback
              src={image}
              alt=""
              loading="lazy"
              fetchPriority="low"
            />
          ) : (
            <span>{property.society_name || property.title}</span>
          )}
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
  status,
}: {
  items: RecommendationShelfItem[];
  status: RecommendationStatus;
}) {
  if (items.length === 0 && status !== "pending") return null;

  return (
    <section
      id="more-homes"
      className="property-nearby-rail"
      aria-labelledby="property-nearby-title"
    >
      <div className="property-section-line">
        <h2 id="property-nearby-title">More homes to compare</h2>
      </div>
      <div className="property-nearby-rail__scroller">
        {status === "pending" && items.length === 0 && (
          <>
            <span className="property-nearby-skeleton" />
            <span className="property-nearby-skeleton" />
            <span className="property-nearby-skeleton" />
            <span className="property-nearby-skeleton" />
          </>
        )}
        {items.map((item, index) => (
          <NearbyHomeCard key={item.id} item={item} sceneIndex={index} />
        ))}
      </div>
      <Link
        className="property-nearby-rail__all"
        to="/"
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
    url: `https://openestates.in/property/${p.id}`,
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
    jsonLd.image = p.hero_image;
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
  return (
    <PropertyPageBody
      key={`${id}:${focusParam ?? ""}`}
      id={id}
      focusParam={focusParam}
    />
  );
}

function PropertyPageBody({
  id,
  focusParam,
}: {
  id: string;
  focusParam: string | null;
}) {
  const proofFocus = useMemo(
    () => parseProofFocusParam(focusParam),
    [focusParam],
  );
  const { compareIds } = useNotebook();
  const [storyPlaying, setStoryPlaying] = useState(true);
  const [data, setData] = useState<PropertyDetailResponse | null>(null);
  const [recommendations, setRecommendations] =
    useState<RecommendationResponse | null>(null);
  const [aroundThisHomeScene, setAroundThisHomeScene] =
    useState<SurfaceSceneResponse | null>(null);
  const [allProperties, setAllProperties] = useState<PropertyCard[]>([]);
  const [recommendationStatus, setRecommendationStatus] =
    useState<RecommendationStatus>("pending");
  const [status, setStatus] = useState<
    "loading" | "error" | "not_found" | "ok"
  >("loading");

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
  }, [id]);

  useEffect(() => {
    const propertyId = data?.property?.id;
    if (!propertyId) return;
    let cancelled = false;

    getPropertyRecommendations(propertyId)
      .then((response) => {
        if (cancelled) return;
        setRecommendations(response);
        setRecommendationStatus(response.status);
      })
      .catch(() => {
        if (cancelled) return;
        setRecommendations(null);
        setRecommendationStatus("unavailable");
      });

    return () => {
      cancelled = true;
    };
  }, [data?.property?.id]);

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
  }, [status, data?.property?.id, proofFocus, aroundThisHomeScene]);

  useEffect(() => {
    let cancelled = false;

    getProperties()
      .then((properties) => {
        if (!cancelled) setAllProperties(properties);
      })
      .catch(() => {
        if (!cancelled) setAllProperties([]);
      });

    return () => {
      cancelled = true;
    };
  }, []);

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
    return <PageState variant="error" context="property" />;
  if (!data) return null;

  const { property: p, society } = data;

  const pageTitle = hasKnownNumber(p.bhk)
    ? `${p.title} — ${p.bhk} BHK in ${p.area} | OpenEstates`
    : `${p.title} in ${p.area} | OpenEstates`;
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
  const aroundThisHomeContext = propertyMapContextFromSurfaceScene(
    aroundThisHomeScene,
    data.map_context,
  );
  const showNearbyPlate = hasAroundThisHomePlate(aroundThisHomeContext);
  const recommendationBranches =
    recommendations?.items ?? data.recommendation_branches ?? [];
  const currentCard = propertyToCard(data);
  const marketPropertyMap = new Map<string, PropertyCard>();
  for (const property of [
    currentCard,
    ...allProperties,
    ...data.similar_properties,
    ...recommendationBranches.map((branch) => branch.property),
  ]) {
    marketPropertyMap.set(property.id, property);
  }
  const displayTitle = p.title.trim();
  const story = projectPropertyStory(data);
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
  const savedIds = readShortlistIds();
  const requestedCompareIds = compareIds.length > 0 ? compareIds : savedIds;
  const availableCompareIds = compareIds.length > 0
    ? requestedCompareIds
        .filter((propertyId) => marketPropertyMap.has(propertyId))
        .slice(0, 4)
    : distinctSocietyIds(requestedCompareIds, marketPropertyMap);
  const selectedCompareIds = [
    p.id,
    ...availableCompareIds.filter((propertyId) => propertyId !== p.id),
  ].slice(0, 4);
  const savedComparisons = savedComparisonHomes(
    selectedCompareIds,
    marketPropertyMap,
    p.id,
  );
  const savedCompareHref = workspaceCompareHref(
    selectedCompareIds,
    p.id,
  );
  const comparisonIds = new Set(savedComparisons.map((home) => home.id));
  const moreNearbyItems = recommendationShelfItems(
    recommendationBranches,
    currentCard,
    comparisonIds,
  );

  return (
    <div className="property-decision-page property-story-page">
      <Helmet>
        <title>{pageTitle}</title>
        <meta name="description" content={pageDescription} />
        <meta property="og:title" content={pageTitle} />
        <meta property="og:description" content={pageDescription} />
        <meta property="og:type" content="website" />
        <meta property="og:site_name" content="OpenEstates" />
        {p.hero_image && <meta property="og:image" content={p.hero_image} />}
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

        {showNearbyPlate && aroundThisHomeContext && (
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
          homes={savedComparisons}
          compareHref={savedCompareHref}
        />

        <NearbyHomesRail
          items={moreNearbyItems}
          status={recommendationStatus}
        />
      </main>

    </div>
  );
}
