import { useEffect, useState, type ReactNode } from "react";
import { Link, useParams, useNavigate, useSearchParams } from "react-router-dom";
import { Helmet } from "react-helmet-async";
import type {
  DetailSignal,
  DecisionCheckSummary,
  DecisionLabel,
  EvidenceSection,
  ExternalReviewCard,
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
import { ApproachRoadTrail, hasApproachRoadTrail } from "../components/evidence/ApproachRoadTrail.tsx";
import { AroundThisHomePlate } from "../components/evidence/AroundThisHomePlate.tsx";
import { SaveHeartButton } from "../components/SaveHeartButton.tsx";
import { NotebookCommentAnchor } from "../components/notebook/NotebookCommentAnchor.tsx";
import { NotebookPinButton } from "../components/notebook/NotebookPinButton.tsx";
import { LabelPill, type LabelPillTone } from "../components/ui/LabelPill.tsx";
import { ImageWithFallback } from "../components/ImageWithFallback.tsx";
import {
  derivePriceBands,
  formatSqftCompact,
} from "../components/AreaPriceBands.tsx";
import { AreaTrackerSection } from "../components/AreaTrackerSection.tsx";
import { usePropertySceneImages } from "../hooks/usePropertySceneImages.ts";
import { propertySceneImageAt, sceneLabelForIndex } from "../lib/propertyScene.ts";
import { LabelVisualIcon } from "../lib/LabelVisualIcon.tsx";
import {
  isRedundantHomeState,
} from "../lib/property-signals.ts";
import { hasAroundThisHomePlate } from "../lib/nearbyPlateProjection.ts";
import { propertyMapContextFromSurfaceScene } from "../lib/surfaceSceneProjection.ts";

function formatPrice(price: number): string {
  if (price >= 10_000_000) return `${(price / 10_000_000).toFixed(1)} Cr`;
  if (price >= 100_000) return `${(price / 100_000).toFixed(1)} L`;
  return price.toLocaleString("en-IN");
}

function hasKnownNumber(value: number | null | undefined): value is number {
  return typeof value === "number" && Number.isFinite(value) && value > 0;
}

function compactLifecycleLabel(value: string): string {
  const normalized = value
    .replace(/^home state:\s*/i, "")
    .replace(/_/g, " ")
    .trim();
  if (!normalized) return value;
  return normalized.charAt(0).toUpperCase() + normalized.slice(1);
}

function displayName(value: string): string {
  const keepUpper = new Set(["BHK", "ITPL", "JP", "KR"]);
  return value
    .replace(/^(\d+(?:\.\d+)?)\s+BHK\s+(?:in|at)\s+/i, "$1 BHK ")
    .replace(/\b[A-Z][A-Z0-9&.'-]*\b/g, (word) => {
    if (keepUpper.has(word) || /\d/.test(word)) return word;
    return word.charAt(0) + word.slice(1).toLowerCase();
    });
}

function truncateCopy(value: string, limit = 220): string {
  const normalized = value.replace(/\s+/g, " ").trim();
  if (normalized.length <= limit) return normalized;
  const trimmed = normalized.slice(0, limit).replace(/\s+\S*$/, "");
  return `${trimmed}...`;
}

function reviewSnippetCopy(value: string): string {
  return truncateCopy(
    value
      .replace(/^Google review feedback reads/i, "Google reviews read")
      .replace(/,?\s*though recurring themes are still being extracted\.?/i, "."),
  );
}

function formatGoogleRating(value: number | null | undefined): string | null {
  if (!hasKnownNumber(value)) return null;
  return value.toFixed(1);
}

function ratingTone(value: number | null | undefined): "good" | "weak" | null {
  if (!hasKnownNumber(value)) return null;
  return value >= 4 ? "good" : "weak";
}

function formatReviewCount(value: number | null | undefined): string | null {
  if (!hasKnownNumber(value)) return null;
  return `${value.toLocaleString("en-IN")} Google ${value === 1 ? "review" : "reviews"}`;
}

function reviewSpaceCost(review: ExternalReviewCard): number {
  const words = review.text.trim().split(/\s+/).filter(Boolean).length;
  if (words <= 32) return 1;
  if (words <= 70) return 1.8;
  return 2.4;
}

function fitReviewCards(reviewCards: ExternalReviewCard[], budget = 22): ExternalReviewCard[] {
  const selected: ExternalReviewCard[] = [];
  let used = 0;
  for (const review of reviewCards) {
    const cost = reviewSpaceCost(review);
    if (selected.length >= 8 && used + cost > budget) break;
    if (selected.length >= 12) break;
    selected.push(review);
    used += cost;
  }
  return selected;
}

function detailSignalPills(signals: DetailSignal[] | undefined): DetailSignal[] {
  return (signals ?? []).filter((signal) => signal.label.trim()).slice(0, 8);
}

function PropertySignalPills({ signals }: { signals: DetailSignal[] | undefined }) {
  const signalPills = detailSignalPills(signals);
  if (signalPills.length === 0) return null;

  return (
    <section className="property-signal-section" aria-label="Positive review themes">
      <span className="property-signal-section__label">Positive themes</span>
      <div className="property-signal-pills">
        {signalPills.map((signal) => (
          <span key={signal.key} className="property-signal-pill">
            <LabelVisualIcon id={signal.icon || signal.key} size={22} />
            <strong>{signal.label}</strong>
          </span>
        ))}
      </div>
    </section>
  );
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

function societyKey(property: Pick<PropertyCard, "kg_entity_refs" | "society_name">): string {
  return property.kg_entity_refs?.society_entity_id || property.society_name.trim().toLowerCase();
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
    google_reviews_url: data.external_reviews?.google_reviews_url ?? society?.google_reviews_url,
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
  | { kind: "branch"; id: string; property: PropertyCard; branch: RecommendationResponse["items"][number] }
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
      const reviewDelta = reviewStrength(right.property) - reviewStrength(left.property);
      if (Math.abs(reviewDelta) > 0.001) return reviewDelta;
      const branchDelta =
        (right.kind === "branch" ? right.branch.magnitude : 0)
        - (left.kind === "branch" ? left.branch.magnitude : 0);
      if (Math.abs(branchDelta) > 0.001) return branchDelta;
      return left.property.price - right.property.price;
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
  const scopedPrimaryItems = primaryItems.filter((item) =>
    allowedAreas.has(item.property.area) && isDifferentSociety(item.property));
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
      return left.price - right.price;
    })
    .slice(0, Math.max(0, 8 - scopedPrimaryItems.length))
    .map((property) => ({
      kind: "nearby",
      id: `nearby-${property.id}`,
      property,
    }));

  return [...scopedPrimaryItems, ...fillers].slice(0, 8);
}

