import { useMemo, useState } from "react";
import type { FocusEvent, ReactNode } from "react";
import { Link } from "react-router-dom";
import { LivingEvidenceTile } from "./evidence/LivingEvidenceTile.tsx";
import { propertyDetailPath } from "../lib/api.ts";
import { filterListableProperties, uniqueSocietiesForDiscovery } from "../lib/property-filters.ts";
import type { PropertyCard } from "../lib/types.ts";
import { useLandingSceneController } from "../hooks/useLandingSceneController.ts";
import { useLandingResolveSequence } from "../hooks/useLandingResolveSequence.ts";
import { useLandingStoryMotion } from "../hooks/useLandingStoryMotion.ts";

const FEATURED_LIMIT = 6;
const STORY_SCENE_IDS = ["resolve", "reveal", "remember", "converge", "record"] as const;
const RESOLVE_QUERY = "3BHK under 2Cr near metro with strong reviews";

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
              allowSave
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
        + (hasKnownNumber(property.price) && property.price <= 20_000_000 ? 3 : 0)
        + (hasKnownNumber(property.open_space_pct) ? 2 : 0)
        + (hasKnownNumber(property.google_rating) ? 1 : 0)
        + (hasKnownNumber(property.google_review_count) && property.google_review_count >= 500 ? 2 : 0),
    }))
    .sort((left, right) => right.score - left.score || left.index - right.index)
    .map(({ property }) => property)
    .slice(0, 3);
}

function resolveReasons(property: PropertyCard): string[] {
  const reasons: string[] = [];
  if (hasKnownNumber(property.google_rating)) reasons.push(`Google ${property.google_rating.toFixed(1)}`);
  if (hasKnownNumber(property.open_space_pct)) reasons.push(`${Math.round(property.open_space_pct)}% open space`);
  return reasons.slice(0, 2);
}

function StoryProgress({ activeSceneId }: { activeSceneId: string | null }) {
  const activeIndex = STORY_SCENE_IDS.findIndex((sceneId) => sceneId === activeSceneId);

  return (
    <div className="landing-story-progress" aria-hidden="true">
      <div className="landing-story-progress__body">
        <span className="landing-story-progress__track"><i /></span>
        {STORY_SCENE_IDS.map((sceneId, index) => (
          <i
            key={sceneId}
            className={[
              "landing-story-progress__marker",
              index < activeIndex ? "is-complete" : "",
              index === activeIndex ? "is-active" : "",
            ].filter(Boolean).join(" ")}
          />
        ))}
      </div>
    </div>
  );
}

