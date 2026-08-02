import { useMemo, useState } from "react";
import type { ReactNode } from "react";
import { Link } from "react-router-dom";
import { LivingEvidenceTile } from "./evidence/LivingEvidenceTile.tsx";
import { propertyDetailPath } from "../lib/api.ts";
import { filterListableProperties, uniqueSocietiesForDiscovery } from "../lib/property-filters.ts";
import type { PropertyCard } from "../lib/types.ts";
import { useLandingSceneController } from "../hooks/useLandingSceneController.ts";

const FEATURED_LIMIT = 6;
const STORY_SCENE_IDS = ["search", "nearby", "proof", "decide"] as const;
const JOURNEY_QUERY = "Quiet 3BHK under 2.5Cr with strong reviews";

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

function storyHomesForSearch(properties: PropertyCard[]): PropertyCard[] {
  return rankHomesForLens(properties, "family")
    .map((property, index) => ({
      property,
      index,
      score: (property.bhk === 3 ? 4 : 0)
        + (hasKnownNumber(property.price) && property.price <= 25_000_000 ? 3 : 0)
        + (hasKnownNumber(property.google_rating) ? 2 : 0),
    }))
    .sort((left, right) => right.score - left.score || left.index - right.index)
    .map(({ property }) => property)
    .slice(0, 3);
}

function searchReasons(property: PropertyCard): string[] {
  const reasons: string[] = [];
  if (hasKnownNumber(property.metro_distance_mins)) reasons.push(`${property.metro_distance_mins} min metro`);
  if (hasKnownNumber(property.google_rating)) reasons.push(`Google ${property.google_rating.toFixed(1)}`);
  if (hasKnownNumber(property.open_space_pct)) reasons.push(`${Math.round(property.open_space_pct)}% open space`);
  return reasons.slice(0, 2);
}

function projectRecords(property: PropertyCard): EvidenceFact[] {
  const records: EvidenceFact[] = [];
  const registration = property.decision_check_summary?.registrationNumberCompact;
  const projectState = isKnownText(property.home_state_display)
    ? property.home_state_display
    : isKnownText(property.project_status_display)
      ? property.project_status_display
      : null;

  if (isKnownText(registration)) {
    records.push({ id: "registration", label: "Registration", value: registration });
  }
  if (projectState) records.push({ id: "state", label: "Project", value: projectState });
  if (hasKnownNumber(property.society_land_acres)) {
    records.push({ id: "land", label: "Land", value: `${Math.round(property.society_land_acres)} acres` });
  }
  if (hasKnownNumber(property.open_space_pct)) {
    records.push({ id: "open-space", label: "Open space", value: `${Math.round(property.open_space_pct)}%` });
  }
  if (isKnownText(property.builder_delivery_display)) {
    records.push({ id: "builder", label: "Builder", value: property.builder_delivery_display });
  }

  return records.slice(0, 3);
}

function evidenceScore(property: PropertyCard): number {
  return projectRecords(property).length
    + Number(hasKnownNumber(property.metro_distance_mins))
    + Number(hasKnownNumber(property.google_rating));
}

function selectEvidenceHome(properties: PropertyCard[]): PropertyCard {
  return [...properties].sort((left, right) => evidenceScore(right) - evidenceScore(left))[0];
}

function selectNearbyHome(properties: PropertyCard[]): PropertyCard {
  return [...properties].sort((left, right) => {
    return Number(hasKnownNumber(right.metro_distance_mins)) - Number(hasKnownNumber(left.metro_distance_mins))
      || evidenceScore(right) - evidenceScore(left);
  })[0];
}

