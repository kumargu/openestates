import { useEffect, useMemo, useState } from "react";
import type { FocusEvent, ReactNode } from "react";
import { AnimatePresence, LayoutGroup, motion } from "motion/react";
import { Link } from "react-router-dom";
import { ImageWithFallback } from "./ImageWithFallback.tsx";
import { LivingEvidenceTile } from "./evidence/LivingEvidenceTile.tsx";
import { AroundThisHomePlate } from "./evidence/AroundThisHomePlate.tsx";
import { LandingReraCanvas } from "./LandingReraCanvas.tsx";
import { getProperty, getPropertyRera, propertyDetailPath } from "../lib/api.ts";
import { filterListableProperties, uniqueSocietiesForDiscovery } from "../lib/property-filters.ts";
import type {
  PropertyCard,
  PropertyDetailResponse,
  PropertyMapContext,
  ReraEvidenceReportResponse,
} from "../lib/types.ts";
import { useLandingSceneController } from "../hooks/useLandingSceneController.ts";
import { useLandingLoopSequence } from "../hooks/useLandingChapterSequence.ts";
import { useLandingResolveSequence } from "../hooks/useLandingResolveSequence.ts";
import { useLandingStoryMotion } from "../hooks/useLandingStoryMotion.ts";

const FEATURED_LIMIT = 6;
const STORY_SCENE_IDS = ["resolve", "reveal", "remember", "converge", "record"] as const;
const RESOLVE_QUERY = "3BHK under 2Cr with strong reviews and generous open space";
const NOTEBOOK_SEQUENCE_DURATIONS = [1_400, 1_100, 2_200] as const;
const COMPARE_SEQUENCE_DURATIONS = [1_800, 1_500, 2_200] as const;
const CARD_UNFOLD_TRANSITION = {
  type: "spring",
  visualDuration: 0.55,
  damping: 25.5,
  bounce: 0.05,
  restDelta: 0.01,
} as const;

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

const FEATURED_LENSES: FeaturedLens[] = [
  { id: "metro", label: "Near metro", query: "Homes near metro with low commute pain" },
  { id: "family", label: "Family-friendly", query: "Family-friendly 3BHK near good schools" },
  { id: "township", label: "Large townships", query: "Large townships with generous open space" },
  { id: "feedback", label: "Resident feedback", query: "Homes with strong resident feedback" },
];

