import type { PropertyCard } from "../lib/types.ts";
import {
  sentimentsForAreas,
  themeKindLabel,
  type AreaSentiment,
} from "../lib/areaSentiments.ts";

/* eslint-disable react-refresh/only-export-components */

/** Match Area Tracker: show a row once we have at least 2 priced listings. */
const MIN_SAMPLES = 2;
const MAX_DOTS = 36;

/** Soft Levels.fyi-style row pastels. */
const BAND_PALETTE = [
  { fill: "rgba(176, 186, 230, 0.55)", border: "rgba(112, 124, 186, 0.7)", dot: "rgba(112, 124, 186, 0.42)" },
  { fill: "rgba(168, 214, 214, 0.55)", border: "rgba(72, 148, 148, 0.7)", dot: "rgba(72, 148, 148, 0.42)" },
  { fill: "rgba(236, 176, 168, 0.55)", border: "rgba(196, 108, 96, 0.7)", dot: "rgba(196, 108, 96, 0.42)" },
  { fill: "rgba(230, 186, 206, 0.55)", border: "rgba(176, 108, 144, 0.7)", dot: "rgba(176, 108, 144, 0.42)" },
  { fill: "rgba(232, 198, 140, 0.55)", border: "rgba(188, 140, 56, 0.7)", dot: "rgba(188, 140, 56, 0.42)" },
  { fill: "rgba(176, 214, 186, 0.55)", border: "rgba(72, 148, 104, 0.7)", dot: "rgba(72, 148, 104, 0.42)" },
  { fill: "rgba(198, 188, 230, 0.55)", border: "rgba(128, 112, 186, 0.7)", dot: "rgba(128, 112, 186, 0.42)" },
  { fill: "rgba(186, 214, 230, 0.55)", border: "rgba(88, 140, 176, 0.7)", dot: "rgba(88, 140, 176, 0.42)" },
] as const;

export type PriceBand = {
  area: string;
  p10: number;
  p25: number;
  median: number;
  p75: number;
  p90: number;
  n: number;
  middleSpread: number;
  samples: number[];
  colorIndex: number;
  thin: boolean;
};

export type AreaMarketContext = {
  area: string;
  homePriceMin: number;
  homePriceMax: number;
  bhks: number[];
  societies: number;
};

function percentile(sorted: number[], p: number): number {
  if (sorted.length === 0) return 0;
  if (sorted.length === 1) return sorted[0];
  const index = (p / 100) * (sorted.length - 1);
  const lower = Math.floor(index);
  const upper = Math.ceil(index);
  if (lower === upper) return sorted[lower];
  const weight = index - lower;
  return sorted[lower] * (1 - weight) + sorted[upper] * weight;
}

function samplePrices(prices: number[], limit: number): number[] {
  if (prices.length <= limit) return prices;
  const step = prices.length / limit;
  const picked: number[] = [];
  for (let i = 0; i < limit; i += 1) {
    picked.push(prices[Math.min(prices.length - 1, Math.floor(i * step))]);
  }
  return picked;
}

function bandFromPrices(area: string, prices: number[]): PriceBand | null {
  if (prices.length < MIN_SAMPLES) return null;
  const sorted = [...prices].sort((a, b) => a - b);
  const p10 = Math.round(percentile(sorted, 10));
  const p25 = Math.round(percentile(sorted, 25));
  const median = Math.round(percentile(sorted, 50));
  const p75 = Math.round(percentile(sorted, 75));
  const p90 = Math.round(percentile(sorted, 90));
  return {
    area,
    p10,
    p25,
    median,
    p75,
    p90,
    n: prices.length,
    middleSpread: p75 - p25,
    samples: samplePrices(sorted, MAX_DOTS),
    colorIndex: 0,
    thin: prices.length < 4,
  };
}

