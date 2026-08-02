import { useMemo, useState } from "react";
import type { FocusEvent, ReactNode } from "react";
import { Link } from "react-router-dom";
import { LivingEvidenceTile } from "./evidence/LivingEvidenceTile.tsx";
import { propertyDetailPath } from "../lib/api.ts";
import { filterListableProperties, uniqueSocietiesForDiscovery } from "../lib/property-filters.ts";
import type { PropertyCard } from "../lib/types.ts";
import { useLandingSceneController } from "../hooks/useLandingSceneController.ts";

const FEATURED_LIMIT = 6;
const STORY_SCENE_IDS = ["resolve", "reveal", "converge"] as const;
const RESOLVE_QUERY = "3BHK under 2.5Cr, near metro, with strong reviews";

type StorySceneId = typeof STORY_SCENE_IDS[number];
type FeaturedLensId = "metro" | "family" | "township" | "feedback";

type FeaturedLens = {
  id: FeaturedLensId;
  label: string;
  query: string;
};

type EvidenceFact = {
  id: string;
  label: string;
  value: string;
};

type ComparisonWinner = "left" | "right" | "tie";

type ComparisonRow = {
  id: string;
  label: string;
  left: string;
  right: string;
  winner: ComparisonWinner;
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
};

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

function formatPrice(price: number): string {
  if (!hasKnownNumber(price)) return "";
  if (price >= 10_000_000) return `₹${(price / 10_000_000).toFixed(1)} Cr`;
  if (price >= 100_000) return `₹${(price / 100_000).toFixed(1)} L`;
  return `₹${price.toLocaleString("en-IN")}`;
}

