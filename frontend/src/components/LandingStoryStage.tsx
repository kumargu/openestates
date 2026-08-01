import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { LivingEvidenceTile } from "./evidence/LivingEvidenceTile.tsx";
import { SoftNearbyIcon } from "./ui/SoftIcons.tsx";
import { LabelPill } from "./ui/LabelPill.tsx";
import { propertyDetailPath } from "../lib/api.ts";
import { filterListableProperties } from "../lib/property-filters.ts";
import {
  sentimentsForAreas,
  sentimentSourceLabel,
  themeKindLabel,
  type AreaSentiment,
} from "../lib/areaSentiments.ts";
import type { NotebookLabelId } from "../lib/notebook.ts";
import type { PropertyCard } from "../lib/types.ts";
import "../features/home-plan/home-plan.css";

function formatPrice(price: number): string {
  if (price >= 10_000_000) return `₹${(price / 10_000_000).toFixed(1)} Cr`;
  if (price >= 100_000) return `₹${(price / 100_000).toFixed(1)} L`;
  return `₹${price.toLocaleString("en-IN")}`;
}

function formatAdvantage(price: number): string {
  const advantage = Math.max(8, Math.round(price / 1_000_000) * 1.2);
  return `₹${advantage.toFixed(1)}L`;
}

const FEATURED_LIMIT = 12;
const DEMO_BUDGET_INR = 25_000_000;

type SearchDemoTheme = "metro" | "acres" | "family" | "reviews";

type SearchDemoBeat = {
  id: string;
  query: string;
  theme: SearchDemoTheme;
  intents: string[];
  motif: string;
  landsOn: string;
  societyHints: string[];
  fallbackHints: string[];
  proof: (home: PropertyCard) => string[];
};

const SEARCH_DEMO_BEATS: SearchDemoBeat[] = [
  {
    id: "kadugodi-metro",
    query: "Near Kadugodi metro",
    theme: "metro",
    intents: ["Named place", "Metro access", "Whitefield corridor"],
    motif: "Transit intent pulls the closest Purple Line homes",
    landsOn: "Prestige Waterford",
    societyHints: ["waterford"],
    fallbackHints: ["kadugodi", "whitefield", "itpl"],
    proof: (home) => {
      const labels: string[] = [];
      if (home.metro_distance_mins > 0) labels.push(`${home.metro_distance_mins} min metro`);
      labels.push("Near Kadugodi");
      if (typeof home.google_rating === "number" && home.google_rating > 0) {
        labels.push(`Google ${home.google_rating.toFixed(1)}`);
      }
      return labels.slice(0, 3);
    },
  },
  {
    id: "large-township",
    query: "100+ acre society with lake",
    theme: "acres",
    intents: ["Land scale", "Lake township", "Open campus"],
    motif: "Scale + water intent lifts the large lakeside township",
    landsOn: "Prestige Lakeside Habitat",
    societyHints: ["lakeside habitat", "lakeside"],
    fallbackHints: ["habitat", "township"],
    proof: (home) => {
      const labels: string[] = [];
      if (typeof home.society_land_acres === "number" && home.society_land_acres > 0) {
        labels.push(`${Math.round(home.society_land_acres)} acres`);
      } else {
        labels.push("Large township");
      }
      labels.push("Lake setting");
      if (typeof home.open_space_pct === "number" && home.open_space_pct > 0) {
        labels.push(`${Math.round(home.open_space_pct)}% open`);
      }
      return labels.slice(0, 3);
    },
  },
  {
    id: "quiet-family",
    query: "Quiet 3BHK near schools under 2.5Cr",
    theme: "family",
    intents: ["3 BHK", "Under 2.5 Cr", "Schools", "Calm"],
    motif: "Family life maps BHK, budget, and calm context together",
    landsOn: "A calm 3BHK fit",
    societyHints: [],
    fallbackHints: [],
    proof: (home) => {
      const labels: string[] = [];
      if (home.bhk === 3) labels.push("3 BHK fit");
      if (home.price > 0 && home.price <= DEMO_BUDGET_INR) labels.push("Under 2.5 Cr");
      if (typeof home.google_rating === "number" && home.google_rating >= 4) {
        labels.push(`Google ${home.google_rating.toFixed(1)}`);
      }
      if (home.metro_distance_mins > 0 && home.metro_distance_mins <= 20) {
        labels.push(`${home.metro_distance_mins} min metro`);
      }
      return labels.slice(0, 3);
    },
  },
  {
    id: "google-proof",
    query: "Whitefield homes with strong Google reviews",
    theme: "reviews",
    intents: ["Whitefield", "Google proof", "Resident signal"],
    motif: "Review strength becomes the rank axis — not a silent filter",
    landsOn: "Strongest Google-backed home",
    societyHints: [],
    fallbackHints: ["whitefield"],
    proof: (home) => {
      const labels: string[] = [];
      if (typeof home.google_rating === "number" && home.google_rating > 0) {
        labels.push(`Google ${home.google_rating.toFixed(1)}`);
      }
      if (typeof home.google_review_count === "number" && home.google_review_count > 0) {
        labels.push(`${home.google_review_count} reviews`);
      }
      labels.push(home.area);
      return labels.slice(0, 3);
    },
  },
];