export function derivePriceBands(
  properties: PropertyCard[],
  preferredAreas?: string[],
): PriceBand[] {
  const byArea: Record<string, number[]> = {};
  for (const property of properties) {
    if (!property.area || property.price_per_sqft <= 0) continue;
    (byArea[property.area] ??= []).push(property.price_per_sqft);
  }

  // When Area Tracker passes preferred areas, keep that exact set/order.
  if (preferredAreas && preferredAreas.length > 0) {
    return preferredAreas
      .map((area) => bandFromPrices(area, byArea[area] ?? []))
      .filter((band): band is PriceBand => band !== null)
      .map((band, index) => ({
        ...band,
        colorIndex: index % BAND_PALETTE.length,
      }));
  }

  return Object.entries(byArea)
    .map(([area, prices]) => bandFromPrices(area, prices))
    .filter((band): band is PriceBand => band !== null)
    .sort((a, b) => b.n - a.n)
    .map((band, index) => ({
      ...band,
      colorIndex: index % BAND_PALETTE.length,
    }));
}

export function formatSqftCompact(value: number): string {
  if (value >= 1000) {
    const thousands = value / 1000;
    const rounded = thousands >= 10 ? thousands.toFixed(0) : thousands.toFixed(1).replace(/\.0$/, "");
    return `₹${rounded}k`;
  }
  return `₹${value.toLocaleString("en-IN")}`;
}

function formatHomePrice(value: number): string {
  if (value >= 10_000_000) {
    return `₹${(value / 10_000_000).toFixed(1).replace(/\.0$/, "")} Cr`;
  }
  if (value >= 100_000) {
    return `₹${Math.round(value / 100_000)} L`;
  }
  return `₹${value.toLocaleString("en-IN")}`;
}

function niceAxisTicks(min: number, max: number): number[] {
  const span = Math.max(max - min, 1);
  const stepGuess = span / 4;
  const magnitude = 10 ** Math.floor(Math.log10(stepGuess));
  const normalized = stepGuess / magnitude;
  const step =
    normalized <= 1.5 ? magnitude
    : normalized <= 3 ? 2 * magnitude
    : normalized <= 7 ? 5 * magnitude
    : 10 * magnitude;

  const start = Math.floor(min / step) * step;
  const ticks: number[] = [];
  for (let value = start; value <= max + step * 0.01; value += step) {
    if (value >= min - step * 0.05) ticks.push(value);
  }
  return ticks.length >= 2 ? ticks : [min, max];
}

function jitterY(index: number, total: number): number {
  const t = total <= 1 ? 0.5 : index / (total - 1);
  const wave = Math.sin(index * 2.3) * 0.28;
  return 50 + (t - 0.5) * 18 + wave * 100;
}

function BandRow({
  band,
  marketContext,
  scaleMin,
  scaleMax,
  onSelect,
  density,
}: {
  band: PriceBand;
  marketContext?: AreaMarketContext;
  scaleMin: number;
  scaleMax: number;
  onSelect: (area: string) => void;
  density: "standard" | "compact";
}) {
  const span = Math.max(scaleMax - scaleMin, 1);
  const pct = (value: number) => `${((value - scaleMin) / span) * 100}%`;
  const palette = BAND_PALETTE[band.colorIndex];
  const medianLeft = pct(band.median);
  const hasHomePriceRange =
    marketContext && marketContext.homePriceMin > 0 && marketContext.homePriceMax > 0;

  return (
    <button
      type="button"
      className={`price-bands__row${band.thin ? " price-bands__row--thin" : ""}`}
      onClick={() => onSelect(band.area)}
      aria-label={`${band.area} price band, ${formatSqftCompact(band.median)} per sqft`}
    >
      <div className="price-bands__label">
        <span className="price-bands__area">{band.area}</span>
        <span className="price-bands__meta">
          {hasHomePriceRange
            ? `${formatHomePrice(marketContext.homePriceMin)}–${formatHomePrice(marketContext.homePriceMax)}`
            : `${band.n} listing${band.n === 1 ? "" : "s"}`}
          {marketContext && marketContext.bhks.length > 0
            ? ` · ${marketContext.bhks.join(", ")} BHK`
            : ""}
          {band.thin && density !== "compact" ? " · early" : ""}
        </span>
      </div>
      <div className="price-bands__plot" aria-hidden="true">
        <span className="price-bands__axis-line" />
        {band.samples.map((price, index) => (
          <span
            key={`${band.area}-${price}-${index}`}
            className="price-bands__dot"
            style={{
              left: pct(price),
              top: `${jitterY(index, band.samples.length)}%`,
              background: palette.dot,
            }}
          />
        ))}
        <span
          className="price-bands__whisker"
          style={{
            left: pct(band.p10),
            width: `calc(${pct(band.p90)} - ${pct(band.p10)})`,
            background: palette.border,
          }}
        />
        <span className="price-bands__whisker-cap" style={{ left: pct(band.p10), background: palette.border }} />
        <span className="price-bands__whisker-cap" style={{ left: pct(band.p90), background: palette.border }} />
        <span
          className="price-bands__box"
          style={{
            left: pct(band.p25),
            width: `calc(${pct(band.p75)} - ${pct(band.p25)})`,
            background: palette.fill,
            borderColor: palette.border,
          }}
        />
        <span className="price-bands__median" style={{ left: medianLeft }} />
        <span className="price-bands__median-label" style={{ left: medianLeft }}>
          {formatSqftCompact(band.median)}
        </span>
      </div>
    </button>
  );
}