type LandingStoryStageProps = {
  properties: PropertyCard[];
  onSearch: (query: string) => void;
  compactOpening?: boolean;
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

function usePropertyDetail(propertyId?: string): PropertyDetailResponse | undefined {
  const [loaded, setLoaded] = useState<{
    propertyId: string;
    detail?: PropertyDetailResponse;
  }>();

  useEffect(() => {
    if (!propertyId) return undefined;
    const controller = new AbortController();
    getProperty(propertyId, { signal: controller.signal })
      .then((detail) => setLoaded({ propertyId, detail }))
      .catch((error: unknown) => {
        if (!(error instanceof DOMException && error.name === "AbortError")) {
          setLoaded({ propertyId, detail: undefined });
        }
      });
    return () => controller.abort();
  }, [propertyId]);

  return loaded && loaded.propertyId === propertyId ? loaded.detail : undefined;
}

function usePropertyRera(propertyId?: string): ReraEvidenceReportResponse | undefined {
  const [loaded, setLoaded] = useState<{
    propertyId: string;
    report?: ReraEvidenceReportResponse;
  }>();

  useEffect(() => {
    if (!propertyId) return undefined;
    getPropertyRera(propertyId)
      .then((report) => setLoaded({ propertyId, report }))
      .catch(() => setLoaded({ propertyId, report: undefined }));
    return undefined;
  }, [propertyId]);

  return loaded && loaded.propertyId === propertyId ? loaded.report : undefined;
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

function querySegments(query: string): string[] {
  const words = query.split(/\s+/).filter(Boolean);
  const segmentSize = Math.max(1, Math.ceil(words.length / 3));
  const segments: string[] = [];
  for (let index = 0; index < words.length; index += segmentSize) {
    segments.push(words.slice(index, index + segmentSize).join(" "));
  }
  return segments;
}

function resolveReasons(property: PropertyCard): EvidenceFact[] {
  const reasons: EvidenceFact[] = [];
  if (hasKnownNumber(property.metro_distance_mins)) {
    reasons.push({ id: "metro", label: "Metro", value: `${property.metro_distance_mins} min` });
  }
  if (hasKnownNumber(property.google_rating)) {
    reasons.push({ id: "reviews", label: "Reviews", value: `Google ${property.google_rating.toFixed(1)}` });
  }
  if (hasKnownNumber(property.open_space_pct)) {
    reasons.push({ id: "open-space", label: "Open space", value: `${Math.round(property.open_space_pct)}%` });
  }
  return reasons.slice(0, 2);
}

function JourneyHomeName({
  active,
  children,
  reducedMotion,
}: {
  active: boolean;
  children: ReactNode;
  reducedMotion: boolean;
}) {
  if (!active || reducedMotion) return <strong>{children}</strong>;

  return (
    <motion.strong
      className="landing-journey-home"
      layoutId="landing-journey-home"
      transition={{ type: "spring", stiffness: 210, damping: 30, mass: 0.8 }}
    >
      {children}
    </motion.strong>
  );
}

function ResolveCanvas({
  active,
  homes,
  paused,
  query,
  reducedMotion,
}: {
  active: boolean;
  homes: PropertyCard[];
  paused: boolean;
  query: string;
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
      <div className="landing-resolve__world" aria-hidden="true">
        <span className="landing-resolve__orbit landing-resolve__orbit--one" />
        <span className="landing-resolve__orbit landing-resolve__orbit--two" />
        <span className="landing-resolve__node landing-resolve__node--one" />
        <span className="landing-resolve__node landing-resolve__node--two" />
        <span className="landing-resolve__node landing-resolve__node--three" />
      </div>
      <div className="landing-resolve__intent">
        <p className="landing-resolve__query" aria-label={query}>
          {querySegments(query).map((segment) => <span key={segment}>{segment}</span>)}
        </p>
      </div>
      <div className="landing-resolve__homes">
        {homes.map((property, index) => {
          const meta = [
            property.bhk > 0 ? `${property.bhk} BHK` : null,
            formatPrice(property.price) || null,
          ].filter((value): value is string => Boolean(value));
          const image = property.hero_image
            || property.images?.find((candidate) => candidate && !candidate.startsWith("placeholder://"))
            || null;

          return (
            <article
              key={property.id}
              className={`landing-resolve__home${index === 0 ? " is-focus" : ""}`}
            >
              <div className="landing-resolve__home-media" aria-hidden="true">
                <ImageWithFallback
                  src={image}
                  alt=""
                  loading={index === 0 ? "eager" : "lazy"}
                  fetchPriority={index === 0 ? "high" : "low"}
                />
              </div>
              <span className="landing-resolve__rank">0{index + 1}</span>
              <div className="landing-resolve__home-copy">
                {index === 0 ? (
                  <JourneyHomeName active={active} reducedMotion={reducedMotion}>
                    {homeName(property)}
                  </JourneyHomeName>
                ) : <strong>{homeName(property)}</strong>}
                {meta.length > 0 ? <span>{meta.join(" · ")}</span> : null}
                {index === 0 && reasons.length > 0 ? (
                  <div className="landing-resolve__reasons">
                    {reasons.map((reason) => (
                      <em key={reason.id}>
                        <span>{reason.label}</span>
                        <b>{reason.value}</b>
                      </em>
                    ))}
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

function RevealCanvas({
  active,
  mapContext,
  property,
  reducedMotion,
}: {
  active: boolean;
  mapContext?: PropertyMapContext;
  property: PropertyCard;
  reducedMotion: boolean;
}) {
  const homeMeta = [
    property.bhk > 0 ? `${property.bhk} BHK` : null,
    formatPrice(property.price) || null,
  ].filter((value): value is string => Boolean(value));

  return (
    <div
      className="landing-product landing-product--reveal"
      data-active={active ? "true" : "false"}
    >
      <header className="landing-reveal__home">
        <div>
          <JourneyHomeName active={active} reducedMotion={reducedMotion}>
            {homeName(property)}
          </JourneyHomeName>
          <span>{property.area}</span>
        </div>
        {homeMeta.length > 0 ? <p>{homeMeta.join(" · ")}</p> : null}
      </header>

      <div className="landing-reveal__real-map">
        {mapContext ? (
          <AroundThisHomePlate propertyId={property.id} context={mapContext} />
        ) : null}
      </div>
    </div>
  );
}

function NotebookCanvas({
  active,
  paused,
  property,
  reducedMotion,
}: {
  active: boolean;
  paused: boolean;
  property: PropertyCard;
  reducedMotion: boolean;
}) {
  const phase = useLandingLoopSequence({
    active,
    durations: NOTEBOOK_SEQUENCE_DURATIONS,
    paused,
    reducedMotion,
  });
  const notes = [
    { id: "commute", label: "Commute", text: "Easy metro access" },
    { id: "visit", label: "Visit", text: "Check evening traffic and water pressure" },
    { id: "price", label: "Price", text: "Compare maintenance and monthly cost" },
  ];

  return (
    <div className="landing-product landing-product--remember" data-phase={phase}>
      <header className="landing-remember__home">
        <div>
          <span>Saved home</span>
          <JourneyHomeName active={active} reducedMotion={reducedMotion}>
            {homeName(property)}
          </JourneyHomeName>
        </div>
        <i aria-hidden="true">♥</i>
      </header>

      <div className="landing-remember__page">
        <div className="landing-remember__note-sheet">
          <div className="landing-remember__notes">
            {notes.map((note, index) => (
              <motion.div
                key={note.id}
                initial={false}
                animate={active && index <= phase ? { opacity: 1, y: 0 } : { opacity: 0, y: 80 }}
                transition={{
                  ...CARD_UNFOLD_TRANSITION,
                  delay: reducedMotion ? 0 : 0.02 + index * 0.06,
                  visualDuration: reducedMotion ? 0 : CARD_UNFOLD_TRANSITION.visualDuration,
                }}
              >
                <span className={`is-${note.id}`}>{note.label}</span>
                <p>{note.text}</p>
              </motion.div>
            ))}
          </div>
        </div>
        <div className="landing-remember__action-sheet">
          <AnimatePresence mode="popLayout" initial={false}>
            {phase === 0 ? (
              <motion.div
                key="commands"
                className="landing-remember__command-menu"
                initial={reducedMotion ? false : { opacity: 0, y: "110%" }}
                animate={{ opacity: 1, y: 0 }}
                exit={reducedMotion ? undefined : { opacity: 0, y: "-110%" }}
                transition={{
                  ...CARD_UNFOLD_TRANSITION,
                  visualDuration: reducedMotion ? 0 : CARD_UNFOLD_TRANSITION.visualDuration,
                }}
              >
                <p><span>/visit</span><strong>Visit</strong></p>
                <p><span>/budget</span><strong>Budget</strong></p>
                <p><span>/payment</span><strong>Before payment</strong></p>
              </motion.div>
            ) : (
              <motion.div
                key="visit-checklist"
                initial={reducedMotion ? false : { opacity: 0, y: "110%" }}
                animate={{ opacity: 1, y: 0 }}
                exit={reducedMotion ? undefined : { opacity: 0, y: "-110%" }}
                transition={{
                  ...CARD_UNFOLD_TRANSITION,
                  visualDuration: reducedMotion ? 0 : CARD_UNFOLD_TRANSITION.visualDuration,
                }}
              >
                <div className="landing-remember__command">
                  <span>/visit</span>
                  <strong>Visit checklist</strong>
                </div>
                {phase >= 2 ? (
                  <motion.div
                    className="landing-remember__checklist"
                    initial={reducedMotion ? false : { opacity: 0, y: "70%", scale: 0.98 }}
                    animate={{ opacity: 1, y: 0, scale: 1 }}
                    transition={{
                      ...CARD_UNFOLD_TRANSITION,
                      visualDuration: reducedMotion ? 0 : CARD_UNFOLD_TRANSITION.visualDuration,
                    }}
                  >
                    <header>
                      <strong>Visit</strong>
                      <span>0 / 3 done</span>
                    </header>
                    <p><i aria-hidden="true" /> Check water pressure</p>
                    <p><i aria-hidden="true" /> Listen for balcony traffic noise</p>
                    <p><i aria-hidden="true" /> Confirm parking slot</p>
                  </motion.div>
                ) : null}
              </motion.div>
            )}
          </AnimatePresence>
        </div>
      </div>
    </div>
  );
}

function compareProjectSignals(property: PropertyCard): string[] {
  const signals: string[] = [];
  if (hasKnownNumber(property.society_land_acres)) {
    signals.push(`${property.society_land_acres.toFixed(1)} acres`);
  }
  if (hasKnownNumber(property.open_space_pct)) {
    signals.push(`${Math.round(property.open_space_pct)}% open`);
  }
  if (hasKnownNumber(property.google_rating)) {
    signals.push(`Google ${property.google_rating.toFixed(1)}`);
  }
  return signals.slice(0, 3);
}

function ConvergeCanvas({
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
  const phase = useLandingLoopSequence({
    active,
    durations: COMPARE_SEQUENCE_DURATIONS,
    paused,
    reducedMotion,
  });
  const [left, right] = homes;
  if (!left || !right) return null;
  const comparedHomes = [left, right];

  return (
    <div
      className="landing-product landing-product--converge"
      data-active={active ? "true" : "false"}
      data-phase={phase}
    >
      <div className="landing-converge__toolbar">
        <span>{left.bhk} BHK</span>
      </div>

      <div className="landing-converge__table">
        <div className="landing-converge__homes">
          {comparedHomes.map((home, index) => (
            <Link key={home.id} to={propertyDetailPath(home.id)}>
              <i aria-hidden="true">0{index + 1}</i>
              {index === 0 ? (
                <JourneyHomeName active={active} reducedMotion={reducedMotion}>
                  {homeName(home)}
                </JourneyHomeName>
              ) : <strong>{homeName(home)}</strong>}
              <span>{home.area}</span>
              <small>{formatPrice(home.price)}{hasKnownNumber(home.sqft) ? ` · ${home.sqft.toLocaleString("en-IN")} sqft` : ""}</small>
            </Link>
          ))}
        </div>

        <AnimatePresence mode="popLayout" initial={false}>
          {phase === 0 ? (
            <motion.section
              key="society"
              className="landing-converge__society"
              initial={reducedMotion ? false : { opacity: 0, y: "110%" }}
              animate={{ opacity: 1, y: 0 }}
              exit={reducedMotion ? undefined : { opacity: 0, y: "-110%" }}
              transition={{
                ...CARD_UNFOLD_TRANSITION,
                visualDuration: reducedMotion ? 0 : CARD_UNFOLD_TRANSITION.visualDuration,
              }}
            >
              <h3>Society</h3>
              <div>
                {comparedHomes.map((home) => (
                  <article key={home.id}>
                    <span>Project scale</span>
                    <div>
                      {compareProjectSignals(home).map((signal) => <em key={signal}>{signal}</em>)}
                    </div>
                    {isKnownText(home.home_state_display) ? (
                      <p><span>Home state</span><strong>{home.home_state_display}</strong></p>
                    ) : null}
                  </article>
                ))}
              </div>
            </motion.section>
          ) : phase === 1 ? (
            <motion.section
              key="labels"
              className="landing-converge__labels"
              initial={reducedMotion ? false : { opacity: 0, y: "110%" }}
              animate={{ opacity: 1, y: 0 }}
              exit={reducedMotion ? undefined : { opacity: 0, y: "-110%" }}
              transition={{
                ...CARD_UNFOLD_TRANSITION,
                visualDuration: reducedMotion ? 0 : CARD_UNFOLD_TRANSITION.visualDuration,
              }}
            >
              <h3>Buyer notes</h3>
              <div><span className="is-commute">Commute</span><p>Test evening traffic</p><p>Time the office route</p></div>
              <div><span className="is-visit">Visit</span><p>Check water pressure</p><p>Inspect construction noise</p></div>
            </motion.section>
          ) : (
            <motion.section
              key="plan"
              className="landing-converge__plan-full"
              initial={reducedMotion ? false : { opacity: 0, y: "110%" }}
              animate={{ opacity: 1, y: 0 }}
              exit={reducedMotion ? undefined : { opacity: 0, y: "-110%" }}
              transition={{
                ...CARD_UNFOLD_TRANSITION,
                visualDuration: reducedMotion ? 0 : CARD_UNFOLD_TRANSITION.visualDuration,
              }}
            >
              <header><span>Buy vs Rent</span><strong>Follow both paths to year 20</strong></header>
              <svg viewBox="0 0 560 120" preserveAspectRatio="none" aria-hidden="true">
                <path className="is-grid" d="M0 28 H560 M0 62 H560 M0 96 H560" />
                <path className="is-buy" d="M4 106 C84 102, 142 91, 216 72 S350 34, 556 14" />
                <path className="is-rent" d="M4 88 C94 84, 164 78, 244 66 S390 45, 556 31" />
              </svg>
              <footer>
                <span><i className="is-buy" />Buy</span>
                <span><i className="is-rent" />Rent + invest</span>
                <Link to={`/workspace/buy-vs-rent/${left.id}`}>Open plan <span aria-hidden="true">→</span></Link>
              </footer>
            </motion.section>
          )}
        </AnimatePresence>
      </div>

      {phase < 2 ? <div className="landing-converge__plan">
        <div>
          <span>Buy vs Rent</span>
          <strong>Follow both paths to year 20</strong>
        </div>
        <svg viewBox="0 0 180 54" aria-hidden="true">
          <path className="is-buy" d="M4 46 C40 44, 64 36, 92 25 C120 14, 146 10, 176 6" />
          <path className="is-rent" d="M4 38 C42 36, 76 33, 108 26 C140 19, 158 16, 176 13" />
        </svg>
        <Link to={`/workspace/buy-vs-rent/${left.id}`}>Open plan <span aria-hidden="true">→</span></Link>
      </div> : null}
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
  groupActive?: boolean;
  presentation?: "wide" | "tile" | "focus";
};

function StoryScene({
  id,
  side,
  title,
  description,
  action,
  canvas,
  controller,
  groupActive = false,
  presentation,
}: StorySceneProps) {
  const isActive = controller.activeSceneId === id || groupActive;
  const hasEntered = controller.hasEntered(id) || groupActive;
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
    presentation ? `landing-scene--${presentation}` : "",
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
        <h2>
          {title.split(" ").map((word, index) => (
            <span key={`${word}-${index}`}>{word}</span>
          ))}
        </h2>
        <p>{description}</p>
        <div className="landing-scene__action">{action}</div>
      </div>
      <div className="landing-scene__canvas-wrap">
        <div className="landing-canvas">{canvas}</div>
      </div>
    </article>
  );
}

type StoryChapter = {
  id: StorySceneId;
  side: "left" | "right";
  title: string;
  description: string;
  action: ReactNode;
  canvas: ReactNode;
};

export function LandingStoryStage({ properties, onSearch, compactOpening = false }: LandingStoryStageProps) {
  const listable = filterListableProperties(properties);
  const uniqueHomes = uniqueSocietiesForDiscovery(listable);
  const isDesktopStory = useDesktopStory();
  const controller = useLandingSceneController(STORY_SCENE_IDS, isDesktopStory);
  const storyRef = useLandingStoryMotion(controller.isReducedMotion);
  const resolveHomes = storyHomesForResolve(uniqueHomes);
  const revealHome = resolveHomes[0] ?? selectEvidenceHome(uniqueHomes);
  const revealDetail = usePropertyDetail(revealHome?.id);
  const revealRera = usePropertyRera(revealHome?.id);

  if (!revealHome || resolveHomes.length === 0) return null;

  const rankedStoryHomes = rankEvidenceHomes(uniqueHomes);
  const notebookHome = resolveHomes.find((home) => home.id !== revealHome.id)
    ?? rankedStoryHomes.find((home) => home.id !== revealHome.id)
    ?? revealHome;
  const compareHomes = rankedStoryHomes
    .filter((home) => home.id !== revealHome.id && home.id !== notebookHome.id)
    .slice(0, 2);
  const resolveIsActive = controller.activeSceneId === "resolve";
  const activeSceneId = (controller.activeSceneId ?? "resolve") as StorySceneId;
  const middleChaptersActive = isDesktopStory
    && (["reveal", "remember", "converge"] as StorySceneId[]).includes(activeSceneId);
  const chapters: StoryChapter[] = [
    {
      id: "resolve",
      side: "right",
      title: "Start with the life you want",
      description: "A natural-language search becomes a small, ranked set of homes with reasons attached.",
      action: (
        <button type="button" onClick={() => onSearch(RESOLVE_QUERY)}>
          Try this search <span aria-hidden="true">→</span>
        </button>
      ),
      canvas: (
        <ResolveCanvas
          key={`${resolveIsActive ? "active" : "rest"}-${controller.isReducedMotion ? "reduced" : "motion"}`}
          active={resolveIsActive}
          homes={resolveHomes}
          paused={controller.isPaused("resolve")}
          query={RESOLVE_QUERY}
          reducedMotion={controller.isReducedMotion}
        />
      ),
    },
    {
      id: "reveal",
      side: "left",
      title: "See the home in context",
      description: "Map context, project checks and resident reviews—together.",
      action: (
        <Link to={propertyDetailPath(revealHome.id)}>
          Open details <span aria-hidden="true">→</span>
        </Link>
      ),
      canvas: (
        <RevealCanvas
          active={controller.activeSceneId === "reveal" || middleChaptersActive}
          mapContext={revealDetail?.map_context}
          property={revealHome}
          reducedMotion={controller.isReducedMotion}
        />
      ),
    },
    {
      id: "remember",
      side: "right",
      title: "Keep notes with the home",
      description: "Save what you notice and turn it into a visit checklist.",
      action: (
        <Link to="/workspace">
          Open notes <span aria-hidden="true">→</span>
        </Link>
      ),
      canvas: (
        <NotebookCanvas
          active={controller.activeSceneId === "remember" || middleChaptersActive}
          paused={controller.isPaused("remember")}
          property={notebookHome}
          reducedMotion={controller.isReducedMotion}
        />
      ),
    },
    ...(compareHomes.length >= 2 ? [{
      id: "converge" as const,
      side: "left" as const,
      title: "Compare what matters",
      description: "Put saved homes side by side, then check Buy vs Rent.",
      action: (
        <Link to="/workspace/compare">
          Compare homes <span aria-hidden="true">→</span>
        </Link>
      ),
      canvas: (
        <ConvergeCanvas
          active={controller.activeSceneId === "converge" || middleChaptersActive}
          homes={compareHomes}
          paused={controller.isPaused("converge")}
          reducedMotion={controller.isReducedMotion}
        />
      ),
    }] : []),
    {
      id: "record",
      side: "right",
      title: "Check the official record",
      description: "Registration, schedule, filings and complaints in one place.",
      action: (
        <Link to={`${propertyDetailPath(revealHome.id)}/rera`}>
          Open RERA record <span aria-hidden="true">→</span>
        </Link>
      ),
      canvas: (
        <LandingReraCanvas
          active={controller.activeSceneId === "record"}
          detail={revealDetail}
          paused={controller.isPaused("record")}
          property={revealHome}
          reducedMotion={controller.isReducedMotion}
          report={revealRera}
        />
      ),
    },
  ];
  const searchChapter = compactOpening ? undefined : chapters[0];
  const middleChapters = chapters.slice(1, -1);
  const recordChapter = chapters[chapters.length - 1];

  return (
    <section
      className="landing-stage"
      aria-label="A buyer journey through OpenEstates"
      data-reduced-motion={controller.isReducedMotion ? "true" : "false"}
    >
      {compactOpening ? null : <FeaturedSuggestions properties={uniqueHomes} onSearch={onSearch} />}

      <div ref={storyRef} className="landing-stage__story">
        <LayoutGroup id="landing-home-journey">
          {isDesktopStory ? <div className="landing-story__desktop">
            {searchChapter ? (
              <StoryScene
                {...searchChapter}
                controller={controller}
                presentation="wide"
              />
            ) : null}

            <div className="landing-story__chapter-grid">
              {middleChapters.map((chapter) => (
                <StoryScene
                  key={chapter.id}
                  {...chapter}
                  controller={controller}
                  groupActive={middleChaptersActive}
                  presentation="tile"
                />
              ))}
            </div>

            {recordChapter ? (
              <StoryScene
                {...recordChapter}
                controller={controller}
                presentation="focus"
              />
            ) : null}
          </div> : (
          <div className="landing-story__mobile">
            {(compactOpening ? chapters.slice(1) : chapters).map((chapter) => (
              <StoryScene key={chapter.id} {...chapter} controller={controller} />
            ))}
          </div>
          )}
        </LayoutGroup>
      </div>
    </section>
  );
}
