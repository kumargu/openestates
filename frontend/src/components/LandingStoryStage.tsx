import { useEffect, useMemo, useState } from "react";
import type { FocusEvent, ReactNode } from "react";
import { AnimatePresence, LayoutGroup, motion } from "motion/react";
import { Link } from "react-router-dom";
import { ImageWithFallback } from "./ImageWithFallback.tsx";
import { LivingEvidenceTile } from "./evidence/LivingEvidenceTile.tsx";
import { getProperty, getPropertyRera, propertyDetailPath } from "../lib/api.ts";
import { availableLayers, layerLabel } from "../lib/nearbyPlateProjection.ts";
import { filterListableProperties, uniqueSocietiesForDiscovery } from "../lib/property-filters.ts";
import type {
  MapPlacePin,
  PropertyCard,
  PropertyDetailResponse,
  PropertyMapContext,
  ReraComplaintSection,
  ReraDocumentSection,
  ReraDossier,
} from "../lib/types.ts";
import { useLandingSceneController } from "../hooks/useLandingSceneController.ts";
import {
  useLandingChapterSequence,
  useLandingLoopSequence,
} from "../hooks/useLandingChapterSequence.ts";
import { useLandingResolveSequence } from "../hooks/useLandingResolveSequence.ts";
import { useLandingStoryMotion } from "../hooks/useLandingStoryMotion.ts";

const FEATURED_LIMIT = 6;
const STORY_SCENE_IDS = ["resolve", "reveal", "remember", "converge", "record"] as const;
const RESOLVE_QUERY = "3BHK under 2Cr with strong reviews and generous open space";
const REVEAL_SEQUENCE_DELAYS = [620, 760] as const;
const NOTEBOOK_SEQUENCE_DURATIONS = [1_400, 1_100, 2_200] as const;
const COMPARE_SEQUENCE_DURATIONS = [1_800, 1_500, 2_200] as const;
const RERA_SEQUENCE_DURATIONS = [1_800, 1_800, 1_800, 2_200] as const;
const RESOLVE_STORY_DURATION_MS = 5_400;
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

type LandingMapStory = {
  id: string;
  label: string;
  places: MapPlacePin[];
  visual: "places" | "metro" | "lakes" | "water" | "lines";
};