function JourneyOverview({
  homes,
  onSearch,
}: {
  homes: PropertyCard[];
  onSearch: (query: string) => void;
}) {
  const rankedHomes = storyHomesForSearch(homes).slice(0, 2);
  const focusHome = rankedHomes[0] ?? homes[0];
  const records = projectRecords(focusHome).slice(0, 2);

  return (
    <section className="landing-overview" aria-labelledby="landing-overview-title">
      <div className="landing-overview__sticky">
        <div className="landing-overview__media">
          <div className="landing-overview__identity">
            <h2 id="landing-overview-title">From one search to a confident decision.</h2>
            <p>Homes, nearby context, project proof and the final tradeoffs—kept in one journey.</p>
          </div>

          <div className="landing-journey">
            <div className="landing-journey__query">
              <svg width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true">
                <circle cx="11" cy="11" r="7" />
                <path d="m20 20-4-4" />
              </svg>
              <span>{JOURNEY_QUERY}</span>
            </div>

            <ol className="landing-journey__route" aria-label="OpenEstates journey">
              <li>Intent</li>
              <li>Homes</li>
              <li>Nearby</li>
              <li>Proof</li>
              <li>Decide</li>
            </ol>

            <div className="landing-journey__panels">
              <div className="landing-journey__results">
                <span className="landing-journey__label">Ranked homes</span>
                {rankedHomes.map((property, index) => {
                  const reasons = searchReasons(property);
                  return (
                    <Link key={property.id} to={propertyDetailPath(property.id)}>
                      <span>{index + 1}</span>
                      <div>
                        <strong>{homeName(property)}</strong>
                        <small>{[property.bhk > 0 ? `${property.bhk} BHK` : null, formatPrice(property.price) || null].filter(Boolean).join(" · ")}</small>
                      </div>
                      {index === 0 && reasons[0] ? <em>{reasons[0]}</em> : null}
                    </Link>
                  );
                })}
              </div>

              <div className="landing-journey__nearby" aria-label="Nearby context">
                <span className="landing-journey__nearby-label">Nearby context</span>
                <strong className="landing-journey__area">{focusHome.area}</strong>
                <span className="landing-journey__road landing-journey__road--one" aria-hidden="true" />
                <span className="landing-journey__road landing-journey__road--two" aria-hidden="true" />
                <span className="landing-journey__radius" aria-hidden="true" />
                <span className="landing-journey__home-pin" aria-hidden="true" />
                {hasKnownNumber(focusHome.metro_distance_mins) ? (
                  <span className="landing-journey__metro">Metro · {focusHome.metro_distance_mins} min</span>
                ) : null}
              </div>

              <div className="landing-journey__proof">
                <span className="landing-journey__label">Project proof</span>
                {records.map((record) => (
                  <div key={record.id}>
                    <span>{record.label}</span>
                    <strong>{record.value}</strong>
                  </div>
                ))}
                <div className="landing-journey__decision">
                  <span>Notebook · Compare · Plan</span>
                </div>
              </div>
            </div>
          </div>

          <button type="button" className="landing-overview__open" onClick={() => onSearch(JOURNEY_QUERY)}>
            Try this journey <span aria-hidden="true">→</span>
          </button>
        </div>
      </div>
    </section>
  );
}