function ResolveCanvas({
  active,
  homes,
  paused,
  reducedMotion,
}: {
  active: boolean;
  homes: PropertyCard[];
  paused: boolean;
  reducedMotion: boolean;
}) {
  const focusHome = homes[0];
  const sequence = useLandingResolveSequence({ active, paused, reducedMotion });
  if (!focusHome) return null;
  const reasons = resolveReasons(focusHome);

  return (
    <div
      className="landing-product landing-product--resolve"
      data-phase={sequence.phase}
      data-query-visible={sequence.queryVisible}
      data-candidates-visible={sequence.candidatesVisible}
      data-selection-visible={sequence.selectionVisible}
      data-proof-visible={sequence.proofVisible}
    >
      <div className="landing-resolve__composer">
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <circle cx="10.8" cy="10.8" r="6.2" />
          <path d="m15.4 15.4 4.1 4.1" />
        </svg>
        <p className="landing-resolve__query">
          <span>3BHK under 2Cr,</span> <span>near metro,</span> <span>with strong reviews</span>
        </p>
        <i aria-hidden="true">→</i>
      </div>
      <p className="landing-resolve__result-count">3 strongest homes</p>
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
              <span className="landing-resolve__rank">0{index + 1}</span>
              <div className="landing-resolve__home-copy">
                <strong>{homeName(property)}</strong>
                {meta.length > 0 ? <span>{meta.join(" · ")}</span> : null}
                {index === 0 && reasons.length > 0 ? (
                  <div className="landing-resolve__reasons">
                    {reasons.map((reason) => <em key={reason}>{reason}</em>)}
                  </div>
                ) : null}
              </div>
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

function rankEvidenceHomes(properties: PropertyCard[]): PropertyCard[] {
  return [...properties].sort((left, right) => evidenceScore(right) - evidenceScore(left));
}

function selectEvidenceHome(properties: PropertyCard[]): PropertyCard {
  return rankEvidenceHomes(properties)[0];
}

function evidenceFacts(property: PropertyCard): EvidenceFact[] {
  const facts: EvidenceFact[] = [];
  const checks = property.decision_check_summary;
  const primaryCheck = checks?.primaryLabels?.[0]?.label;
  const registration = checks?.registrationNumberCompact;
  const projectState = isKnownText(property.home_state_display)
    ? property.home_state_display
    : isKnownText(property.project_status_display)
      ? property.project_status_display
      : null;

  if (isKnownText(primaryCheck)) {
    facts.push({ id: "attention", label: "Watch", value: primaryCheck });
  }
  if (isKnownText(registration)) {
    facts.push({ id: "registration", label: "Registration", value: registration });
  }
  if (projectState) facts.push({ id: "state", label: "Project", value: projectState });
  if (hasKnownNumber(property.open_space_pct)) {
    facts.push({ id: "open-space", label: "Open space", value: `${Math.round(property.open_space_pct)}%` });
  } else if (hasKnownNumber(property.society_land_acres)) {
    facts.push({ id: "land", label: "Project land", value: `${Math.round(property.society_land_acres)} acres` });
  }

  return facts.slice(0, 4);
}

function RevealCanvas({ property }: { property: PropertyCard }) {
  const facts = evidenceFacts(property);
  const hasResidentSignal = hasKnownNumber(property.google_rating);
  const checkLabel = property.decision_check_summary?.tileLabel;
  const homeMeta = [
    property.bhk > 0 ? `${property.bhk} BHK` : null,
    formatPrice(property.price) || null,
  ].filter((value): value is string => Boolean(value));

  return (
    <div className="landing-product landing-product--reveal">
      <header className="landing-reveal__home">
        <div>
          <strong>{homeName(property)}</strong>
          <span>{property.area}</span>
        </div>
        {homeMeta.length > 0 ? <p>{homeMeta.join(" · ")}</p> : null}
      </header>

      <div className="landing-reveal__layout">
        <section className="landing-reveal__field">
          <header>
            <h3>Around this home</h3>
            <div aria-hidden="true">
              <span>Schools</span>
              <span>Hospitals</span>
              <span>Parks</span>
            </div>
          </header>
          <div className="landing-reveal__map" aria-hidden="true">
            <span className="landing-reveal__route landing-reveal__route--one" />
            <span className="landing-reveal__route landing-reveal__route--two" />
            <svg className="landing-reveal__metro-line" viewBox="0 0 320 220" preserveAspectRatio="none">
              <path d="M-14 56 C50 80, 70 142, 136 130 S214 64, 336 92" />
            </svg>
            <span className="landing-reveal__pin" />
            <span className="landing-reveal__marker landing-reveal__marker--one">1</span>
            <span className="landing-reveal__marker landing-reveal__marker--two">2</span>
            <span className="landing-reveal__marker landing-reveal__marker--three">3</span>
            <span className="landing-reveal__transit">M</span>
          </div>
        </section>

        <section className="landing-reveal__dossier">
          <header>
            <h3>Project checks</h3>
            {isKnownText(checkLabel) ? <span>{checkLabel}</span> : null}
          </header>
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
              <span>Resident reviews</span>
              <strong>
                Google {property.google_rating?.toFixed(1)}
                {hasKnownNumber(property.google_review_count)
                  ? ` · ${property.google_review_count.toLocaleString("en-IN")} reviews`
                  : ""}
              </strong>
            </div>
          ) : null}
        </section>
      </div>
    </div>
  );
}