function InlinePriceRangeSignal({
  area,
  pricePerSqft,
  properties,
}: {
  area: string;
  pricePerSqft: number;
  properties: PropertyCard[];
}) {
  if (!hasKnownNumber(pricePerSqft)) return null;
  const band = derivePriceBands(properties, [area])[0];
  if (!band) return null;

  const typicalLow = band.p25;
  const typicalHigh = band.p75;
  const areaName = area.split(",")[0];

  return (
    <p
      className="property-price-range"
      aria-label={`${formatSqftCompact(pricePerSqft)} per sqft against ${areaName} range ${formatSqftCompact(typicalLow)} to ${formatSqftCompact(typicalHigh)}`}
    >
      <span>{formatSqftCompact(pricePerSqft)}/sqft</span>
      <span>{areaName} range {formatSqftCompact(typicalLow)}-{formatSqftCompact(typicalHigh)}/sqft</span>
    </p>
  );
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
  const recommendedAreas = new Set(recommendationItems.map((item) => item.property.area));
  return [...byArea.entries()]
    .filter(([, areaProperties]) => areaProperties.length >= 2)
    .map(([area, areaProperties]) => {
      const tokens = areaTokens(area);
      const sharedTokenCount = [...tokens].filter((token) => currentTokens.has(token)).length;
      const medianPpsf = areaProperties
        .map((property) => property.price_per_sqft)
        .sort((a, b) => a - b)[Math.floor(areaProperties.length / 2)];
      const priceCloseness = hasKnownNumber(currentPricePerSqft)
        ? Math.max(0, 25 - Math.abs(medianPpsf - currentPricePerSqft) / 1000)
        : 0;
      const score =
        (area === currentArea ? 100 : 0)
        + (recommendedAreas.has(area) ? 45 : 0)
        + sharedTokenCount * 30
        + Math.min(areaProperties.length, 10)
        + priceCloseness;
      return { area, score };
    })
    .sort((left, right) => right.score - left.score || left.area.localeCompare(right.area))
    .slice(0, 5)
    .map((item) => item.area);
}