function LocalChatter({ themes }: { themes: AreaSentiment[] }) {
  if (themes.length === 0) return null;
  return (
    <aside className="price-bands__chatter" aria-label="Local chatter">
      <p>Local chatter</p>
      <div>
        {themes.map((theme) => (
          <span
            key={`${theme.kind}-${theme.theme}-${theme.line}`}
            className={`price-bands__chatter-chip price-bands__chatter-chip--${theme.polarity}`}
            title={theme.line}
          >
            <b>{themeKindLabel(theme.kind)}</b>
            {theme.theme}
          </span>
        ))}
      </div>
    </aside>
  );
}

type AreaPriceBandsProps = {
  properties: PropertyCard[];
  preferredAreas?: string[];
  marketContexts?: AreaMarketContext[];
  onSelectArea: (area: string) => void;
  heading?: string;
  subheading?: string;
  density?: "standard" | "compact";
  showCaption?: boolean;
  showLocalChatter?: boolean;
  localChatterLimit?: number;
};

export function AreaPriceBands({
  properties,
  preferredAreas,
  marketContexts,
  onSelectArea,
  heading = "Market map",
  subheading = "",
  density = "standard",
  showCaption = false,
  showLocalChatter = true,
  localChatterLimit = 4,
}: AreaPriceBandsProps) {
  const bands = derivePriceBands(properties, preferredAreas);
  if (bands.length < 1) return null;

  const scaleMin = Math.min(...bands.map((band) => band.p10));
  const scaleMax = Math.max(...bands.map((band) => band.p90));
  const pad = Math.max((scaleMax - scaleMin) * 0.08, 250);
  const axisMin = Math.max(0, scaleMin - pad);
  const axisMax = scaleMax + pad;
  const ticks = niceAxisTicks(axisMin, axisMax);
  const themes = sentimentsForAreas(bands.map((band) => band.area), localChatterLimit);
  const marketContextByArea = new Map(
    marketContexts?.map((context) => [context.area, context]),
  );
  const missingPreferred =
    preferredAreas?.filter((area) => !bands.some((band) => band.area === area)) ?? [];

  return (
    <div className={`price-bands price-bands--${density}`}>
      <div className="price-bands__head">
        <div>
          <h2 className="price-bands__title">{heading}</h2>
          {subheading && <p className="price-bands__sub">{subheading}</p>}
        </div>
      </div>

      <div className="price-bands__body">
        <div className="price-bands__chart">
          <div className="price-bands__axis-row" aria-hidden="true">
            <span className="price-bands__axis-spacer" />
            <div className="price-bands__axis-ticks">
              {ticks.map((tick) => (
                <span key={tick}>{formatSqftCompact(tick)}</span>
              ))}
              <span className="price-bands__axis-unit">/ sqft</span>
            </div>
            <span className="price-bands__axis-spacer price-bands__axis-spacer--value" />
          </div>

          <div className="price-bands__rows">
            {bands.map((band) => (
              <BandRow
                key={band.area}
                band={band}
                marketContext={marketContextByArea.get(band.area)}
                scaleMin={axisMin}
                scaleMax={axisMax}
                onSelect={onSelectArea}
                density={density}
              />
            ))}
          </div>
        </div>

        {showLocalChatter && <LocalChatter themes={themes} />}
      </div>

      {showCaption && missingPreferred.length > 0 && (
        <p className="price-bands__caption">
          {missingPreferred.length} tracker area{missingPreferred.length === 1 ? "" : "s"} still need priced listings
        </p>
      )}
    </div>
  );
}