type LandingStoryStageProps = {
  properties: PropertyCard[];
  onSearch: (query: string) => void;
};

type NoteBeat = {
  text: string;
  labels: NotebookLabelId[];
};

type DossierTone = "excellent" | "good" | "clear" | "verified" | "watch";

type DossierRow = {
  label: string;
  value: string;
  tone: DossierTone;
  toneLabel: string;
};

function haystack(property: PropertyCard): string {
  return `${property.society_name} ${property.title} ${property.area}`.toLowerCase();
}

function pickDemoHome(homes: PropertyCard[], beat: SearchDemoBeat): PropertyCard | null {
  if (homes.length === 0) return null;

  for (const hint of beat.societyHints) {
    const match = homes.find((home) => haystack(home).includes(hint));
    if (match) return match;
  }

  if (beat.id === "google-proof") {
    const rated = [...homes]
      .filter((home) => typeof home.google_rating === "number" && home.google_rating > 0)
      .sort((a, b) => (b.google_rating ?? 0) - (a.google_rating ?? 0)
        || (b.google_review_count ?? 0) - (a.google_review_count ?? 0));
    if (rated[0]) return rated[0];
  }

  if (beat.id === "quiet-family") {
    const family = [...homes]
      .filter((home) => home.bhk === 3 && home.price > 0 && home.price <= DEMO_BUDGET_INR)
      .sort((a, b) => (b.google_rating ?? 0) - (a.google_rating ?? 0));
    if (family[0]) return family[0];
  }

  if (beat.id === "kadugodi-metro") {
    const metro = [...homes]
      .filter((home) => home.metro_distance_mins > 0)
      .sort((a, b) => a.metro_distance_mins - b.metro_distance_mins);
    if (metro[0]) return metro[0];
  }

  if (beat.id === "large-township") {
    const byAcres = [...homes]
      .filter((home) => typeof home.society_land_acres === "number" && (home.society_land_acres ?? 0) > 0)
      .sort((a, b) => (b.society_land_acres ?? 0) - (a.society_land_acres ?? 0));
    if (byAcres[0]) return byAcres[0];
  }

  for (const hint of beat.fallbackHints) {
    const match = homes.find((home) => haystack(home).includes(hint));
    if (match) return match;
  }

  return homes[0] ?? null;
}

function semanticMatchLabels(property: PropertyCard, beat: SearchDemoBeat = SEARCH_DEMO_BEATS[2]): string[] {
  return beat.proof(property).slice(0, 2);
}