function reviewEvidenceSections(sections: EvidenceSection[]): EvidenceSection[] {
  return sections.filter((section) =>
    ["community", "community_pulse", "resident_reviews", "reviews"].includes(section.kind)
    || /review|resident|community/i.test(`${section.kind} ${section.title}`));
}

function CleanDialog({
  title,
  kicker,
  children,
  onClose,
}: {
  title: string;
  kicker?: string;
  children: ReactNode;
  onClose: () => void;
}) {
  useEffect(() => {
    const previousOverflow = document.body.style.overflow;
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    document.body.style.overflow = "hidden";
    document.addEventListener("keydown", closeOnEscape);
    return () => {
      document.body.style.overflow = previousOverflow;
      document.removeEventListener("keydown", closeOnEscape);
    };
  }, [onClose]);

  return (
    <div
      className="property-clean-dialog__backdrop"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <section
        className="property-clean-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="property-clean-dialog-title"
      >
        <div className="property-clean-dialog__head">
          <div>
            {kicker && <span>{kicker}</span>}
            <h2 id="property-clean-dialog-title">{title}</h2>
          </div>
          <button type="button" className="property-clean-dialog__close" onClick={onClose} aria-label={`Close ${title}`}>
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M18 6 6 18M6 6l12 12" />
            </svg>
          </button>
        </div>
        <div className="property-clean-dialog__body">{children}</div>
      </section>
    </div>
  );
}

function PropertyPhotoMosaic({
  title,
  societyName,
  heroImage,
  images,
  societyId,
}: {
  title: string;
  societyName?: string;
  heroImage?: string | null;
  images?: string[];
  societyId?: string;
}) {
  const [open, setOpen] = useState(false);
  const { images: sceneImages, loading, hasImages } = usePropertySceneImages({
    heroImage,
    images,
    societyId,
  });
  const leadImage = propertySceneImageAt(sceneImages, 0, heroImage);
  const mosaicImages = sceneImages.slice(1, 5);
  const total = sceneImages.length || (heroImage ? 1 : 0);

  return (
    <section className="property-photo-mosaic" aria-label="Property photos">
      <div className="property-photo-mosaic__lead">
        {leadImage ? (
          <ImageWithFallback src={leadImage} alt={title} loading="eager" />
        ) : (
          <div className="property-photo-mosaic__empty">
            <span>{loading ? "Loading photos" : "Photos unavailable"}</span>
            <strong>{societyName || title}</strong>
          </div>
        )}
      </div>
      <div className="property-photo-mosaic__grid">
        {mosaicImages.map((src, index) => (
          <button key={src} type="button" onClick={() => setOpen(true)}>
            <ImageWithFallback src={src} alt={`${title} - ${sceneLabelForIndex(index + 1)}`} loading="lazy" />
            <span>{sceneLabelForIndex(index + 1)}</span>
          </button>
        ))}
      </div>
      {hasImages && (
        <button type="button" className="property-photo-mosaic__all" onClick={() => setOpen(true)}>
          Show all photos
          <span>{total}</span>
        </button>
      )}

      {open && (
        <CleanDialog title="All photos" kicker="Gallery" onClose={() => setOpen(false)}>
          <div className="property-photo-grid">
            {sceneImages.map((src, index) => (
              <figure key={src}>
                <ImageWithFallback src={src} alt={`${title} - ${sceneLabelForIndex(index)}`} loading="lazy" />
                <figcaption>{sceneLabelForIndex(index)}</figcaption>
              </figure>
            ))}
          </div>
        </CleanDialog>
      )}
    </section>
  );
}