function NotebookCanvas({ property }: { property: PropertyCard }) {
  return (
    <div className="landing-product landing-product--remember">
      <header className="landing-remember__home">
        <div>
          <span>Saved home</span>
          <strong>{homeName(property)}</strong>
        </div>
        <i aria-hidden="true">♥</i>
      </header>

      <div className="landing-remember__page">
        <p className="landing-remember__note">
          Peaceful campus and easy metro access. Check evening traffic before deciding.
        </p>
        <div className="landing-remember__command">
          <span>/visit</span>
          <strong>Visit checklist</strong>
        </div>
        <div className="landing-remember__checklist">
          <header>
            <strong>Visit</strong>
            <span>0 / 3 done</span>
          </header>
          <p><i aria-hidden="true" /> Check water pressure</p>
          <p><i aria-hidden="true" /> Listen for balcony traffic noise</p>
          <p><i aria-hidden="true" /> Confirm parking slot</p>
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

  if (isKnownText(left.home_state_display) && isKnownText(right.home_state_display)) {
    rows.push({
      id: "state",
      label: "Home state",
      left: left.home_state_display,
      right: right.home_state_display,
      winner: "tie",
    });
  }
  if (hasKnownNumber(left.society_land_acres) && hasKnownNumber(right.society_land_acres)) {
    rows.push({
      id: "project-land",
      label: "Project land",
      left: `${left.society_land_acres.toFixed(1)} acres`,
      right: `${right.society_land_acres.toFixed(1)} acres`,
      winner: "tie",
    });
  }
  if (hasKnownNumber(left.open_space_pct) && hasKnownNumber(right.open_space_pct)) {
    rows.push({
      id: "open-space",
      label: "Open space",
      left: `${Math.round(left.open_space_pct)}%`,
      right: `${Math.round(right.open_space_pct)}%`,
      winner: pickWinner(left.open_space_pct, right.open_space_pct, "higher"),
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
      <div className="landing-converge__notebook">
        <span>Shortlist</span>
        <strong>2 homes ready to compare</strong>
        <i aria-hidden="true" />
        <i aria-hidden="true" />
      </div>

      <div className="landing-converge__table">
        <div className="landing-converge__homes">
          <span aria-hidden="true" />
          <div>
            <strong>{homeName(left)}</strong>
            <small>{formatPrice(left.price)}</small>
          </div>
          <div>
            <strong>{homeName(right)}</strong>
            <small>{formatPrice(right.price)}</small>
          </div>
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
          <span>Buy vs Rent</span>
          <strong>Follow both paths to year 20</strong>
        </div>
        <svg viewBox="0 0 180 54" aria-hidden="true">
          <path className="is-buy" d="M4 46 C40 44, 64 36, 92 25 C120 14, 146 10, 176 6" />
          <path className="is-rent" d="M4 38 C42 36, 76 33, 108 26 C140 19, 158 16, 176 13" />
        </svg>
        <Link to={`/workspace/buy-vs-rent/${left.id}`}>Open plan <span aria-hidden="true">→</span></Link>
      </div>
    </div>
  );
}

function ReraCanvas({ property }: { property: PropertyCard }) {
  const summary = property.decision_check_summary;
  const registeredLabel = summary?.groups
    ?.flatMap((group) => group.labels)
    .find((label) => label.key === "rera_registration_available")
    ?.label;
  const cautionLabels = summary?.groups
    ?.find((group) => group.id === "attention")
    ?.labels.slice(0, 2) ?? [];
  const documentLabels = summary?.groups
    ?.find((group) => group.id === "documents")
    ?.labels.slice(0, 3) ?? [];

  return (
    <div className="landing-product landing-product--record">
      <header className="landing-record__head">
        <div>
          <span>RERA</span>
          <strong>{homeName(property)}</strong>
        </div>
        <em>{registeredLabel ?? "Registration"}</em>
      </header>

      {isKnownText(summary?.registrationNumberCompact) ? (
        <div className="landing-record__registration">
          <span>Registration</span>
          <strong>{summary?.registrationNumberCompact}</strong>
        </div>
      ) : null}

      <div className="landing-record__body">
        <section className="landing-record__documents">
          <span>Documents</span>
          {documentLabels.map((label) => (
            <p key={label.key}><i aria-hidden="true" />{label.label}</p>
          ))}
        </section>
        <section className="landing-record__checks">
          <span>Decision checks</span>
          {cautionLabels.map((label) => (
            <p key={label.key}>{label.label}</p>
          ))}
        </section>
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
  const storyRef = useLandingStoryMotion(controller.isReducedMotion);

  if (uniqueHomes.length === 0) return null;

  const resolveHomes = storyHomesForResolve(uniqueHomes);
  const revealHome = resolveHomes[0] ?? selectEvidenceHome(uniqueHomes);
  const compareHomes = resolveHomes.length >= 2
    ? resolveHomes.slice(0, 2)
    : rankEvidenceHomes(uniqueHomes).slice(0, 2);
  const resolveIsActive = controller.activeSceneId === "resolve";

  return (
    <section
      className="landing-stage"
      aria-label="A buyer journey through OpenEstates"
      data-reduced-motion={controller.isReducedMotion ? "true" : "false"}
    >
      <FeaturedSuggestions properties={uniqueHomes} onSearch={onSearch} />

      <div ref={storyRef} className="landing-stage__story">
        <StoryProgress activeSceneId={controller.activeSceneId} />
        <StoryScene
          id="resolve"
          side="right"
          title="Start with the life you want"
          description="A natural-language search becomes a small, ranked set of homes with reasons attached."
          action={(
            <button type="button" onClick={() => onSearch(RESOLVE_QUERY)}>
              Try this search <span aria-hidden="true">→</span>
            </button>
          )}
          canvas={(
            <ResolveCanvas
              key={`${resolveIsActive ? "active" : "rest"}-${controller.isReducedMotion ? "reduced" : "motion"}`}
              active={resolveIsActive}
              homes={resolveHomes}
              paused={controller.isPaused("resolve")}
              reducedMotion={controller.isReducedMotion}
            />
          )}
          controller={controller}
        />

        <StoryScene
          id="reveal"
          side="left"
          title="Open a home, not a listing"
          description="The result expands into map context, project checks and resident reviews without losing why it matched."
          action={(
            <Link to={propertyDetailPath(revealHome.id)}>
              See the full picture <span aria-hidden="true">→</span>
            </Link>
          )}
          canvas={<RevealCanvas property={revealHome} />}
          controller={controller}
        />

        <StoryScene
          id="remember"
          side="right"
          title="Keep your judgment with the home"
          description="Save the home, write what you noticed, then turn a slash command into a visit checklist."
          action={(
            <Link to="/workspace">
              Open notebook <span aria-hidden="true">→</span>
            </Link>
          )}
          canvas={<NotebookCanvas property={revealHome} />}
          controller={controller}
        />

        {compareHomes.length >= 2 ? (
          <StoryScene
            id="converge"
            side="left"
            title="Make the tradeoffs visible"
            description="Put two saved homes side by side, then carry the stronger option into a Buy vs Rent horizon."
            action={(
              <Link to="/workspace/compare">
                Open workspace <span aria-hidden="true">→</span>
              </Link>
            )}
            canvas={<ConvergeCanvas homes={compareHomes} />}
            controller={controller}
          />
        ) : null}


        <StoryScene
          id="record"
          side="right"
          title="Read the official record"
          description="Registration, documents, delays and complaint history stay connected to the same home."
          action={(
            <Link to={`${propertyDetailPath(revealHome.id)}/rera`}>
              Inspect RERA evidence <span aria-hidden="true">→</span>
            </Link>
          )}
          canvas={<ReraCanvas property={revealHome} />}
          controller={controller}
        />
      </div>
    </section>
  );
}