function FeaturedSuggestions({
  properties,
  onSearch,
}: {
  properties: PropertyCard[];
  onSearch: (query: string) => void;
}) {
  const [beatIndex, setBeatIndex] = useState(0);
  const beat = SEARCH_DEMO_BEATS[beatIndex % SEARCH_DEMO_BEATS.length];

  useEffect(() => {
    const media = window.matchMedia("(prefers-reduced-motion: reduce)");
    if (media.matches) return undefined;
    const timer = window.setInterval(() => {
      setBeatIndex((current) => (current + 1) % SEARCH_DEMO_BEATS.length);
    }, 9000);
    return () => window.clearInterval(timer);
  }, []);

  const suggestions = filterListableProperties(properties)
    .map((property) => ({ property, labels: semanticMatchLabels(property, beat) }))
    .sort((a, b) => {
      const preferred = pickDemoHome([a.property, b.property], beat);
      if (preferred?.id === a.property.id) return -1;
      if (preferred?.id === b.property.id) return 1;
      return b.labels.length - a.labels.length;
    })
    .slice(0, FEATURED_LIMIT);
  if (suggestions.length === 0) return null;

  return (
    <div className="landing-featured">
      <header className="landing-featured__head">
        <p className="landing-featured__kicker">Semantic match</p>
        <h2>Change the ask — the ranked why shifts with it.</h2>
        <button type="button" className="landing-featured__query" onClick={() => onSearch(beat.query)}>
          {beat.query}
        </button>
      </header>
      <div className="landing-stage__featured" key={beat.id}>
        {suggestions.map(({ property, labels }) => (
          <div key={`${beat.id}-${property.id}`} className="landing-stage__feature-card">
            <LivingEvidenceTile
              property={property}
              variant="browse"
              matchLabels={labels}
            />
          </div>
        ))}
      </div>
    </div>
  );
}

function SemanticSearchCanvas() {
  const [beatIndex, setBeatIndex] = useState(0);
  const [phase, setPhase] = useState(0);
  const beat = SEARCH_DEMO_BEATS[beatIndex % SEARCH_DEMO_BEATS.length];
  const iconKind =
    beat.theme === "metro"
      ? "metro"
      : beat.theme === "acres"
        ? "water"
        : beat.theme === "reviews"
          ? "essentials"
          : "schools";

  useEffect(() => {
    const media = window.matchMedia("(prefers-reduced-motion: reduce)");
    if (media.matches) {
      setPhase(2);
      return undefined;
    }

    let cancelled = false;
    const timers: number[] = [];

    function clearTimers() {
      while (timers.length > 0) {
        window.clearTimeout(timers.pop());
      }
    }

    // Fast to a readable result, then hold so the context can be absorbed.
    function scheduleCycle() {
      clearTimers();
      setPhase(0);
      timers.push(window.setTimeout(() => {
        if (!cancelled) setPhase(1);
      }, 750));
      timers.push(window.setTimeout(() => {
        if (!cancelled) setPhase(2);
      }, 1400));
      timers.push(window.setTimeout(() => {
        if (cancelled) return;
        setBeatIndex((index) => (index + 1) % SEARCH_DEMO_BEATS.length);
        scheduleCycle();
      }, 1400 + 6500));
    }

    scheduleCycle();
    return () => {
      cancelled = true;
      clearTimers();
    };
  }, []);

  return (
    <div className={`landing-showcase landing-showcase--search is-${beat.theme}`}>
      <p className="landing-showcase__whisper" aria-hidden="true">
        {beat.theme === "metro" ? "Metro" : beat.theme === "acres" ? "Scale" : beat.theme === "reviews" ? "Reviews" : "Life"}
      </p>

      <div className="landing-search-stage">
        <div className={`landing-search-query${phase >= 0 ? " is-in" : ""}`} key={`q-${beat.id}`}>
          <span>Life query</span>
          <strong>{beat.query}</strong>
        </div>

        <div className="landing-search-intents" aria-label="Parsed context">
          {beat.intents.map((intent, index) => (
            <span
              key={`${beat.id}-${intent}`}
              className={`landing-search-intent${phase >= 1 ? " is-in" : ""}`}
              style={{ transitionDelay: `${index * 90}ms` }}
            >
              {intent}
            </span>
          ))}
        </div>

        <div
          key={beat.id}
          className={`landing-search-intent-board${phase >= 2 ? " is-in" : ""}`}
        >
          <div className="landing-search-intent-board__icon" aria-hidden="true">
            <SoftNearbyIcon kind={iconKind} size={26} />
          </div>
          <p className="landing-search-intent-board__motif">{beat.motif}</p>
          <p className="landing-search-intent-board__lands">
            <span>Ranks toward</span>
            <strong>{beat.landsOn}</strong>
          </p>
        </div>
      </div>
    </div>
  );
}