type ResolveStory = {
  id: string;
  query: string;
  homes: PropertyCard[];
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

function landingMapStories(context?: PropertyMapContext): LandingMapStory[] {
  if (!context) return [];

  const stories = availableLayers(context).map((layer): LandingMapStory => ({
    id: layer,
    label: layerLabel(layer, context),
    places: context.places.filter((place) => place.layer === layer),
    visual: layer === "metro"
      ? "metro"
      : layer === "lakes"
        ? "lakes"
        : layer === "red_flags"
          ? "lines"
          : "places",
  }));

  if ((context.metro_lines?.length ?? 0) > 0 && !stories.some((story) => story.id === "metro")) {
    stories.unshift({ id: "metro", label: layerLabel("metro", context), places: [], visual: "metro" });
  }
  if ((context.lakes?.length ?? 0) > 0 && !stories.some((story) => story.id === "lakes")) {
    stories.push({ id: "lakes", label: layerLabel("lakes", context), places: [], visual: "lakes" });
  }
  if (context.water) {
    stories.push({ id: "water", label: "Groundwater", places: [], visual: "water" });
  }

  return stories;
}

function useLandingMapStoryIndex(
  storyCount: number,
  active: boolean,
  paused: boolean,
  reducedMotion: boolean,
): number {
  const [storyIndex, setStoryIndex] = useState(0);

  useEffect(() => {
    if (!active || paused || reducedMotion || storyCount <= 1) return undefined;
    const timer = window.setInterval(() => {
      setStoryIndex((current) => (current + 1) % storyCount);
    }, 2_500);
    return () => window.clearInterval(timer);
  }, [active, paused, reducedMotion, storyCount]);

  return storyCount > 0 ? storyIndex % storyCount : 0;
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

function usePropertyReraDossier(propertyId?: string): ReraDossier | undefined {
  const [loaded, setLoaded] = useState<{
    propertyId: string;
    dossier?: ReraDossier;
  }>();

  useEffect(() => {
    if (!propertyId) return undefined;
    let cancelled = false;
    getPropertyRera(propertyId)
      .then((response) => {
        if (!cancelled) setLoaded({ propertyId, dossier: response });
      })
      .catch(() => {
        if (!cancelled) setLoaded({ propertyId, dossier: undefined });
      });
    return () => {
      cancelled = true;
    };
  }, [propertyId]);

  return loaded && loaded.propertyId === propertyId ? loaded.dossier : undefined;
}

function buyerFacingDetail(value: string): string {
  return value
    .replace(/\s*·\s*parsed with caveats/gi, "")
    .replace(/\bparsed with caveats\b/gi, "")
    .replace(/\s{2,}/g, " ")
    .trim();
}

function toneLabel(tone?: string): string {
  if (tone === "risk" || tone === "watch" || tone === "caution") return "Attention";
  if (tone === "positive") return "Clear";
  return "Neutral";
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

function resolveStories(properties: PropertyCard[]): ResolveStory[] {
  const candidates: ResolveStory[] = [
    {
      id: "family-budget",
      query: RESOLVE_QUERY,
      homes: storyHomesForResolve(properties),
    },
    ...FEATURED_LENSES
      .filter((lens) => lens.id !== "family")
      .map((lens) => ({
        id: lens.id,
        query: lens.query,
        homes: rankHomesForLens(properties, lens.id).slice(0, 3),
      })),
  ];
  const seenResults = new Set<string>();

  return candidates.filter((story) => {
    if (story.homes.length === 0) return false;
    const signature = story.homes.map((home) => home.id).join("|");
    if (seenResults.has(signature)) return false;
    seenResults.add(signature);
    return true;
  });
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
      <div className="landing-resolve__composer">
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <circle cx="10.8" cy="10.8" r="6.2" />
          <path d="m15.4 15.4 4.1 4.1" />
        </svg>
        <p className="landing-resolve__query">
          {querySegments(query).map((segment) => <span key={segment}>{segment} </span>)}
        </p>
        <i aria-hidden="true">→</i>
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

function RevealCanvas({
  active,
  mapContext,
  paused,
  property,
  reducedMotion,
}: {
  active: boolean;
  mapContext?: PropertyMapContext;
  paused: boolean;
  property: PropertyCard;
  reducedMotion: boolean;
}) {
  const phase = useLandingChapterSequence({
    active,
    delays: REVEAL_SEQUENCE_DELAYS,
    paused,
    reducedMotion,
  });
  const mapStories = useMemo(() => landingMapStories(mapContext), [mapContext]);
  const mapStoryIndex = useLandingMapStoryIndex(
    mapStories.length,
    active,
    paused,
    reducedMotion,
  );
  const mapStory = mapStories[mapStoryIndex];
  const facts = evidenceFacts(property);
  const hasResidentSignal = hasKnownNumber(property.google_rating);
  const checkLabel = property.decision_check_summary?.tileLabel;
  const homeMeta = [
    property.bhk > 0 ? `${property.bhk} BHK` : null,
    formatPrice(property.price) || null,
  ].filter((value): value is string => Boolean(value));

  return (
    <div
      className="landing-product landing-product--reveal"
      data-active={active ? "true" : "false"}
      data-phase={phase}
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

      <div className="landing-reveal__layout">
        <section className="landing-reveal__field">
          <header>
            <h3>Around this home</h3>
            {mapStory ? <span className="landing-reveal__map-label">{mapStory.label}</span> : null}
          </header>
          <div className="landing-reveal__map" aria-hidden="true">
            <AnimatePresence mode="popLayout" initial={false}>
              <motion.div
                key={mapStory?.id ?? "home"}
                className="landing-reveal__map-frame"
                data-visual={mapStory?.visual ?? "places"}
                initial={reducedMotion ? false : { opacity: 0, y: "110%" }}
                animate={{ opacity: 1, y: 0 }}
                exit={reducedMotion ? undefined : { opacity: 0, y: "-110%" }}
                transition={{
                  ...CARD_UNFOLD_TRANSITION,
                  visualDuration: reducedMotion ? 0 : CARD_UNFOLD_TRANSITION.visualDuration,
                }}
              >
                {mapStory?.visual === "places" || mapStory?.visual === "metro" ? (
                  <>
                    <span className="landing-reveal__route landing-reveal__route--one" />
                    <span className="landing-reveal__route landing-reveal__route--two" />
                  </>
                ) : null}
                {mapStory?.visual === "metro" ? (
                  <svg className="landing-reveal__metro-line" viewBox="0 0 320 220" preserveAspectRatio="none">
                    <path d="M-14 56 C50 80, 70 142, 136 130 S214 64, 336 92" />
                  </svg>
                ) : null}
                {mapStory?.visual === "lakes" ? <span className="landing-reveal__lake" /> : null}
                {mapStory?.visual === "water" ? <span className="landing-reveal__water-zone" /> : null}
                {mapStory?.visual === "lines" ? (
                  <span className="landing-reveal__power-line">
                    <i /><i /><i />
                  </span>
                ) : null}
                <span className="landing-reveal__pin" />
                {(mapStory?.places ?? []).slice(0, 3).map((place, index) => (
                  <span
                    key={place.feature_id ?? place.place_entity_id ?? `${place.name}-${index}`}
                    className={`landing-reveal__marker landing-reveal__marker--${["one", "two", "three"][index]}`}
                  >
                    {index + 1}
                  </span>
                ))}
                {mapStory?.visual === "metro" ? <span className="landing-reveal__transit">M</span> : null}
              </motion.div>
            </AnimatePresence>
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

function formatRecordDate(value?: string): string | undefined {
  if (!value) return undefined;
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return new Intl.DateTimeFormat("en-IN", { month: "short", year: "numeric" }).format(date);
}

function ReraCanvas({
  active,
  detail,
  dossier,
  paused,
  property,
  reducedMotion,
}: {
  active: boolean;
  detail?: PropertyDetailResponse;
  dossier?: ReraDossier;
  paused: boolean;
  property: PropertyCard;
  reducedMotion: boolean;
}) {
  const phase = useLandingLoopSequence({
    active,
    durations: RERA_SEQUENCE_DURATIONS,
    paused,
    reducedMotion,
  });
  const summary = property.decision_check_summary;
  const rera = detail?.rera;
  const portfolio = detail?.builder_portfolio;
  const activeDossier = dossier ?? detail?.rera_dossier;
  const registration = activeDossier?.source.registration_number
    ?? rera?.registration_number
    ?? summary?.registrationNumberCompact;
  const status = activeDossier?.source.status ?? rera?.status;
  const documentFallback = summary?.groups
    ?.find((group) => group.id === "documents")
    ?.labels.slice(0, 3)
    .map((label) => ({ group: label.key, label: label.label, count: 1 })) ?? [];
  const summaryCards = (activeDossier?.summary_cards ?? [])
    .filter((card) => isKnownText(card.title) || isKnownText(card.detail))
    .filter((card) => !(isKnownText(registration) && /rera registered|registration/i.test(card.title)))
    .slice(0, 3);
  const glanceCards = summaryCards.length > 0
    ? summaryCards
    : (summary?.primaryLabels ?? []).slice(0, 3).map((label) => ({
      id: label.key,
      title: label.label,
      detail: label.valueText ?? "",
      tone: label.severity,
    }));
  const documentSections: Array<Pick<ReraDocumentSection, "group" | "label" | "count">> = (() => {
    const sections = (activeDossier?.document_sections ?? [])
      .filter((section) => (section.count ?? section.items?.length ?? 0) > 0)
      .slice(0, 4)
      .map((section) => ({
        group: section.group,
        label: section.label,
        count: section.count ?? section.items?.length ?? 0,
      }));
    return sections.length > 0 ? sections : documentFallback;
  })();
  const complaintSections: Array<Pick<ReraComplaintSection, "scope" | "label" | "total" | "open" | "top_themes">> =
    (activeDossier?.complaint_sections ?? [])
      .filter((section) => section.total > 0 || section.open > 0 || section.disposed > 0)
      .slice(0, 2);
  const timeline = activeDossier?.timeline ?? {
    start_date: rera?.start_date,
    original_completion_date: rera?.original_completion_date,
    completion_date: rera?.completion_date,
    delay_months: rera?.delay_months,
  };
  const legalChecks = (activeDossier?.legal_checks ?? [])
    .filter((check) => isKnownText(check.value))
    .slice(0, 3);
  const builderProjects = (portfolio?.projects ?? []).slice(0, 3);
  const phaseLabels = ["At a glance", "Documents", "Builder record", "Schedule"];
  const sceneTransition = {
    ...CARD_UNFOLD_TRANSITION,
    visualDuration: reducedMotion ? 0 : CARD_UNFOLD_TRANSITION.visualDuration,
  };

  return (
    <div className="landing-product landing-product--record" data-phase={phase}>
      <header className="landing-record__head">
        <div>
          <span>RERA</span>
          <JourneyHomeName active={active} reducedMotion={reducedMotion}>
            {homeName(property)}
          </JourneyHomeName>
        </div>
        <em>{phaseLabels[phase]}</em>
      </header>

      <div className="landing-record__movie">
        <AnimatePresence mode="popLayout" initial={false}>
          {phase === 0 ? (
            <motion.section
              key="glance"
              className="landing-record__scene landing-record__glance"
              initial={reducedMotion ? false : { opacity: 0, y: "110%" }}
              animate={{ opacity: 1, y: 0 }}
              exit={reducedMotion ? undefined : { opacity: 0, y: "-110%" }}
              transition={sceneTransition}
            >
              {(isKnownText(registration) || isKnownText(status)) ? (
                <div className="landing-record__registry">
                  {isKnownText(registration) ? <strong>{registration}</strong> : null}
                  {isKnownText(status) ? <span>{status}</span> : null}
                </div>
              ) : null}
              <div className="landing-record__summary-rows">
                {glanceCards.map((card) => (
                  <article key={card.id} className={`is-${toneLabel(card.tone).toLowerCase()}`}>
                    <span>{toneLabel(card.tone)}</span>
                    <strong>{card.title}</strong>
                    {isKnownText(card.detail) ? <p>{buyerFacingDetail(card.detail)}</p> : null}
                  </article>
                ))}
              </div>
            </motion.section>
          ) : phase === 1 ? (
            <motion.section
              key="documents"
              className="landing-record__scene landing-record__documents"
              initial={reducedMotion ? false : { opacity: 0, y: "110%" }}
              animate={{ opacity: 1, y: 0 }}
              exit={reducedMotion ? undefined : { opacity: 0, y: "-110%" }}
              transition={sceneTransition}
            >
              <div className="landing-record__tabs" aria-hidden="true">
                <span className="is-active">All</span>
                {documentSections.map((section) => (
                  <span key={section.group}>{section.label}</span>
                ))}
              </div>
              <div className="landing-record__document-list">
                {documentSections.map((section) => (
                  <p key={section.group}>
                    <i aria-hidden="true" />
                    <strong>{section.label}</strong>
                    <small>{section.count}</small>
                  </p>
                ))}
              </div>
            </motion.section>
          ) : phase === 2 ? (
            <motion.section
              key="builder"
              className="landing-record__scene landing-record__builder"
              initial={reducedMotion ? false : { opacity: 0, y: "110%" }}
              animate={{ opacity: 1, y: 0 }}
              exit={reducedMotion ? undefined : { opacity: 0, y: "-110%" }}
              transition={sceneTransition}
            >
              <div className="landing-record__builder-name">
                <strong>{portfolio?.builder_name || property.builder_name}</strong>
              </div>
              <div className="landing-record__builder-stats">
                {portfolio ? (
                  <>
                    <p><strong>{portfolio.tracked_projects}</strong><span>tracked projects</span></p>
                    <p><strong>{portfolio.rera_registered_projects}/{portfolio.tracked_projects}</strong><span>RERA linked</span></p>
                    <p><strong>{portfolio.delayed_projects}</strong><span>delayed</span></p>
                    {typeof portfolio.revocations === "number" ? (
                      <p><strong>{portfolio.revocations}</strong><span>revocations</span></p>
                    ) : null}
                  </>
                ) : null}
                {complaintSections.map((section) => (
                  <p key={section.scope}>
                    <strong>{section.total}{section.open > 0 ? ` · ${section.open} open` : ""}</strong>
                    <span>{section.label}</span>
                  </p>
                ))}
              </div>
              {builderProjects.length > 0 ? (
                <div className="landing-record__builder-projects">
                  {builderProjects.map((project) => (
                    <article key={`${project.property_id}-${project.project_name}`}>
                      <strong>{project.project_name}</strong>
                      <span>
                        {project.area}
                        {project.current ? " · This home" : ""}
                        {hasKnownNumber(project.delay_months) ? ` · ${project.delay_months} mo` : ""}
                      </span>
                    </article>
                  ))}
                </div>
              ) : null}
            </motion.section>
          ) : (
            <motion.section
              key="schedule"
              className="landing-record__scene landing-record__schedule"
              initial={reducedMotion ? false : { opacity: 0, y: "110%" }}
              animate={{ opacity: 1, y: 0 }}
              exit={reducedMotion ? undefined : { opacity: 0, y: "-110%" }}
              transition={sceneTransition}
            >
              <div className="landing-record__metrics">
                {formatRecordDate(timeline?.start_date) ? (
                  <p><span>Start</span><strong>{formatRecordDate(timeline?.start_date)}</strong></p>
                ) : null}
                {formatRecordDate(timeline?.original_completion_date) ? (
                  <p><span>Original target</span><strong>{formatRecordDate(timeline?.original_completion_date)}</strong></p>
                ) : null}
                {formatRecordDate(timeline?.completion_date) ? (
                  <p><span>Current target</span><strong>{formatRecordDate(timeline?.completion_date)}</strong></p>
                ) : null}
                {hasKnownNumber(timeline?.delay_months) ? (
                  <p className="is-attention"><span>Movement</span><strong>{timeline?.delay_months} months</strong></p>
                ) : null}
              </div>
              {legalChecks.length > 0 ? (
                <div className="landing-record__legal">
                  {legalChecks.map((check) => (
                    <article key={check.key}>
                      <span>{check.label}</span>
                      <strong>{check.value}</strong>
                    </article>
                  ))}
                </div>
              ) : null}
            </motion.section>
          )}
        </AnimatePresence>
      </div>

      <div className="landing-record__progress" aria-hidden="true">
        {phaseLabels.map((label, index) => (
          <span key={label} className={index === phase ? "is-active" : ""} />
        ))}
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

export function LandingStoryStage({ properties, onSearch }: LandingStoryStageProps) {
  const listable = filterListableProperties(properties);
  const uniqueHomes = uniqueSocietiesForDiscovery(listable);
  const isDesktopStory = useDesktopStory();
  const controller = useLandingSceneController(STORY_SCENE_IDS, isDesktopStory);
  const storyRef = useLandingStoryMotion(controller.isReducedMotion);
  const stories = resolveStories(uniqueHomes);
  const resolveStoryDurations = Array.from(
    { length: stories.length },
    () => RESOLVE_STORY_DURATION_MS,
  );
  const resolveStoryIndex = useLandingLoopSequence({
    active: controller.activeSceneId === "resolve",
    durations: resolveStoryDurations,
    paused: controller.isPaused("resolve"),
    reducedMotion: controller.isReducedMotion,
  });
  const resolveStory = stories.length > 0
    ? stories[resolveStoryIndex % stories.length]
    : undefined;
  const resolveHomes = stories[0]?.homes ?? [];
  const revealHome = resolveHomes[0] ?? selectEvidenceHome(uniqueHomes);
  const revealDetail = usePropertyDetail(revealHome?.id);
  const revealRera = usePropertyReraDossier(revealHome?.id);

  if (!revealHome || !resolveStory) return null;

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
        <button type="button" onClick={() => onSearch(resolveStory.query)}>
          Try this search <span aria-hidden="true">→</span>
        </button>
      ),
      canvas: (
        <ResolveCanvas
          key={`${resolveStory.id}-${resolveIsActive ? "active" : "rest"}-${controller.isReducedMotion ? "reduced" : "motion"}`}
          active={resolveIsActive}
          homes={resolveStory.homes}
          paused={controller.isPaused("resolve")}
          query={resolveStory.query}
          reducedMotion={controller.isReducedMotion}
        />
      ),
    },
    {
      id: "reveal",
      side: "left",
      title: "Open a home, not a listing",
      description: "The result expands into map context, project checks and resident reviews without losing why it matched.",
      action: (
        <Link to={propertyDetailPath(revealHome.id)}>
          See the full picture <span aria-hidden="true">→</span>
        </Link>
      ),
      canvas: (
        <RevealCanvas
          active={controller.activeSceneId === "reveal" || middleChaptersActive}
          mapContext={revealDetail?.map_context}
          paused={controller.isPaused("reveal")}
          property={revealHome}
          reducedMotion={controller.isReducedMotion}
        />
      ),
    },
    {
      id: "remember",
      side: "right",
      title: "Keep your judgment with the home",
      description: "Save the home, write what you noticed, then turn a slash command into a visit checklist.",
      action: (
        <Link to="/workspace">
          Open notebook <span aria-hidden="true">→</span>
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
      title: "Make the tradeoffs visible",
      description: "Put two saved homes side by side, then carry the stronger option into a Buy vs Rent horizon.",
      action: (
        <Link to="/workspace/compare">
          Open workspace <span aria-hidden="true">→</span>
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
      title: "Read the official record",
      description: "At a glance, documents, builder record and schedule stay connected to the same home.",
      action: (
        <Link to={`${propertyDetailPath(revealHome.id)}/rera`}>
          Inspect RERA evidence <span aria-hidden="true">→</span>
        </Link>
      ),
      canvas: (
        <ReraCanvas
          active={controller.activeSceneId === "record"}
          detail={revealDetail}
          dossier={revealRera}
          paused={controller.isPaused("record")}
          property={revealHome}
          reducedMotion={controller.isReducedMotion}
        />
      ),
    },
  ];
  const searchChapter = chapters[0];
  const middleChapters = chapters.slice(1, -1);
  const recordChapter = chapters[chapters.length - 1];

  return (
    <section
      className="landing-stage"
      aria-label="A buyer journey through OpenEstates"
      data-reduced-motion={controller.isReducedMotion ? "true" : "false"}
    >
      <FeaturedSuggestions properties={uniqueHomes} onSearch={onSearch} />

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
            {chapters.map((chapter) => (
              <StoryScene key={chapter.id} {...chapter} controller={controller} />
            ))}
          </div>
          )}
        </LayoutGroup>
      </div>
    </section>
  );
}
