import { useEffect, useState } from "react";
import {
  Link,
  useParams,
  useNavigate,
  useSearchParams,
} from "react-router-dom";
import { Helmet } from "react-helmet-async";
import type {
  PropertyCard,
  PropertyDetailResponse,
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
import { PageState } from "../components/PageState.tsx";
import { RailPageControls } from "../components/RailPageControls.tsx";
import { AroundThisHomePlate } from "../components/evidence/AroundThisHomePlate.tsx";
import { ImageWithFallback } from "../components/ImageWithFallback.tsx";
import { AreaTrackerSection } from "../components/AreaTrackerSection.tsx";
import { usePropertySceneImages } from "../hooks/usePropertySceneImages.ts";
import { PropertyArrivalFilm } from "../components/property/PropertyArrivalFilm.tsx";
import { PropertyReraTeaser } from "../components/property/PropertyReraTeaser.tsx";
import { PropertyReviewsDeck } from "../components/property/PropertyReviewsDeck.tsx";
import { PropertySceneCard } from "../components/property/PropertySceneCard.tsx";
import { PropertyShortCompare } from "../components/property/PropertyShortCompare.tsx";
import { PropertyStoryTopbar } from "../components/property/PropertyStoryTopbar.tsx";
import { propertySceneImageAt } from "../lib/propertyScene.ts";
import { projectPropertyStory } from "../lib/propertyStory.ts";
import { formatGoogleRating } from "../lib/reviewFormatting.ts";
import { hasAroundThisHomePlate } from "../lib/nearbyPlateProjection.ts";
import { propertyMapContextFromSurfaceScene } from "../lib/surfaceSceneProjection.ts";

function formatPrice(price: number): string {
  if (!hasKnownNumber(price)) return "Price unavailable";
  if (price >= 10_000_000) return `₹${(price / 10_000_000).toFixed(1)} Cr`;
  if (price >= 100_000) return `₹${(price / 100_000).toFixed(1)} L`;
  return `₹${price.toLocaleString("en-IN")}`;
}

function hasKnownNumber(value: number | null | undefined): value is number {
  return typeof value === "number" && Number.isFinite(value) && value > 0;
}

function comparablePrice(price: number): number {
  return hasKnownNumber(price) ? price : Number.MAX_SAFE_INTEGER;
}

function cleanAreaToken(value: string): string {
  return value
    .toLowerCase()
    .replace(/[^a-z0-9\s,-]/g, " ")
    .split(/[,\s]+/)
    .filter((part) => part.length > 2)
    .join(" ");
}

function areaTokens(value: string): Set<string> {
  return new Set(cleanAreaToken(value).split(" ").filter(Boolean));
}

function societyKey(
  property: Pick<PropertyCard, "kg_entity_refs" | "society_name">,
): string {
  return (
    property.kg_entity_refs?.society_entity_id ||
    property.society_name.trim().toLowerCase()
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

type RankedRecommendationItem =
  | {
      kind: "branch";
      id: string;
      property: PropertyCard;
      branch: RecommendationResponse["items"][number];
    }
  | { kind: "nearby"; id: string; property: PropertyCard };

function reviewStrength(property: PropertyCard): number {
  const rating = property.google_rating ?? 0;
  const reviewCount = property.google_review_count ?? 0;
  if (rating <= 0 || reviewCount <= 0) return 0;
  return rating * 100 + Math.log10(reviewCount + 1) * 12;
}

function rankedRecommendationItems(
  branches: RecommendationResponse["items"],
  nearby: PropertyCard[],
): RankedRecommendationItem[] {
  const usedIds = new Set(branches.map((branch) => branch.property.id));
  const branchItems: RankedRecommendationItem[] = branches.map((branch) => ({
    kind: "branch",
    id: `${branch.branch_id}-${branch.property.id}`,
    property: branch.property,
    branch,
  }));
  const nearbyItems: RankedRecommendationItem[] = nearby
    .filter((property) => !usedIds.has(property.id))
    .map((property) => ({
      kind: "nearby",
      id: property.id,
      property,
    }));

  return [...branchItems, ...nearbyItems]
    .sort((left, right) => {
      const reviewDelta =
        reviewStrength(right.property) - reviewStrength(left.property);
      if (Math.abs(reviewDelta) > 0.001) return reviewDelta;
      const branchDelta =
        (right.kind === "branch" ? right.branch.magnitude : 0) -
        (left.kind === "branch" ? left.branch.magnitude : 0);
      if (Math.abs(branchDelta) > 0.001) return branchDelta;
      return (
        comparablePrice(left.property.price) -
        comparablePrice(right.property.price)
      );
    })
    .slice(0, 8);
}

function nearbyRailItems(
  primaryItems: RankedRecommendationItem[],
  properties: PropertyCard[],
  currentProperty: PropertyCard,
  preferredAreas: string[],
): RankedRecommendationItem[] {
  const currentSocietyKey = societyKey(currentProperty);
  const allowedAreas = new Set([currentProperty.area, ...preferredAreas]);
  const isDifferentSociety = (property: PropertyCard) =>
    !currentSocietyKey || societyKey(property) !== currentSocietyKey;
  const scopedPrimaryItems = primaryItems.filter(
    (item) =>
      allowedAreas.has(item.property.area) && isDifferentSociety(item.property),
  );
  const used = new Set(scopedPrimaryItems.map((item) => item.property.id));
  used.add(currentProperty.id);
  const areaRank = new Map(preferredAreas.map((area, index) => [area, index]));
  const fillers: RankedRecommendationItem[] = properties
    .filter((property) => !used.has(property.id))
    .filter((property) => allowedAreas.has(property.area))
    .filter(isDifferentSociety)
    .filter((property) => property.hero_image || property.society_name)
    .sort((left, right) => {
      const leftAreaRank = areaRank.get(left.area) ?? 99;
      const rightAreaRank = areaRank.get(right.area) ?? 99;
      if (leftAreaRank !== rightAreaRank) return leftAreaRank - rightAreaRank;
      const reviewDelta = reviewStrength(right) - reviewStrength(left);
      if (Math.abs(reviewDelta) > 0.001) return reviewDelta;
      return comparablePrice(left.price) - comparablePrice(right.price);
    })
    .slice(0, Math.max(0, 8 - scopedPrimaryItems.length))
    .map((property) => ({
      kind: "nearby",
      id: `nearby-${property.id}`,
      property,
    }));

  return [...scopedPrimaryItems, ...fillers].slice(0, 8);
}

function microMarketAreas(
  currentArea: string,
  currentPricePerSqft: number,
  properties: PropertyCard[],
  recommendationItems: RankedRecommendationItem[],
): string[] {
  const byArea = new Map<string, PropertyCard[]>();
  for (const property of properties) {
    if (!property.area || property.price_per_sqft <= 0) continue;
    const list = byArea.get(property.area) ?? [];
    list.push(property);
    byArea.set(property.area, list);
  }

  const currentTokens = areaTokens(currentArea);
  const recommendedAreas = new Set(
    recommendationItems.map((item) => item.property.area),
  );
  return [...byArea.entries()]
    .filter(([, areaProperties]) => areaProperties.length >= 2)
    .map(([area, areaProperties]) => {
      const tokens = areaTokens(area);
      const sharedTokenCount = [...tokens].filter((token) =>
        currentTokens.has(token),
      ).length;
      const medianPpsf = areaProperties
        .map((property) => property.price_per_sqft)
        .sort((a, b) => a - b)[Math.floor(areaProperties.length / 2)];
      const priceCloseness = hasKnownNumber(currentPricePerSqft)
        ? Math.max(0, 25 - Math.abs(medianPpsf - currentPricePerSqft) / 1000)
        : 0;
      const score =
        (area === currentArea ? 100 : 0) +
        (recommendedAreas.has(area) ? 45 : 0) +
        sharedTokenCount * 30 +
        Math.min(areaProperties.length, 10) +
        priceCloseness;
      return { area, score };
    })
    .sort(
      (left, right) =>
        right.score - left.score || left.area.localeCompare(right.area),
    )
    .slice(0, 5)
    .map((item) => item.area);
}

function NearbyHomeCard({
  item,
  sceneIndex,
}: {
  item: RankedRecommendationItem;
  sceneIndex: number;
}) {
  const property = item.property;
  const { images } = usePropertySceneImages({
    heroImage: property.hero_image,
    images: property.images,
  });
  const image = propertySceneImageAt(images, sceneIndex, property.hero_image);
  const title = property.title.trim();
  const note = `${property.area} · ${property.bhk} BHK`;

  return (
    <article className="property-nearby-card">
      <Link to={`/property/${property.id}`}>
        <span className="property-nearby-card__image">
          {image ? (
            <ImageWithFallback
              src={image}
              alt={title}
              loading="lazy"
              fetchPriority="low"
            />
          ) : (
            <span>{property.society_name || property.title}</span>
          )}
        </span>
        <strong>{title}</strong>
        <span>{note}</span>
        <em>
          {formatPrice(property.price)}
          {formatGoogleRating(property.google_rating)
            ? ` · ★ ${formatGoogleRating(property.google_rating)}`
            : ""}
        </em>
      </Link>
    </article>
  );
}

function NearbyHomesRail({
  items,
  status,
}: {
  items: RankedRecommendationItem[];
  status: RecommendationStatus;
}) {
  const [page, setPage] = useState(0);
  const [pageSize, setPageSize] = useState(4);

  useEffect(() => {
    const media = window.matchMedia("(max-width: 760px)");
    const syncPageSize = () => setPageSize(media.matches ? 1 : 4);
    syncPageSize();
    media.addEventListener("change", syncPageSize);
    return () => media.removeEventListener("change", syncPageSize);
  }, []);

  const pageCount = Math.max(1, Math.ceil(items.length / pageSize));
  const safePage = Math.min(page, pageCount - 1);
  const visibleItems = items.slice(
    safePage * pageSize,
    safePage * pageSize + pageSize,
  );

  if (items.length === 0 && status !== "pending") return null;

  return (
    <section
      className="property-nearby-rail"
      aria-labelledby="property-nearby-title"
    >
      <div className="property-section-line">
        <h2 id="property-nearby-title">More homes nearby</h2>
        <RailPageControls
          page={safePage}
          pageCount={items.length > pageSize ? pageCount : 1}
          onPageChange={setPage}
          label="More homes pages"
        />
      </div>
      <div className="property-nearby-rail__scroller">
        {status === "pending" && items.length === 0 && (
          <>
            <span className="property-nearby-skeleton" />
            <span className="property-nearby-skeleton" />
            <span className="property-nearby-skeleton" />
          </>
        )}
        {visibleItems.map((item, index) => (
          <NearbyHomeCard key={item.id} item={item} sceneIndex={index} />
        ))}
      </div>
    </section>
  );
}

function MicroMarketTracker({
  currentArea,
  properties,
  areas,
  onSelectArea,
}: {
  currentArea: string;
  properties: PropertyCard[];
  areas: string[];
  onSelectArea: (area: string) => void;
}) {
  if (areas.length === 0) return null;

  return (
    <AreaTrackerSection
      id="property-micro-markets"
      className="property-micro-market"
      properties={properties}
      areaTracker={null}
      preferredAreas={areas}
      highlightArea={currentArea}
      onSearch={onSelectArea}
      heading="Nearby markets"
      maxMarkets={areas.length}
    />
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
    description:
      p.description_summary ||
      `${p.bhk} BHK, ${sizeDescription} in ${p.area}, ${p.city}`,
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
    numberOfRooms: p.bhk,
  };
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
  const navigate = useNavigate();
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

    const focus = parseProofFocusParam(focusParam);
    getPropertySurface(propertyId, "around_this_home", focus)
      .then((scene) => {
        if (!cancelled) setAroundThisHomeScene(scene);
      })
      .catch(() => {
        if (!cancelled) setAroundThisHomeScene(null);
      });

    return () => {
      cancelled = true;
    };
  }, [data?.property?.id, focusParam]);

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
      <div className="page-container-wide">
        {/* Hero placeholder */}
        <div
          className="skeleton-bar"
          style={{
            width: "100%",
            height: "320px",
            borderRadius: "var(--radius-md)",
            marginBottom: "1.5rem",
          }}
        />
        {/* Title bar */}
        <div
          className="skeleton-bar"
          style={{ width: "60%", height: "28px", marginBottom: "0.5rem" }}
        />
        {/* Subtitle */}
        <div
          className="skeleton-bar"
          style={{ width: "40%", height: "16px", marginBottom: "1rem" }}
        />
        {/* Price bar */}
        <div
          className="skeleton-bar"
          style={{ width: "25%", height: "24px", marginBottom: "0.75rem" }}
        />
        {/* Tags row */}
        <div style={{ display: "flex", gap: "0.5rem", marginBottom: "2rem" }}>
          <div
            className="skeleton-bar"
            style={{ width: "60px", height: "24px", borderRadius: "999px" }}
          />
          <div
            className="skeleton-bar"
            style={{ width: "80px", height: "24px", borderRadius: "999px" }}
          />
          <div
            className="skeleton-bar"
            style={{ width: "70px", height: "24px", borderRadius: "999px" }}
          />
          <div
            className="skeleton-bar"
            style={{ width: "90px", height: "24px", borderRadius: "999px" }}
          />
        </div>
        {/* Two-column layout */}
        <div className="property-layout">
          <div className="property-main">
            <div className="skeleton-detail-section skeleton-bar" />
            <div className="skeleton-detail-section skeleton-bar" />
            <div
              className="skeleton-detail-section skeleton-bar"
              style={{ height: "140px" }}
            />
          </div>
          <div className="property-sidebar">
            <div
              className="skeleton-detail-section skeleton-bar"
              style={{ height: "120px" }}
            />
            <div
              className="skeleton-detail-section skeleton-bar"
              style={{ height: "160px" }}
            />
          </div>
        </div>
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

  const pageTitle = `${p.title} — ${p.bhk} BHK in ${p.area} | OpenEstates`;
  const pricePerSqftLabel = hasKnownNumber(p.price_per_sqft)
    ? `${p.price_per_sqft.toLocaleString("en-IN")} /sqft`
    : null;
  const sizeLabel = hasKnownNumber(p.carpet_area_sqft)
    ? `${p.carpet_area_sqft.toLocaleString("en-IN")} sqft`
    : null;
  const pageDescription = [
    `${p.bhk} BHK`,
    sizeLabel,
    `in ${society?.name ? society.name + ", " : ""}${p.area}`,
    hasKnownNumber(p.price) ? formatPrice(p.price) : null,
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
  const recommendationItems = rankedRecommendationItems(
    recommendationBranches,
    data.similar_properties,
  );
  const currentCard = propertyToCard(data);
  const marketPropertyMap = new Map<string, PropertyCard>();
  for (const property of [
    currentCard,
    ...allProperties,
    ...data.similar_properties,
    ...recommendationItems.map((item) => item.property),
  ]) {
    marketPropertyMap.set(property.id, property);
  }
  const marketProperties = [...marketPropertyMap.values()];
  const microAreas = microMarketAreas(
    p.area,
    p.price_per_sqft,
    marketProperties,
    recommendationItems,
  );
  const nearbyItems = nearbyRailItems(
    recommendationItems,
    marketProperties,
    currentCard,
    microAreas,
  );
  const displayTitle = p.title.trim();
  const story = projectPropertyStory(data, {
    comparisonProperties: recommendationBranches.map(
      (branch) => branch.property,
    ),
  });
  const comparisonIds = new Set(story.comparisons.map((home) => home.id));
  const moreNearbyItems = nearbyItems.filter(
    (item) => !comparisonIds.has(item.property.id),
  );
  function handleAreaSelect(area: string) {
    navigate(`/?q=${encodeURIComponent(area)}`);
  }

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
      <PropertyStoryTopbar
        propertyId={p.id}
        title={displayTitle}
        canPlay={
          story.media.frames.length > 1 || story.arrival.frames.length > 1
        }
        playing={storyPlaying}
        onPlayingChange={setStoryPlaying}
      />
      <PropertySceneCard
        sectionId="property-cinema"
        story={story}
        playback={{
          playing: storyPlaying,
          onPlayingChange: setStoryPlaying,
        }}
      />

      <main className="property-clean-flow">
        {showNearbyPlate && aroundThisHomeContext && (
          <section
            id="around-this-home"
            className="property-map-section"
            aria-label="Around this home"
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
        />

        <PropertyReraTeaser cards={story.recordCards} />

        <PropertyShortCompare
          homes={story.comparisons}
          compareHref={story.compareHref}
        />

        <NearbyHomesRail
          items={moreNearbyItems}
          status={recommendationStatus}
        />
        <MicroMarketTracker
          currentArea={p.area}
          properties={marketProperties}
          areas={microAreas}
          onSelectArea={handleAreaSelect}
        />
      </main>

    </div>
  );
}
