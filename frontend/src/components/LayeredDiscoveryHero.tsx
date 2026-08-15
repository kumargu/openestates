import { useEffect, useMemo, useState } from "react";
import { Link } from "react-router-dom";
import { propertyDetailPath } from "../lib/api.ts";
import { filterListableProperties, uniqueSocietiesForDiscovery } from "../lib/property-filters.ts";
import type { PropertyCard } from "../lib/types.ts";

const DEMO_QUERY = "Quiet 3BHK near metro under 2Cr";
const HERO_PHASE_DELAYS = [1_450, 3_350, 7_600] as const;

type LayeredDiscoveryHeroProps = {
  properties: PropertyCard[];
  query: string;
  onQueryChange: (query: string) => void;
  onSearch: (query: string) => void;
};

function hasKnownNumber(value: number | null | undefined): value is number {
  return typeof value === "number" && Number.isFinite(value) && value > 0;
}

function formatPrice(price: number): string {
  if (!hasKnownNumber(price)) return "Price on request";
  if (price >= 10_000_000) return `₹${(price / 10_000_000).toFixed(1)} Cr`;
  return `₹${(price / 100_000).toFixed(1)} L`;
}

function displayName(property: PropertyCard): string {
  const society = property.society_name.trim();
  return society && society.toLowerCase() !== "unknown" ? society : property.title;
}

function rankForDemo(properties: PropertyCard[]): PropertyCard[] {
  const homes = uniqueSocietiesForDiscovery(filterListableProperties(properties));
  const exactIntentHomes = homes.filter((property) => property.bhk === 3 && property.price <= 20_000_000);
  return [...(exactIntentHomes.length >= 3 ? exactIntentHomes : homes)]
    .sort((left, right) => (
      left.metro_distance_mins - right.metro_distance_mins
      || (right.google_rating ?? 0) - (left.google_rating ?? 0)
      || left.price - right.price
    ))
    .slice(0, 3);
}

function matchReasons(property: PropertyCard): string[] {
  const reasons: string[] = [];
  if (property.bhk === 3) reasons.push("3 BHK");
  if (property.price <= 20_000_000) reasons.push("Within ₹2Cr");
  if (hasKnownNumber(property.metro_distance_mins)) reasons.push(`${property.metro_distance_mins} min metro`);
  if (hasKnownNumber(property.google_rating)) reasons.push(`Google ${property.google_rating.toFixed(1)}`);
  return reasons.slice(0, 3);
}

function HeroHomeCard({
  property,
  position,
}: {
  property: PropertyCard;
  position: "left" | "lead" | "right";
}) {
  const reasons = matchReasons(property);

  return (
    <article className={`layered-home-card layered-home-card--${position}`}>
      <div className={`layered-home-card__photo layered-home-card__photo--${position}`} aria-hidden="true">
        <span>{position === "lead" ? "Sponsored launch" : "Sponsored"}</span>
      </div>
      <div className="layered-home-card__body">
        <div className="layered-home-card__identity">
          <h2>{displayName(property)}</h2>
          <strong>{formatPrice(property.price)}</strong>
        </div>
        <p>{property.area} · {property.bhk} BHK · {property.sqft.toLocaleString("en-IN")} sqft</p>
        <div className="layered-home-card__reasons" aria-label="Home facts">
          {reasons.map((reason) => <span key={reason}>{reason}</span>)}
        </div>
        <Link to={propertyDetailPath(property.id)}>
          Explore launch <span aria-hidden="true">→</span>
        </Link>
      </div>
    </article>
  );
}

export function LayeredDiscoveryHero({
  properties,
  query,
  onQueryChange,
  onSearch,
}: LayeredDiscoveryHeroProps) {
  const [reducedMotion] = useState(
    () => typeof window !== "undefined" && window.matchMedia("(prefers-reduced-motion: reduce)").matches,
  );
  const [phase, setPhase] = useState(reducedMotion ? 2 : 0);
  const [cycle, setCycle] = useState(0);
  const [demoText, setDemoText] = useState(reducedMotion ? DEMO_QUERY : "");
  const [focused, setFocused] = useState(false);
  const homes = useMemo(() => rankForDemo(properties), [properties]);

  useEffect(() => {
    if (reducedMotion) return undefined;

    const reset = window.setTimeout(() => {
      setDemoText("");
      setPhase(0);
    }, 0);
    let characterIndex = 0;
    const typing = window.setInterval(() => {
      characterIndex += 1;
      setDemoText(DEMO_QUERY.slice(0, characterIndex));
      if (characterIndex >= DEMO_QUERY.length) window.clearInterval(typing);
    }, 36);
    const phaseTimers = HERO_PHASE_DELAYS.map((delay, index) => window.setTimeout(() => {
      if (index === HERO_PHASE_DELAYS.length - 1) setCycle((current) => current + 1);
      else setPhase(index + 1);
    }, delay));

    return () => {
      window.clearTimeout(reset);
      window.clearInterval(typing);
      phaseTimers.forEach((timer) => window.clearTimeout(timer));
    };
  }, [cycle, reducedMotion]);

  const submitQuery = () => onSearch(query.trim() || DEMO_QUERY);

  if (homes.length < 3) return null;

  return (
    <section className="layered-hero" data-phase={phase} aria-labelledby="layered-hero-title">
      <div className="layered-hero__wash" aria-hidden="true" />
      <Link className="layered-hero__brand" to="/" aria-label="OpenEstates home">OpenEstates</Link>
      <div className="layered-hero__inner">
        <div className="layered-hero__headline">
          <h1 id="layered-hero-title">Find homes with proof you can trust</h1>
          <span aria-hidden="true" />
        </div>

        <form
          className={`layered-hero__search${focused ? " is-focused" : ""}`}
          role="search"
          aria-label="Search homes"
          onSubmit={(event) => {
            event.preventDefault();
            submitQuery();
          }}
        >
          <span className="layered-hero__spark" aria-hidden="true">✦</span>
          {!query && !focused ? <span className="layered-hero__demo" aria-hidden="true">{demoText}<i /></span> : null}
          <input
            type="text"
            value={query}
            onChange={(event) => onQueryChange(event.target.value)}
            onFocus={() => setFocused(true)}
            onBlur={() => setFocused(false)}
            aria-label="Describe the property you are looking for"
            autoComplete="off"
          />
          <button type="submit" aria-label="Search">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
              <path d="M5 12h14M13 6l6 6-6 6" />
            </svg>
          </button>
        </form>

        <div className="layered-hero__cards">
          <HeroHomeCard property={homes[1]} position="left" />
          <HeroHomeCard property={homes[0]} position="lead" />
          <HeroHomeCard property={homes[2]} position="right" />
        </div>

        <p className="layered-hero__receipt"><span aria-hidden="true">◇</span> Sponsored placements are clearly labelled.</p>
      </div>
    </section>
  );
}
