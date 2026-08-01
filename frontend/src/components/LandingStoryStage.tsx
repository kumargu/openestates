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

function FeaturedSuggestions({ properties }: { properties: PropertyCard[] }) {
  const suggestions = filterListableProperties(properties).slice(0, FEATURED_LIMIT);
  if (suggestions.length === 0) return null;

  return (
    <div className="landing-featured">
      <div className="landing-stage__featured">
        {suggestions.map((property) => (
          <div key={property.id} className="landing-stage__feature-card">
            <LivingEvidenceTile property={property} variant="browse" />
          </div>
        ))}
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
      <div className="landing-showcase__chrome">
        <div className="landing-showcase__tabs" aria-hidden="true">
          <span className={phase === "notes" ? "is-active" : ""}>Notes</span>
          <span className={phase === "compare" ? "is-active" : ""}>Compare</span>
        </div>
        <span className="landing-showcase__live">{pair.map((p) => p.society_name || p.title).join(" · ")}</span>
      </div>

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
      <div className="landing-showcase__chrome">
        <div className="landing-showcase__tabs" aria-hidden="true">
          <span className="is-active">Map</span>
          <span>Risk</span>
          <span>Proof</span>
        </div>
        <span className="landing-showcase__live">{property.society_name || property.title}</span>
      </div>

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
      <div className="landing-showcase__chrome">
        <div className="landing-showcase__tabs" aria-hidden="true">
          <span className={focus === "google" ? "is-active" : ""}>Google</span>
          <span className={focus === "reddit" ? "is-active" : ""}>Reddit</span>
          <span>Themes</span>
        </div>
        <span className="landing-showcase__live">{property.area}</span>
      </div>

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
      <div className="landing-showcase__chrome">
        <div className="landing-showcase__tabs" aria-hidden="true">
          <span className="is-active">Plan</span>
          <span>Buy</span>
          <span>Rent</span>
        </div>
        <span className="landing-showcase__live">{property.area}</span>
      </div>
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

      <FeaturedSuggestions properties={listable} />

      <article className="landing-scene landing-scene--right">
        <div className="landing-scene__copy">
          <p className="landing-scene__kicker">Notebook</p>
          <h2>Notes with tags — then compare</h2>
          <p>
            Write what you notice, pin labels with icons, then the same tags open into a
            labeled diff.
          </p>
          <Link to="/workspace" className="landing-scene__cta">
            Open notebook
          </Link>
        </div>
        <div className="landing-canvas landing-canvas--product">
          <NotebookCompareCanvas homes={notebookHomes} />
        </div>
      </article>

      <article className="landing-scene landing-scene--left">
        <div className="landing-canvas landing-canvas--product">
          <MapEvidenceCanvas property={mapHome} />
        </div>
        <div className="landing-scene__copy">
          <p className="landing-scene__kicker">Map</p>
          <h2>Neighborhood, quieter</h2>
          <p>
            Lake, roads, and home pin on a calm stage — status tones beside each signal,
            not a busy map dump.
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
          <p className="landing-scene__kicker">Sentiment</p>
          <h2>Google and Reddit, already sorted</h2>
          <p>
            Ratings become themes. Local threads become short lines you can scan —
            praise, caution, and tradeoffs side by side.
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

      <article className="landing-scene landing-scene--left landing-scene--compare">
        <div className="landing-canvas landing-canvas--product">
          <PlanCanvas property={planHome} />
        </div>
        <div className="landing-scene__copy">
          <p className="landing-scene__kicker">Plan</p>
          <h2>Watch the tradeoff move</h2>
          <p>
            Buy and rent curves on the same horizon — the year marker slides so the advantage
            feels concrete.
          </p>
          <Link to={`/property/${planHome.id}/plan`} className="landing-scene__cta">
            Open plan
          </Link>
        </div>
      </article>
    </section>
  );
}