function SearchCanvas({ homes }: { homes: PropertyCard[] }) {
  const focusHome = homes[0];
  if (!focusHome) return null;
  const reasons = searchReasons(focusHome);

  return (
    <div className="landing-product landing-product--resolve">
      <p className="landing-resolve__query">
        <span>Quiet 3BHK,</span> <span>under 2.5Cr,</span> <span>with strong reviews</span>
      </p>
      <div className="landing-resolve__intents" aria-label="Search preferences">
        <span>3 BHK</span>
        <span>Under ₹2.5 Cr</span>
        <span>Strong reviews</span>
      </div>
      <div className="landing-resolve__homes">
        {homes.map((property, index) => {
          const meta = [
            property.bhk > 0 ? `${property.bhk} BHK` : null,
            formatPrice(property.price) || null,
          ].filter((value): value is string => Boolean(value));

          return (
            <article key={property.id} className={`landing-resolve__home${index === 0 ? " is-focus" : ""}`}>
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

function NearbyCanvas({ property }: { property: PropertyCard }) {
  return (
    <div className="landing-product landing-product--nearby">
      <header className="landing-nearby__home">
        <strong>{homeName(property)}</strong>
        <span>{property.area}</span>
      </header>
      <div className="landing-nearby__map">
        <span className="landing-nearby__road landing-nearby__road--one" aria-hidden="true" />
        <span className="landing-nearby__road landing-nearby__road--two" aria-hidden="true" />
        <span className="landing-nearby__radius" aria-hidden="true" />
        <span className="landing-nearby__pin" aria-hidden="true" />
        {hasKnownNumber(property.metro_distance_mins) ? (
          <span className="landing-nearby__place">
            <i aria-hidden="true" />
            Metro
            <strong>{property.metro_distance_mins} min</strong>
          </span>
        ) : null}
      </div>
    </div>
  );
}

function ProofCanvas({ property }: { property: PropertyCard }) {
  const records = projectRecords(property);

  return (
    <div className="landing-product landing-product--proof">
      <header className="landing-proof__home">
        <strong>{homeName(property)}</strong>
        <span>{property.area}</span>
      </header>
      <div className="landing-proof__timeline">
        {records.map((record) => (
          <div key={record.id} className="landing-proof__record">
            <i aria-hidden="true" />
            <span>{record.label}</span>
            <strong>{record.value}</strong>
          </div>
        ))}
      </div>
      {hasKnownNumber(property.google_rating) ? (
        <div className="landing-proof__resident">
          <span>Resident view</span>
          <strong>
            Google {property.google_rating.toFixed(1)}
            {hasKnownNumber(property.google_review_count)
              ? ` · ${property.google_review_count.toLocaleString("en-IN")} reviews`
              : ""}
          </strong>
        </div>
      ) : null}
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

function DecisionCanvas({ homes }: { homes: PropertyCard[] }) {
  const [left, right] = homes;
  if (!left || !right) return null;
  const rows = comparisonRows(left, right);

  return (
    <div className="landing-product landing-product--converge">
      <div className="landing-converge__steps" aria-label="Decision workspace">
        <span>Notebook</span>
        <i aria-hidden="true">→</i>
        <span>Compare</span>
        <i aria-hidden="true">→</i>
        <span>Plan</span>
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
  title: string;
  description: string;
  action: ReactNode;
  canvas: ReactNode;
  controller: ReturnType<typeof useLandingSceneController>;
};

function StoryScene({
  id,
  title,
  description,
  action,
  canvas,
  controller,
}: StorySceneProps) {
  const isActive = controller.activeSceneId === id;
  const hasEntered = controller.hasEntered(id);
  const sceneClassName = [
    "landing-scene",
    `landing-scene--${id}`,
    isActive ? "is-active" : "",
    hasEntered ? "has-entered" : "",
  ].filter(Boolean).join(" ");

  return (
    <article
      ref={controller.sceneRef(id)}
      className={sceneClassName}
      data-scene-id={id}
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

  const searchHomes = storyHomesForSearch(uniqueHomes);
  const compareHomes = searchHomes.length >= 2 ? searchHomes.slice(0, 2) : uniqueHomes.slice(0, 2);
  const searchHomeIds = new Set(searchHomes.map((home) => home.id));
  const nearbyCandidates = uniqueHomes.filter((home) => !searchHomeIds.has(home.id));
  const nearbyHome = selectNearbyHome(nearbyCandidates.length > 0 ? nearbyCandidates : uniqueHomes);
  const proofCandidates = uniqueHomes.filter((home) => (
    home.id !== nearbyHome.id && !compareHomes.some((compareHome) => compareHome.id === home.id)
  ));
  const evidenceHome = selectEvidenceHome(proofCandidates.length > 0 ? proofCandidates : uniqueHomes);

  return (
    <section
      className="landing-stage"
      aria-label="How OpenEstates helps you decide"
      data-reduced-motion={controller.isReducedMotion ? "true" : "false"}
    >
      <FeaturedSuggestions properties={uniqueHomes} onSearch={onSearch} />
      <JourneyOverview homes={uniqueHomes} onSearch={onSearch} />

      <div className="landing-stage__chapters">
        <div className="landing-stage__chapters-head">
          <h2>Four parts. One clear decision.</h2>
          <p>Each step keeps the next question close.</p>
        </div>

        <div className="landing-stage__story">
          <StoryScene
            id="search"
            title="Search in your own words"
            description="Ask for the life around the home, not a stack of filters."
            action={(
              <button type="button" onClick={() => onSearch(JOURNEY_QUERY)}>
                Try this search <span aria-hidden="true">→</span>
              </button>
            )}
            canvas={<SearchCanvas homes={searchHomes} />}
            controller={controller}
          />

          <StoryScene
            id="nearby"
            title="Understand what is nearby"
            description="Put the home in context, with distances that matter to your routine."
            action={(
              <Link to={propertyDetailPath(nearbyHome.id)}>
                Explore this home <span aria-hidden="true">→</span>
              </Link>
            )}
            canvas={<NearbyCanvas property={nearbyHome} />}
            controller={controller}
          />

          <StoryScene
            id="proof"
            title="Read the project record"
            description="Project facts and resident signals stay close to the decision."
            action={(
              <Link to={propertyDetailPath(evidenceHome.id)}>
                See the proof <span aria-hidden="true">→</span>
              </Link>
            )}
            canvas={<ProofCanvas property={evidenceHome} />}
            controller={controller}
          />

          {compareHomes.length >= 2 ? (
            <StoryScene
              id="decide"
              title="Keep the decision together"
              description="Notes, comparisons and the financial plan stay in one calm workspace."
              action={(
                <Link to="/workspace/compare">
                  Open workspace <span aria-hidden="true">→</span>
                </Link>
              )}
              canvas={<DecisionCanvas homes={compareHomes} />}
              controller={controller}
            />
          ) : null}
        </div>
      </div>
    </section>
  );
}