function homeName(property: PropertyCard): string {
  return isKnownText(property.society_name) ? property.society_name : property.title;
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

function FeaturedSuggestions({
  properties,
  onSearch,
}: {
  properties: PropertyCard[];
  onSearch: (query: string) => void;
}) {
  const [activeLensId, setActiveLensId] = useState<FeaturedLensId>("metro");
  const activeLens = FEATURED_LENSES.find((lens) => lens.id === activeLensId) ?? FEATURED_LENSES[0];
  const suggestions = useMemo(
    () => rankHomesForLens(properties, activeLensId).slice(0, FEATURED_LIMIT),
    [activeLensId, properties],
  );

  if (suggestions.length === 0) return null;

  return (
    <section className="landing-featured" aria-labelledby="landing-featured-title">
      <div className="landing-featured__head">
        <h2 id="landing-featured-title">A few homes with clear reasons</h2>
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

      <div className="landing-stage__featured">
        {suggestions.map((property) => (
          <div key={property.id} className="landing-stage__feature-card">
            <LivingEvidenceTile
              property={property}
              variant="browse"
              matchLabels={matchLabels(property, activeLensId)}
            />
          </div>
        ))}
      </div>
    </section>
  );
}

function storyHomesForResolve(properties: PropertyCard[]): PropertyCard[] {
  return rankHomesForLens(properties, "family")
    .map((property, index) => ({
      property,
      index,
      score: (property.bhk === 3 ? 4 : 0)
        + (hasKnownNumber(property.price) && property.price <= 25_000_000 ? 3 : 0)
        + (hasKnownNumber(property.metro_distance_mins) ? 2 : 0)
        + (hasKnownNumber(property.google_rating) ? 1 : 0),
    }))
    .sort((left, right) => right.score - left.score || left.index - right.index)
    .map(({ property }) => property)
    .slice(0, 3);
}

function resolveReasons(property: PropertyCard): string[] {
  const reasons: string[] = [];
  if (hasKnownNumber(property.metro_distance_mins)) reasons.push(`${property.metro_distance_mins} min metro`);
  if (hasKnownNumber(property.google_rating)) reasons.push(`Google ${property.google_rating.toFixed(1)}`);
  if (hasKnownNumber(property.open_space_pct)) reasons.push(`${Math.round(property.open_space_pct)}% open space`);
  return reasons.slice(0, 2);
}

function ResolveCanvas({ homes }: { homes: PropertyCard[] }) {
  const focusHome = homes[0];
  if (!focusHome) return null;
  const reasons = resolveReasons(focusHome);

  return (
    <div className="landing-product landing-product--resolve">
      <p className="landing-resolve__query">
        <span>3BHK under 2.5Cr,</span> <span>near metro,</span> <span>with strong reviews</span>
      </p>
      <div className="landing-resolve__intents" aria-label="Search preferences">
        <span>3 BHK</span>
        <span>Under ₹2.5 Cr</span>
        <span>Metro</span>
        <span>Reviews</span>
      </div>
      <div className="landing-resolve__homes">
        {homes.map((property, index) => {
          const meta = [
            property.bhk > 0 ? `${property.bhk} BHK` : null,
            formatPrice(property.price) || null,
          ].filter((value): value is string => Boolean(value));

          return (
            <article
              key={property.id}
              className={`landing-resolve__home${index === 0 ? " is-focus" : ""}`}
            >
              <strong>{homeName(property)}</strong>
              {meta.length > 0 ? <span>{meta.join(" · ")}</span> : null}
              {index === 0 && reasons.length > 0 ? (
                <div className="landing-resolve__reasons">
                  {reasons.map((reason) => <em key={reason}>{reason}</em>)}
                </div>
              ) : null}
            </article>
          );
        })}
      </div>
      <Link className="landing-resolve__why" to={propertyDetailPath(focusHome.id)}>
        Why this home <span aria-hidden="true">→</span>
      </Link>
    </div>
  );
}

function evidenceScore(property: PropertyCard): number {
  return [
    hasKnownNumber(property.metro_distance_mins),
    hasKnownNumber(property.open_space_pct),
    hasKnownNumber(property.society_land_acres),
    hasKnownNumber(property.google_rating),
    hasKnownNumber(property.google_review_count),
    isKnownText(property.project_status_display),
    isKnownText(property.home_state_display),
  ].filter(Boolean).length;
}

function selectEvidenceHome(properties: PropertyCard[]): PropertyCard {
  return [...properties].sort((left, right) => evidenceScore(right) - evidenceScore(left))[0];
}

function evidenceFacts(property: PropertyCard): EvidenceFact[] {
  const facts: EvidenceFact[] = [];
  const projectState = isKnownText(property.home_state_display)
    ? property.home_state_display
    : isKnownText(property.project_status_display)
      ? property.project_status_display
      : null;

  if (projectState) facts.push({ id: "state", label: "Home", value: projectState });
  if (hasKnownNumber(property.metro_distance_mins)) {
    facts.push({ id: "metro", label: "Metro", value: `${property.metro_distance_mins} min` });
  }
  if (hasKnownNumber(property.open_space_pct)) {
    facts.push({ id: "open-space", label: "Open space", value: `${Math.round(property.open_space_pct)}%` });
  } else if (hasKnownNumber(property.society_land_acres)) {
    facts.push({ id: "land", label: "Township", value: `${Math.round(property.society_land_acres)} acres` });
  }

  return facts.slice(0, 3);
}

function RevealCanvas({ property }: { property: PropertyCard }) {
  const facts = evidenceFacts(property);
  const hasResidentSignal = hasKnownNumber(property.google_rating);

  return (
    <div className="landing-product landing-product--reveal">
      <header className="landing-reveal__home">
        <strong>{homeName(property)}</strong>
        <span>{property.area}</span>
      </header>

      <div className="landing-reveal__layout">
        <div className="landing-reveal__field" aria-hidden="true">
          <span className="landing-reveal__route landing-reveal__route--one" />
          <span className="landing-reveal__route landing-reveal__route--two" />
          <span className="landing-reveal__radius" />
          <span className="landing-reveal__pin" />
          <span className="landing-reveal__marker landing-reveal__marker--one" />
          <span className="landing-reveal__marker landing-reveal__marker--two" />
          <span className="landing-reveal__marker landing-reveal__marker--three" />
        </div>

        <div className="landing-reveal__dossier">
          <ul>
            {facts.map((fact) => (
              <li key={fact.id}>
                <span>{fact.label}</span>
                <strong>{fact.value}</strong>
              </li>
            ))}
          </ul>
          {hasResidentSignal ? (
            <div className="landing-reveal__resident">
              <span>Resident signal</span>
              <strong>
                Google {property.google_rating?.toFixed(1)}
                {hasKnownNumber(property.google_review_count)
                  ? ` · ${property.google_review_count.toLocaleString("en-IN")} reviews`
                  : ""}
              </strong>
            </div>
          ) : null}
        </div>
      </div>
    </div>
  );
}

function pickWinner(left: number, right: number, direction: "lower" | "higher"): ComparisonWinner {
  if (left === right) return "tie";
  if (direction === "lower") return left < right ? "left" : "right";
  return left > right ? "left" : "right";
}

function comparisonRows(left: PropertyCard, right: PropertyCard): ComparisonRow[] {
  const rows: ComparisonRow[] = [];

  if (hasKnownNumber(left.price) && hasKnownNumber(right.price)) {
    rows.push({
      id: "price",
      label: "Price",
      left: formatPrice(left.price),
      right: formatPrice(right.price),
      winner: pickWinner(left.price, right.price, "lower"),
    });
  }
  if (hasKnownNumber(left.sqft) && hasKnownNumber(right.sqft)) {
    rows.push({
      id: "space",
      label: "Space",
      left: `${left.sqft.toLocaleString("en-IN")} sqft`,
      right: `${right.sqft.toLocaleString("en-IN")} sqft`,
      winner: pickWinner(left.sqft, right.sqft, "higher"),
    });
  }
  if (hasKnownNumber(left.metro_distance_mins) && hasKnownNumber(right.metro_distance_mins)) {
    rows.push({
      id: "metro",
      label: "Metro",
      left: `${left.metro_distance_mins} min`,
      right: `${right.metro_distance_mins} min`,
      winner: pickWinner(left.metro_distance_mins, right.metro_distance_mins, "lower"),
    });
  }
  if (hasKnownNumber(left.google_rating) && hasKnownNumber(right.google_rating)) {
    rows.push({
      id: "reviews",
      label: "Google",
      left: left.google_rating.toFixed(1),
      right: right.google_rating.toFixed(1),
      winner: pickWinner(left.google_rating, right.google_rating, "higher"),
    });
  }

  return rows.slice(0, 4);
}

function comparisonCellClass(winner: ComparisonWinner, side: "left" | "right"): string {
  return winner === side ? " is-stronger" : "";
}

function ConvergeCanvas({ homes }: { homes: PropertyCard[] }) {
  const [left, right] = homes;
  if (!left || !right) return null;
  const rows = comparisonRows(left, right);

  return (
    <div className="landing-product landing-product--converge">
      <div className="landing-converge__notes" aria-hidden="true">
        <span>Commute</span>
        <span>Space</span>
        <span>Reviews</span>
      </div>

      <div className="landing-converge__table">
        <div className="landing-converge__homes">
          <span aria-hidden="true" />
          <strong>{homeName(left)}</strong>
          <strong>{homeName(right)}</strong>
        </div>
        {rows.map((row) => (
          <div key={row.id} className="landing-converge__row">
            <span>{row.label}</span>
            <strong className={comparisonCellClass(row.winner, "left")}>{row.left}</strong>
            <strong className={comparisonCellClass(row.winner, "right")}>{row.right}</strong>
          </div>
        ))}
      </div>

      <div className="landing-converge__plan">
        <div>
          <span>Buy or rent</span>
          <strong>See the tradeoff over time</strong>
        </div>
        <svg viewBox="0 0 180 54" aria-hidden="true">
          <path className="is-buy" d="M4 46 C40 44, 64 36, 92 25 C120 14, 146 10, 176 6" />
          <path className="is-rent" d="M4 38 C42 36, 76 33, 108 26 C140 19, 158 16, 176 13" />
        </svg>
        <Link to={`/property/${left.id}/plan`}>Open plan <span aria-hidden="true">→</span></Link>
      </div>
    </div>
  );
}

type StorySceneProps = {
  id: StorySceneId;
  side: "left" | "right";
  title: string;
  description: string;
  action: ReactNode;
  canvas: ReactNode;
  controller: ReturnType<typeof useLandingSceneController>;
};

function StoryScene({
  id,
  side,
  title,
  description,
  action,
  canvas,
  controller,
}: StorySceneProps) {
  const isActive = controller.activeSceneId === id;
  const hasEntered = controller.hasEntered(id);
  const isPaused = controller.isPaused(id);

  const handleBlur = (event: FocusEvent<HTMLElement>) => {
    if (!event.currentTarget.contains(event.relatedTarget as Node | null)) {
      controller.resumeScene(id);
    }
  };

  const sceneClassName = [
    "landing-scene",
    `landing-scene--${id}`,
    `landing-scene--canvas-${side}`,
    isActive ? "is-active" : "",
    hasEntered ? "has-entered" : "",
    isPaused ? "is-paused" : "",
  ].filter(Boolean).join(" ");

  return (
    <article
      ref={controller.sceneRef(id)}
      className={sceneClassName}
      data-scene-id={id}
      onPointerEnter={() => controller.pauseScene(id)}
      onPointerLeave={() => controller.resumeScene(id)}
      onFocusCapture={() => controller.pauseScene(id)}
      onBlurCapture={handleBlur}
    >
      <div className="landing-scene__copy">
        <h2>{title}</h2>
        <p>{description}</p>
        <div className="landing-scene__action">{action}</div>
      </div>
      <div className="landing-scene__canvas-wrap">
        <div className="landing-canvas">{canvas}</div>
      </div>
    </article>
  );
}

export function LandingStoryStage({ properties, onSearch }: LandingStoryStageProps) {
  const listable = filterListableProperties(properties);
  const uniqueHomes = uniqueSocietiesForDiscovery(listable);
  const controller = useLandingSceneController(STORY_SCENE_IDS);

  if (uniqueHomes.length === 0) return null;

  const resolveHomes = storyHomesForResolve(uniqueHomes);
  const revealHome = selectEvidenceHome(uniqueHomes);
  const compareHomes = resolveHomes.length >= 2 ? resolveHomes.slice(0, 2) : uniqueHomes.slice(0, 2);

  return (
    <section
      className="landing-stage"
      aria-label="How OpenEstates helps you decide"
      data-reduced-motion={controller.isReducedMotion ? "true" : "false"}
    >
      <FeaturedSuggestions properties={uniqueHomes} onSearch={onSearch} />

      <div className="landing-stage__story">
        <StoryScene
          id="resolve"
          side="right"
          title="Ranked for your life"
          description="Your request becomes a small set of homes, with the strongest reasons kept in view."
          action={(
            <button type="button" onClick={() => onSearch(RESOLVE_QUERY)}>
              Try this search <span aria-hidden="true">→</span>
            </button>
          )}
          canvas={<ResolveCanvas homes={resolveHomes} />}
          controller={controller}
        />

        <StoryScene
          id="reveal"
          side="left"
          title="See what listings leave out"
          description="Project context and resident signals settle around the home, so the tradeoff is visible before a visit."
          action={(
            <Link to={propertyDetailPath(revealHome.id)}>
              See the full picture <span aria-hidden="true">→</span>
            </Link>
          )}
          canvas={<RevealCanvas property={revealHome} />}
          controller={controller}
        />

        {compareHomes.length >= 2 ? (
          <StoryScene
            id="converge"
            side="right"
            title="Compare and decide"
            description="Saved homes, visit notes and the financial horizon come together in one calm workspace."
            action={(
              <Link to="/workspace/compare">
                Open workspace <span aria-hidden="true">→</span>
              </Link>
            )}
            canvas={<ConvergeCanvas homes={compareHomes} />}
            controller={controller}
          />
        ) : null}
      </div>
    </section>
  );
}