function PopupActionButton({
  label,
  caption,
  tone,
  onClick,
}: {
  label: string;
  caption?: string;
  tone?: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      className={`property-popup-action${tone ? ` property-popup-action--${tone}` : ""}`}
      onClick={onClick}
      aria-haspopup="dialog"
    >
      <span>
        <strong>{label}</strong>
        {caption && <small>{caption}</small>}
      </span>
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true">
        <path d="m9 18 6-6-6-6" />
      </svg>
    </button>
  );
}

function safeNotebookLabels(label: DecisionLabel): string[] {
  const labels = label.notebookLabels?.filter(Boolean) ?? [];
  return labels.length > 0 ? labels.slice(0, 4) : [label.key];
}

function projectCheckTags(summary: DecisionCheckSummary): DecisionLabel[] {
  const seen = new Set<string>();
  const labels = (summary.groups ?? [])
    .flatMap((group) => group.labels)
    .concat(summary.primaryLabels ?? []);
  return labels.filter((label) => {
    if (seen.has(label.key)) return false;
    seen.add(label.key);
    return true;
  });
}

function decisionTone(severity: DecisionLabel["severity"]): LabelPillTone {
  if (severity === "positive") return "positive";
  if (severity === "caution") return "caution";
  if (severity === "risk") return "risk";
  return "info";
}

function ProjectCheckTag({
  label,
  propertyId,
}: {
  label: DecisionLabel;
  propertyId: string;
}) {
  return (
    <LabelPill
      label={label.label}
      surface="fact"
      tone={decisionTone(label.severity)}
      className="property-check-tag"
    >
      <NotebookPinButton
        propertyId={propertyId}
        catalogKey={`rera:${propertyId}:label:${label.key}`}
        title={label.label}
        labels={safeNotebookLabels(label)}
        detail={label.valueText ?? label.label}
        source="RERA"
        className="property-check-tag__pin"
      />
    </LabelPill>
  );
}

function ProjectChecksContent({
  summary,
  propertyId,
}: {
  summary: DecisionCheckSummary;
  propertyId: string;
}) {
  const tags = projectCheckTags(summary);
  return (
    <div className="property-checks">
      <div className="property-checks__registry">
        {summary.registrationNumber && (
          <div className="property-checks__registry-number">
            <button
              type="button"
              onClick={() => void navigator.clipboard?.writeText(summary.registrationNumber ?? "")}
              title="Copy registration number"
            >
              {summary.registrationNumber}
            </button>
            <NotebookPinButton
              propertyId={propertyId}
              catalogKey={`rera:${propertyId}:registration:${summary.registrationNumber}`}
              title={`RERA ${summary.registrationNumber}`}
              labels={["legal"]}
              detail={summary.registrationNumber}
              source="RERA"
              className="property-checks__registry-pin"
            />
          </div>
        )}
      </div>
      <div className="property-check-tags" aria-label="RERA facts">
        {tags.map((label) => (
          <ProjectCheckTag key={label.key} label={label} propertyId={propertyId} />
        ))}
      </div>
    </div>
  );
}

