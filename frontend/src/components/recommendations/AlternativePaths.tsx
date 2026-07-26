import { Link } from "react-router-dom";
import type {
  PropertyCard,
  RecommendationBranch,
  RecommendationLens,
  RecommendationStatus,
} from "../../lib/types.ts";
import { ImageWithFallback } from "../ImageWithFallback.tsx";
import { usePropertySceneImages } from "../../hooks/usePropertySceneImages.ts";
import { propertySceneImageAt } from "../../lib/propertyScene.ts";
import {
  BuildingIcon,
  RupeeIcon,
  SealIcon,
  TrainIcon,
} from "../evidence/EvidenceIcons.tsx";

type RecommendationItem =
  | { kind: "branch"; id: string; property: PropertyCard; branch: RecommendationBranch }
  | { kind: "nearby"; id: string; property: PropertyCard };

function formatPrice(price: number): string {
  if (price >= 1_00_00_000) return `₹${(price / 1_00_00_000).toFixed(2)} Cr`;
  if (price >= 1_00_000) return `₹${(price / 1_00_000).toFixed(1)} L`;
  return `₹${price.toLocaleString("en-IN")}`;
}

const LENS_META: Record<
  RecommendationLens,
  { spine: string; icon: typeof SealIcon; gainLabel: string }
> = {
  proof: { spine: "trust", icon: SealIcon, gainLabel: "More proof" },
  value: { spine: "value", icon: RupeeIcon, gainLabel: "Better value" },
  trust: { spine: "trust", icon: BuildingIcon, gainLabel: "Safer file" },
  commute: { spine: "commute", icon: TrainIcon, gainLabel: "Closer commute" },
};

function reviewStrength(property: PropertyCard): number {
  const rating = property.google_rating ?? 0;
  const reviewCount = property.google_review_count ?? 0;
  if (rating <= 0 || reviewCount <= 0) return 0;
  return rating * 100 + Math.log10(reviewCount + 1) * 12;
}

function rankedItems(branches: RecommendationBranch[], nearby: PropertyCard[]): RecommendationItem[] {
  const usedIds = new Set(branches.map((branch) => branch.property.id));
  const branchItems: RecommendationItem[] = branches.map((branch) => ({
    kind: "branch",
    id: `${branch.branch_id}-${branch.property.id}`,
    property: branch.property,
    branch,
  }));
  const nearbyItems: RecommendationItem[] = nearby
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
    .slice(0, 6);
}

function RecommendationCard({
  property,
  badge,
  note,
  spine = "nearby",
  sceneIndex,
}: {
  property: PropertyCard;
  badge?: string;
  note?: string;
  spine?: string;
  sceneIndex: number;
}) {
  const { images } = usePropertySceneImages({
    heroImage: property.hero_image,
    societyId: property.kg_entity_refs?.society_entity_id,
  });
  const cardImage = propertySceneImageAt(images, sceneIndex, property.hero_image);
  const Icon = badge
    ? Object.values(LENS_META).find((meta) => meta.gainLabel === badge)?.icon
    : undefined;
  const meta = [
    property.society_name,
    property.area,
    `${property.bhk} BHK`,
  ].filter(Boolean).join(" · ");

  return (
    <article className={`catalog-card alt-paths__card alt-paths__card--${spine}`}>
      <Link to={`/property/${property.id}`} className="catalog-card__link">
        <div className="catalog-card__media alt-paths__media">
          <ImageWithFallback
            src={cardImage}
            alt={property.title}
            className="catalog-card__image alt-paths__image"
            loading="lazy"
          />
          <span className="alt-paths__vignette" aria-hidden="true" />
          <span className="alt-paths__grain" aria-hidden="true" />
          {badge && (
            <span className="catalog-card__kicker alt-paths__badge">
              {Icon ? <Icon size={12} /> : null}
              {badge}
            </span>
          )}
        </div>
        <div className="catalog-card__caption alt-paths__caption">
          <h3 className="catalog-card__title">{property.title}</h3>
          <p className="catalog-card__meta">{meta}</p>
          <div className="catalog-card__foot alt-paths__foot">
            <span className="catalog-card__price">{formatPrice(property.price)}</span>
            {note ? <span className="alt-paths__note">{note}</span> : null}
          </div>
        </div>
      </Link>
    </article>
  );
}

export function AlternativePaths({
  branches,
  nearby = [],
  status = "ready",
  runtimeLabel,
}: {
  branches: RecommendationBranch[];
  nearby?: PropertyCard[];
  status?: RecommendationStatus;
  runtimeLabel?: string;
}) {
  const items = rankedItems(branches, nearby);
  const total = items.length;
  const societySceneCounts = new Map<string, number>();
  const sceneIndexes = new Map<string, number>();
  for (const item of items) {
    const societyKey = item.property.kg_entity_refs?.society_entity_id
      ?? item.property.society_name
      ?? item.property.id;
    const sceneIndex = societySceneCounts.get(societyKey) ?? 0;
    sceneIndexes.set(item.id, sceneIndex);
    societySceneCounts.set(societyKey, sceneIndex + 1);
  }

  if (total === 0 && status !== "pending") return null;

  return (
    <section className="alt-paths" title={runtimeLabel} aria-label="Homes that may interest you">
      <div className="property-section-heading">
        <span>Continue exploring</span>
        <h2>May interest you</h2>
      </div>

      <div className="results-grid alt-paths__grid">
        {status === "pending" && total === 0 && (
          <>
            <div className="alt-paths__skeleton" aria-hidden="true" />
            <div className="alt-paths__skeleton" aria-hidden="true" />
            <div className="alt-paths__skeleton" aria-hidden="true" />
          </>
        )}
        {items.map((item) => (
          item.kind === "branch" ? (
            <RecommendationCard
              key={item.id}
              property={item.property}
              badge={LENS_META[item.branch.lens].gainLabel}
              note={item.branch.contrast}
              spine={LENS_META[item.branch.lens].spine}
              sceneIndex={sceneIndexes.get(item.id) ?? 0}
            />
          ) : (
            <RecommendationCard
              key={item.id}
              property={item.property}
              note={`Same area · ${item.property.bhk} BHK`}
              sceneIndex={sceneIndexes.get(item.id) ?? 0}
            />
          )
        ))}
      </div>
    </section>
  );
}