function notesFor(property: PropertyCard, index: number): NoteBeat[] {
  const ratingNote =
    typeof property.google_rating === "number" && property.google_rating > 0
      ? `Google ${property.google_rating.toFixed(1)}${property.google_review_count ? ` · ${property.google_review_count}` : ""}`
      : "Reviews still thin — check on visit";

  return [
    {
      text: `${property.area} · ${property.bhk} BHK`,
      labels: index === 0 ? ["commute", "schools"] : ["open-space", "layout"],
    },
    {
      text: ratingNote,
      labels: index === 0 ? ["community"] : ["layout"],
    },
    {
      text: index === 0 ? "Water felt reliable on the last visit" : "Watch approach road at peak hour",
      labels: index === 0 ? ["water"] : ["approach", "risk"],
    },
  ];
}

function NotebookCompareCanvas({ homes }: { homes: PropertyCard[] }) {
  const pair = homes.slice(0, 2);
  const [phase, setPhase] = useState<"notes" | "compare">("notes");
  const [noteStep, setNoteStep] = useState(0);

  useEffect(() => {
    if (pair.length < 2) return undefined;
    const media = window.matchMedia("(prefers-reduced-motion: reduce)");
    if (media.matches) {
      setPhase("compare");
      setNoteStep(2);
      return undefined;
    }

    let step = 0;
    const timer = window.setInterval(() => {
      step += 1;
      if (step <= 3) {
        setPhase("notes");
        setNoteStep(step);
        return;
      }
      if (step === 4) {
        setPhase("compare");
        return;
      }
      step = 0;
      setNoteStep(0);
      setPhase("notes");
    }, 1600);

    return () => window.clearInterval(timer);
  }, [pair.length]);

  if (pair.length === 0) return null;

  const diffs: Array<{
    labelId: NotebookLabelId;
    left: string;
    right: string;
    winner: "left" | "right" | "tie";
  }> = [
    {
      labelId: "commute",
      left: pair[0].metro_distance_mins > 0 ? `${pair[0].metro_distance_mins} min metro` : "ORR access",
      right: pair[1].metro_distance_mins > 0 ? `${pair[1].metro_distance_mins} min metro` : "Longer hop",
      winner: "left",
    },
    {
      labelId: "schools",
      left: "Under 1.2 km",
      right: "About 2.4 km",
      winner: "left",
    },
    {
      labelId: "water",
      left: "Reliable",
      right: "Mixed",
      winner: "left",
    },
    {
      labelId: "open-space",
      left: pair[0].open_space_pct ? `${Math.round(pair[0].open_space_pct)}%` : "Tight",
      right: pair[1].open_space_pct ? `${Math.round(pair[1].open_space_pct)}%` : "More open",
      winner: pair[1].open_space_pct && (!pair[0].open_space_pct || pair[1].open_space_pct > (pair[0].open_space_pct ?? 0))
        ? "right"
        : "tie",
    },
  ];

  return (
    <div className={`landing-showcase landing-showcase--notebook is-${phase}`}>
      <p className="landing-showcase__whisper" aria-hidden="true">
        {phase === "notes" ? "Notes" : "Compare"}
      </p>

      <div className="landing-showcase__stage">
        <div className={`landing-notes-stage${phase === "notes" ? " is-visible" : ""}`}>
          {pair.map((property, homeIndex) => {
            const beats = notesFor(property, homeIndex);
            return (
              <article key={property.id} className="landing-note-card">
                <header>
                  <em>{String(homeIndex + 1).padStart(2, "0")}</em>
                  <div>
                    <strong>{property.society_name || property.title}</strong>
                    <span>{formatPrice(property.price)}</span>
                  </div>
                </header>
                <ul>
                  {beats.map((beat, beatIndex) => {
                    const visible = noteStep > beatIndex;
                    return (
                      <li
                        key={beat.text}
                        className={`landing-note-row${visible ? " is-in" : ""}`}
                        style={{ transitionDelay: `${beatIndex * 90}ms` }}
                      >
                        <span className="landing-note-row__mark" aria-hidden="true">✦</span>
                        <div>
                          <p>{beat.text}</p>
                          <div className="landing-note-row__tags">
                            {beat.labels.map((labelId) => (
                              <LabelPill
                                key={labelId}
                                labelId={labelId}
                                surface="notebook"
                                showIcon
                              />
                            ))}
                          </div>
                        </div>
                      </li>
                    );
                  })}
                </ul>
              </article>
            );
          })}
        </div>

        {pair.length === 2 && (
          <div className={`landing-compare-stage${phase === "compare" ? " is-visible" : ""}`}>
            <div className="landing-compare-heads">
              {pair.map((property, index) => (
                <Link key={property.id} to={propertyDetailPath(property.id)} className="landing-compare-head">
                  <i>{String.fromCharCode(65 + index)}</i>
                  <strong>{property.society_name || property.title}</strong>
                  <span>{property.area}</span>
                </Link>
              ))}
            </div>
            <div className="landing-compare-diffs">
              {diffs.map((diff, index) => (
                <div
                  key={diff.labelId}
                  className="landing-compare-diff"
                  style={{ animationDelay: `${index * 120}ms` }}
                >
                  <div className="landing-compare-diff__label">
                    <LabelPill labelId={diff.labelId} surface="compare" showIcon />
                  </div>
                  <div className={`landing-compare-diff__cell${diff.winner === "left" ? " is-win" : ""}`}>
                    {diff.left}
                  </div>
                  <div className={`landing-compare-diff__cell${diff.winner === "right" ? " is-win" : ""}`}>
                    {diff.right}
                  </div>
                </div>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

function MapEvidenceCanvas({ property }: { property: PropertyCard }) {
  const [pulse, setPulse] = useState(0);

  useEffect(() => {
    const media = window.matchMedia("(prefers-reduced-motion: reduce)");
    if (media.matches) return undefined;
    const timer = window.setInterval(() => setPulse((value) => (value + 1) % 4), 2200);
    return () => window.clearInterval(timer);
  }, []);

  const dossier: DossierRow[] = [
    { label: "Water", value: "BWSSB + borewell", tone: "excellent", toneLabel: "Excellent" },
    { label: "Lake buffer", value: "0.4 km", tone: "good", toneLabel: "Good" },
    { label: "High-tension", value: "Outside 500 m", tone: "clear", toneLabel: "Clear" },
    { label: "Traffic", value: "Quieter corridor", tone: "good", toneLabel: "Good" },
    { label: "RERA", value: "Approvals checked", tone: "verified", toneLabel: "Verified" },
    { label: "Schools", value: `Near ${property.area}`, tone: "good", toneLabel: "Good" },
  ];

  return (
    <div className="landing-showcase landing-showcase--map">
      <p className="landing-showcase__whisper" aria-hidden="true">
        {property.area}
      </p>

      <div className="landing-map-stage">
        <div className="landing-map-art" aria-hidden="true">
          <div className="landing-map-art__lake" />
          <div className="landing-map-art__road landing-map-art__road--a" />
          <div className="landing-map-art__road landing-map-art__road--b" />
          <div className="landing-map-art__buffer" />
          <div className={`landing-map-art__pin is-pulse-${pulse}`}>
            <SoftNearbyIcon kind="essentials" size={22} />
          </div>
          <span className="landing-map-art__label landing-map-art__label--a">{property.area}</span>
          <span className="landing-map-art__label landing-map-art__label--b">Lake</span>
          <span className="landing-map-art__label landing-map-art__label--c">ORR</span>
          <div className="landing-map-art__legend">
            <span><i className="is-road" /> Roads</span>
            <span><i className="is-lake" /> Lake buffer</span>
            <span><i className="is-line" /> Lines clear</span>
          </div>
        </div>

        <div className="landing-dossier">
          <ul>
            {dossier.map((row, index) => (
              <li
                key={row.label}
                className={index === pulse ? "is-focus" : ""}
              >
                <div>
                  <strong>{row.label}</strong>
                  <span>{row.value}</span>
                </div>
                <em className={`landing-dossier__tone landing-dossier__tone--${row.tone}`}>
                  {row.toneLabel}
                </em>
              </li>
            ))}
          </ul>
        </div>
      </div>
    </div>
  );
}

function SentimentCanvas({ property }: { property: PropertyCard }) {
  const pool = sentimentsForAreas([property.area, property.society_name || ""], 10);
  const googleLines = pool.filter((item) => item.source === "google");
  const redditLines = pool.filter((item) => item.source === "reddit");
  const googlePool = googleLines.length > 0
    ? googleLines
    : pool.filter((item) => item.polarity === "positive").slice(0, 4);
  const redditPool = redditLines.length > 0 ? redditLines : pool.slice(0, 5);

  const [focus, setFocus] = useState<"google" | "reddit">("google");
  const [googleIndex, setGoogleIndex] = useState(0);
  const [redditIndex, setRedditIndex] = useState(0);
  const [chipStep, setChipStep] = useState(0);

  useEffect(() => {
    const media = window.matchMedia("(prefers-reduced-motion: reduce)");
    if (media.matches) {
      setChipStep(4);
      return undefined;
    }

    let tick = 0;
    const timer = window.setInterval(() => {
      tick += 1;
      if (tick <= 4) {
        setChipStep(tick);
        setFocus("google");
        return;
      }
      if (tick % 2 === 1) {
        setFocus("reddit");
        setRedditIndex((current) => (current + 1) % Math.max(redditPool.length, 1));
      } else {
        setFocus("google");
        setGoogleIndex((current) => (current + 1) % Math.max(googlePool.length, 1));
      }
    }, 1800);

    return () => window.clearInterval(timer);
  }, [googlePool.length, redditPool.length]);

  const googleActive = googlePool[googleIndex % Math.max(googlePool.length, 1)];
  const redditActive = redditPool[redditIndex % Math.max(redditPool.length, 1)];
  const googleChips = googlePool.slice(0, 4);
  const rating =
    typeof property.google_rating === "number" && property.google_rating > 0
      ? property.google_rating.toFixed(1)
      : null;
  const reviewCount =
    typeof property.google_review_count === "number" && property.google_review_count > 0
      ? property.google_review_count
      : null;

  return (
    <div className={`landing-showcase landing-showcase--sentiment is-${focus}`}>
      <p className="landing-showcase__whisper" aria-hidden="true">
        {focus === "google" ? "Google" : "Reddit"}
      </p>

      <div className="landing-sentiment-stage">
        <div className={`landing-sentiment-panel landing-sentiment-panel--google${focus === "google" ? " is-focus" : ""}`}>
          <header>
            <strong>Google</strong>
            <span>
              {rating ? (
                <>
                  {rating}
                  {reviewCount ? ` · ${reviewCount}` : ""}
                </>
              ) : (
                "Reviews"
              )}
            </span>
          </header>
          <div className="landing-sentiment-chips">
            {googleChips.map((item, index) => (
              <span
                key={`${item.theme}-${item.line}`}
                className={`landing-sentiment-chip landing-sentiment-chip--${item.polarity}${chipStep > index ? " is-in" : ""}${googleActive?.theme === item.theme ? " is-active" : ""}`}
                style={{ transitionDelay: `${index * 80}ms` }}
              >
                {item.theme}
              </span>
            ))}
          </div>
          {googleActive && (
            <SentimentQuote item={googleActive} source="google" />
          )}
        </div>

        <div className={`landing-sentiment-panel landing-sentiment-panel--reddit${focus === "reddit" ? " is-focus" : ""}`}>
          <header>
            <strong>Reddit</strong>
            <span>Local chatter</span>
          </header>
          <div className="landing-sentiment-stack" aria-hidden="true">
            {redditPool.slice(0, 3).map((item, index) => (
              <span
                key={`${item.kind}-${item.theme}`}
                className={`landing-sentiment-stack__card is-${index}${redditActive?.line === item.line ? " is-front" : ""}`}
              >
                {themeKindLabel(item.kind)}
              </span>
            ))}
          </div>
          {redditActive && (
            <SentimentQuote item={redditActive} source="reddit" />
          )}
        </div>
      </div>
    </div>
  );
}

function SentimentQuote({
  item,
  source,
}: {
  item: AreaSentiment;
  source: "google" | "reddit";
}) {
  return (
    <figure className={`landing-sentiment-quote landing-sentiment-quote--${item.polarity}`}>
      <blockquote>{item.line}</blockquote>
      <figcaption>
        <span>{themeKindLabel(item.kind)}</span>
        <span aria-hidden="true">·</span>
        <span>{item.theme}</span>
        <span aria-hidden="true">·</span>
        <span>{sentimentSourceLabel(source)}</span>
      </figcaption>
    </figure>
  );
}

function PlanCanvas({ property }: { property: PropertyCard }) {
  const [year, setYear] = useState(7);

  useEffect(() => {
    const media = window.matchMedia("(prefers-reduced-motion: reduce)");
    if (media.matches) return undefined;
    const timer = window.setInterval(() => {
      setYear((current) => (current === 7 ? 12 : current === 12 ? 3 : 7));
    }, 2600);
    return () => window.clearInterval(timer);
  }, []);

  const cursorX = year === 3 ? 140 : year === 7 ? 300 : 420;
  const buyY = year === 3 ? 152 : year === 7 ? 110 : 68;
  const rentY = year === 3 ? 146 : year === 7 ? 128 : 102;

  return (
    <div className="landing-showcase landing-showcase--plan home-plan-shell">
      <p className="landing-showcase__whisper" aria-hidden="true">
        Year {year}
      </p>
      <div className="landing-product__plan-inner">
        <header className="home-plan-verdict">
          <div className="home-plan-verdict__topline">
            <h3 className="home-plan-verdict__headline">
              In {year} years, you have{" "}
              <span className="home-plan-verdict__amount">{formatAdvantage(property.price)}</span>
              {" "}more if you buy.
            </h3>
          </div>
        </header>
        <p className="home-plan-whisper">
          Rent stays lighter early. Ownership pulls ahead once the loan curve bends.
        </p>
        <div className="home-plan-graph">
          <svg className="home-plan-graph-svg landing-product__plan-svg" viewBox="0 0 560 220" aria-hidden="true">
            <defs>
              <linearGradient id="landing-buy-glow" x1="0" y1="0" x2="0" y2="1">
                <stop offset="0%" stopColor="rgba(201,107,79,0.28)" />
                <stop offset="100%" stopColor="rgba(201,107,79,0)" />
              </linearGradient>
            </defs>
            <path className="home-plan-graph-gap home-plan-graph-gap--buy" d="M40 170 C140 160, 220 140, 300 110 C380 78, 460 58, 520 42 L520 190 L40 190 Z" fill="url(#landing-buy-glow)" />
            <path className="home-plan-graph-line home-plan-graph-line--buy" d="M40 170 C140 160, 220 140, 300 110 C380 78, 460 58, 520 42" />
            <path className="home-plan-graph-line home-plan-graph-line--rent" d="M40 150 C150 148, 240 142, 320 128 C400 112, 470 98, 520 88" />
            <line className="home-plan-cursor" x1={cursorX} y1="36" x2={cursorX} y2="188" />
            <circle className="home-plan-graph-point home-plan-graph-point--buy" cx={cursorX} cy={buyY} r="6" />
            <circle className="home-plan-graph-point home-plan-graph-point--rent" cx={cursorX} cy={rentY} r="5" />
            <text className="home-plan-axis-label home-plan-axis-label--x" x="40" y="208">Now</text>
            <text className="home-plan-axis-label home-plan-axis-label--x" x="300" y="208">Year 7</text>
            <text className="home-plan-axis-label home-plan-axis-label--x" x="500" y="208">Year 15</text>
          </svg>
          <div className="landing-product__plan-legend">
            <span className="is-buy">Buy</span>
            <span className="is-rent">Rent + invest</span>
          </div>
        </div>
      </div>
    </div>
  );
}

export function LandingStoryStage({ properties, onSearch }: LandingStoryStageProps) {
  const listable = filterListableProperties(properties);
  if (listable.length === 0) return null;

  const notebookHomes = listable.slice(0, 2);
  const mapHome = listable[0];
  const sentimentHome = listable.find((item) => item.area !== mapHome.area) ?? listable[1] ?? listable[0];
  const planHome = listable[1] ?? listable[0];

  return (
    <section className="landing-stage" aria-label="How OpenEstates works">
      <div className="landing-stage__wash" aria-hidden="true" />

      <FeaturedSuggestions properties={listable} onSearch={onSearch} />

      <header className="landing-journey">
        <p className="landing-journey__step">Where we differ</p>
        <h2>Context search first. Then the neighborhood proof.</h2>
      </header>

      <article className="landing-scene landing-scene--right">
        <div className="landing-scene__copy">
          <p className="landing-scene__step">01</p>
          <h2>Search by life, not checkboxes</h2>
          <p>
            Watch the ask change — Kadugodi metro lifts Waterford, a 100-acre lake society
            lifts Lakeside Habitat — each result carries a semantic why.
          </p>
          <button
            type="button"
            className="landing-scene__cta"
            onClick={() => onSearch(SEARCH_DEMO_BEATS[0].query)}
          >
            Try a life search
          </button>
        </div>
        <div className="landing-canvas landing-canvas--product">
          <SemanticSearchCanvas />
        </div>
      </article>

      <article className="landing-scene landing-scene--left">
        <div className="landing-canvas landing-canvas--product">
          <MapEvidenceCanvas property={mapHome} />
        </div>
        <div className="landing-scene__copy">
          <p className="landing-scene__step">02</p>
          <h2>Then read what’s around the home</h2>
          <p>
            Water, lake buffer, lines, traffic, schools — status tones beside each signal,
            before you fall for the brochure.
          </p>
          <button
            type="button"
            className="landing-scene__cta"
            onClick={() => onSearch(`${mapHome.area} near good schools`)}
          >
            Explore {mapHome.area}
          </button>
        </div>
      </article>

      <article className="landing-scene landing-scene--right">
        <div className="landing-scene__copy">
          <p className="landing-scene__step">03</p>
          <h2>Then read what people keep saying</h2>
          <p>
            Google themes and Reddit lines, curated side by side — praise, caution, and
            tradeoffs without the tab chase.
          </p>
          <button
            type="button"
            className="landing-scene__cta"
            onClick={() => onSearch(`${sentimentHome.area} with good Google reviews`)}
          >
            Read {sentimentHome.area}
          </button>
        </div>
        <div className="landing-canvas landing-canvas--product">
          <SentimentCanvas property={sentimentHome} />
        </div>
      </article>

      <article className="landing-scene landing-scene--left">
        <div className="landing-canvas landing-canvas--product">
          <NotebookCompareCanvas homes={notebookHomes} />
        </div>
        <div className="landing-scene__copy">
          <p className="landing-scene__step">04</p>
          <h2>Capture notes — then compare</h2>
          <p>
            Tag what you notice on visits. Those same labels open into a ready side-by-side
            when two homes are in play.
          </p>
          <Link to="/workspace" className="landing-scene__cta">
            Open notebook
          </Link>
        </div>
      </article>

      <article className="landing-scene landing-scene--right landing-scene--compare">
        <div className="landing-scene__copy">
          <p className="landing-scene__step">05</p>
          <h2>Finish on the money tradeoff</h2>
          <p>
            Buy and rent on one horizon — the year marker slides so the advantage feels
            concrete, not abstract.
          </p>
          <Link to={`/property/${planHome.id}/plan`} className="landing-scene__cta">
            Open plan
          </Link>
        </div>
        <div className="landing-canvas landing-canvas--product">
          <PlanCanvas property={planHome} />
        </div>
      </article>
    </section>
  );
}