function GoogleReviewsSection({
  data,
  reviewSections,
}: {
  data: PropertyDetailResponse;
  reviewSections: EvidenceSection[];
}) {
  const { society } = data;
  const reviews = data.external_reviews;
  const googleUrl = reviews?.google_reviews_url ?? society?.google_reviews_url;
  const rating = formatGoogleRating(reviews?.google_rating);
  const reviewCount = formatReviewCount(reviews?.google_review_count);
  const communityPulse = reviewSections.find((section) => section.community_pulse)?.community_pulse;
  const fallbackCards: ExternalReviewCard[] = [
    communityPulse?.paragraph,
    ...(communityPulse?.quotes?.slice(0, 2).map((quote) => quote.text) ?? []),
  ]
    .filter((value): value is string => Boolean(value?.trim()))
    .map((value, index) => ({
      id: `review-fallback-${index}`,
      source: "Google",
      author: "Google reviewer",
      text: reviewSnippetCopy(value),
      tone: "neutral" as const,
    }));
  const reviewSourceCards = reviews?.reviews?.length ? reviews.reviews : fallbackCards;
  const reviewCards = fitReviewCards(reviewSourceCards);
  const reviewButtonLabel = reviewCount
    ? `Show all ${reviewCount.replace(" Google ", " ")}`
    : "Show more Google reviews";

  if (!googleUrl && reviewCards.length === 0 && !rating) return null;

  return (
    <section className="property-google-reviews" aria-labelledby="property-google-reviews-title">
      <div className="property-section-line">
        <h2 id="property-google-reviews-title">
          {rating ? `★ ${rating}` : "Google reviews"}
          {reviewCount ? ` · ${reviewCount}` : ""}
        </h2>
      </div>

      {reviewCards.length > 0 && (
        <div className="property-review-grid">
          {reviewCards.map((review) => (
            <article key={review.id} className="property-review-card">
              {(review.rating || review.date_label) && (
                <p className="property-review-card__meta">
                  {review.rating && <span>{"★".repeat(Math.round(review.rating))}</span>}
                  {review.rating && review.date_label && " · "}
                  {review.date_label}
                </p>
              )}
              <p>{review.text}</p>
            </article>
          ))}
        </div>
      )}

      {googleUrl && (
        <a className="property-review-more" href={googleUrl} target="_blank" rel="noreferrer">
          {reviewButtonLabel}
        </a>
      )}
    </section>
  );
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
    societyId: property.kg_entity_refs?.society_entity_id,
  });
  const image = propertySceneImageAt(images, sceneIndex, property.hero_image);
  const title = displayName(property.title);
  const note = `${property.area} · ${property.bhk} BHK`;

  return (
    <article className="property-nearby-card">
      <Link to={`/property/${property.id}`}>
        <span className="property-nearby-card__image">
          {image ? (
            <ImageWithFallback src={image} alt={title} loading="lazy" />
          ) : (
            <span>{property.society_name || property.title}</span>
          )}
        </span>
        <strong>{title}</strong>
        <span>{note}</span>
        <em>
          ₹{formatPrice(property.price)}
          {formatGoogleRating(property.google_rating) ? ` · ★ ${formatGoogleRating(property.google_rating)}` : ""}
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
  const visibleItems = items.slice(safePage * pageSize, safePage * pageSize + pageSize);

  if (items.length === 0 && status !== "pending") return null;

  return (
    <section className="property-nearby-rail" aria-labelledby="property-nearby-title">
      <div className="property-section-line">
        <h2 id="property-nearby-title">More homes nearby</h2>
        {items.length > pageSize && (
          <div className="property-rail-controls" aria-label="More homes pages">
            <button
              type="button"
              onClick={() => setPage(Math.max(0, safePage - 1))}
              disabled={safePage === 0}
              aria-label="Previous homes"
            >
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true">
                <path d="m15 18-6-6 6-6" />
              </svg>
            </button>
            <span>{safePage + 1} / {pageCount}</span>
            <button
              type="button"
              onClick={() => setPage(Math.min(pageCount - 1, safePage + 1))}
              disabled={safePage >= pageCount - 1}
              aria-label="Next homes"
            >
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true">
                <path d="m9 18 6-6-6-6" />
              </svg>
            </button>
          </div>
        )}
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
    description: p.description_summary || `${p.bhk} BHK, ${sizeDescription} in ${p.area}, ${p.city}`,
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

  if (!id) return <PageState variant="not_found" context="property" message="Property was not found." />;

  const focusParam = searchParams.get("focus");
  return <PropertyPageBody key={`${id}:${focusParam ?? ""}`} id={id} focusParam={focusParam} />;
}

function PropertyPageBody({
  id,
  focusParam,
}: {
  id: string;
  focusParam: string | null;
}) {
  const navigate = useNavigate();
  const [data, setData] = useState<PropertyDetailResponse | null>(null);
  const [recommendations, setRecommendations] = useState<RecommendationResponse | null>(null);
  const [aroundThisHomeScene, setAroundThisHomeScene] =
    useState<SurfaceSceneResponse | null>(null);
  const [allProperties, setAllProperties] = useState<PropertyCard[]>([]);
  const [recommendationStatus, setRecommendationStatus] =
    useState<RecommendationStatus>("pending");
  const [status, setStatus] = useState<"loading" | "error" | "not_found" | "ok">("loading");
  const [projectChecksOpen, setProjectChecksOpen] = useState(false);

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

  if (status === "loading") return (
    <div className="page-container-wide">
      {/* Hero placeholder */}
      <div className="skeleton-bar" style={{ width: "100%", height: "320px", borderRadius: "var(--radius-md)", marginBottom: "1.5rem" }} />
      {/* Title bar */}
      <div className="skeleton-bar" style={{ width: "60%", height: "28px", marginBottom: "0.5rem" }} />
      {/* Subtitle */}
      <div className="skeleton-bar" style={{ width: "40%", height: "16px", marginBottom: "1rem" }} />
      {/* Price bar */}
      <div className="skeleton-bar" style={{ width: "25%", height: "24px", marginBottom: "0.75rem" }} />
      {/* Tags row */}
      <div style={{ display: "flex", gap: "0.5rem", marginBottom: "2rem" }}>
        <div className="skeleton-bar" style={{ width: "60px", height: "24px", borderRadius: "999px" }} />
        <div className="skeleton-bar" style={{ width: "80px", height: "24px", borderRadius: "999px" }} />
        <div className="skeleton-bar" style={{ width: "70px", height: "24px", borderRadius: "999px" }} />
        <div className="skeleton-bar" style={{ width: "90px", height: "24px", borderRadius: "999px" }} />
      </div>
      {/* Two-column layout */}
      <div className="property-layout">
        <div className="property-main">
          <div className="skeleton-detail-section skeleton-bar" />
          <div className="skeleton-detail-section skeleton-bar" />
          <div className="skeleton-detail-section skeleton-bar" style={{ height: "140px" }} />
        </div>
        <div className="property-sidebar">
          <div className="skeleton-detail-section skeleton-bar" style={{ height: "120px" }} />
          <div className="skeleton-detail-section skeleton-bar" style={{ height: "160px" }} />
        </div>
      </div>
    </div>
  );
  if (status === "not_found") return <PageState variant="not_found" context="property" message={`Property "${id}" was not found.`} />;
  if (status === "error") return <PageState variant="error" context="property" />;
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
    formatPrice(p.price),
    pricePerSqftLabel,
    `${p.area}, ${p.city}`,
  ].filter(Boolean).join(". ");
  const detailEvidenceSections = data.evidence?.sections ?? [];
  const showApproachTrail = hasApproachRoadTrail(detailEvidenceSections);
  const aroundThisHomeContext =
    propertyMapContextFromSurfaceScene(aroundThisHomeScene, data.map_context);
  const showNearbyPlate = hasAroundThisHomePlate(aroundThisHomeContext);
  const showHomeStateChip = Boolean(
    data.home_state_display
    && !isRedundantHomeState(
      data.home_state_display,
      data.project_status_display,
      p.possession_status,
    ),
  );
  const lifecycleTag = showHomeStateChip && data.home_state_display
    ? compactLifecycleLabel(data.home_state_display)
    : null;
  const recommendationBranches = recommendations?.items ?? data.recommendation_branches ?? [];
  const recommendationItems = rankedRecommendationItems(recommendationBranches, data.similar_properties);
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
  const microAreas = microMarketAreas(p.area, p.price_per_sqft, marketProperties, recommendationItems);
  const nearbyItems = nearbyRailItems(recommendationItems, marketProperties, currentCard, microAreas);
  const googleRating = formatGoogleRating(data.external_reviews?.google_rating);
  const googleRatingTone = ratingTone(data.external_reviews?.google_rating);
  const compactStatusRead = (lifecycleTag || data.home_state_display || data.project_status_display || p.possession_status)
    ?.split("·")[0]
    ?.trim();
  const reviewsSections = reviewEvidenceSections(detailEvidenceSections);
  const displayTitle = displayName(p.title);
  const projectChecks = data.decision_check_summary;
  const showProjectChecks = Boolean(projectChecks);

  function handleAreaSelect(area: string) {
    navigate(`/?q=${encodeURIComponent(area)}`);
  }

  return (
    <div className="page-container-wide property-decision-page">
      <Helmet>
        <title>{pageTitle}</title>
        <meta name="description" content={pageDescription} />
        <meta property="og:title" content={pageTitle} />
        <meta property="og:description" content={pageDescription} />
        <meta property="og:type" content="website" />
        <meta property="og:site_name" content="OpenEstates" />
        {p.hero_image && <meta property="og:image" content={p.hero_image} />}
        <script type="application/ld+json">{JSON.stringify(buildPropertyJsonLd(p))}</script>
      </Helmet>
      <section className="property-clean-head">
        <div className="property-clean-head__copy">
          <p>{p.area}, {p.city}</p>
          <h1>{displayTitle}</h1>
        </div>
        <div className="property-clean-actions" aria-label="Property actions">
          <SaveHeartButton propertyId={p.id} className="property-action-link property-action-save" label="Save" />
          <NotebookCommentAnchor
            propertyId={p.id}
            labels={[]}
            detail={displayTitle}
            source="Property detail"
            className="property-action-note"
          />
        </div>
      </section>

      <section className="property-summary-card" aria-label="Home summary">
        <div className="property-clean-meta">
          <span>₹{formatPrice(p.price)}</span>
          <span>{p.bhk} BHK</span>
          {sizeLabel && <span>{sizeLabel}</span>}
          {compactStatusRead && (
            <span className="property-status-pill">
              {compactStatusRead}
            </span>
          )}
          {googleRating && (
            <span className={`property-rating-pill property-rating-pill--${googleRatingTone ?? "good"}`}>
              <span aria-hidden="true">★</span> {googleRating} Google
            </span>
          )}
        </div>
        <InlinePriceRangeSignal
          area={p.area}
          pricePerSqft={p.price_per_sqft}
          properties={marketProperties}
        />
      </section>

      <PropertyPhotoMosaic
        title={displayTitle}
        societyName={society?.name}
        heroImage={p.hero_image}
        images={p.images}
        societyId={p.society_id}
      />

      <main className="property-clean-flow">
        <section className="property-map-section" aria-label="Around this home">
          {showNearbyPlate && aroundThisHomeContext && (
            <AroundThisHomePlate propertyId={id} context={aroundThisHomeContext} />
          )}
        </section>

        {(showApproachTrail || showProjectChecks) && (
          <section className="property-popup-row" aria-label="Home details">
            {showApproachTrail && (
              <ApproachRoadTrail propertyId={id} sections={detailEvidenceSections} variant="compact" />
            )}
            {projectChecks && (
              <PopupActionButton
                label={projectChecks.tileLabel}
                tone={projectChecks.tone}
                onClick={() => setProjectChecksOpen(true)}
              />
            )}
          </section>
        )}

        <PropertySignalPills signals={data.detail_signals} />

        <GoogleReviewsSection data={data} reviewSections={reviewsSections} />

        <NearbyHomesRail items={nearbyItems} status={recommendationStatus} />

        <MicroMarketTracker
          currentArea={p.area}
          properties={marketProperties}
          areas={microAreas}
          onSelectArea={handleAreaSelect}
        />
      </main>

      {projectChecksOpen && projectChecks && (
        <CleanDialog title="RERA" onClose={() => setProjectChecksOpen(false)}>
          <ProjectChecksContent summary={projectChecks} propertyId={p.id} />
        </CleanDialog>
      )}
    </div>
  );
}
