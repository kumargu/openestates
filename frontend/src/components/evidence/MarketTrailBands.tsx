import { useMemo, useState } from "react";
import type { EvidenceSection } from "../../lib/types.ts";

/* eslint-disable react-refresh/only-export-components */

const MAX_DOTS = 16;

const TRAIL_PALETTE = [
  { fill: "rgba(176, 186, 230, 0.55)", border: "rgba(112, 124, 186, 0.7)", dot: "rgba(112, 124, 186, 0.42)" },
  { fill: "rgba(168, 214, 214, 0.55)", border: "rgba(72, 148, 148, 0.7)", dot: "rgba(72, 148, 148, 0.42)" },
  { fill: "rgba(236, 176, 168, 0.55)", border: "rgba(196, 108, 96, 0.7)", dot: "rgba(196, 108, 96, 0.42)" },
  { fill: "rgba(230, 186, 206, 0.55)", border: "rgba(176, 108, 144, 0.7)", dot: "rgba(176, 108, 144, 0.42)" },
] as const;

type MarketTrailRow = {
  bhk: number;
  label: string;
  saleLow: number | null;
  saleHigh: number | null;
  rentLow: number | null;
  rentHigh: number | null;
  rateLow: number | null;
  rateHigh: number | null;
  sourceUrl?: string;
};

function valueTokenToInr(token: string): number | null {
  const cleaned = token
    .replace(/INR/gi, "")
    .replace(/[₹,]/g, "")
    .trim();
  const match = cleaned.match(/(\d+(?:\.\d+)?)\s*(cr|crore|l|lac|lakh)?/i);
  if (!match) return null;
  const value = Number(match[1]);
  if (!Number.isFinite(value)) return null;
  const unit = match[2]?.toLowerCase();
  if (unit === "cr" || unit === "crore") return Math.round(value * 10_000_000);
  if (unit === "l" || unit === "lac" || unit === "lakh") return Math.round(value * 100_000);
  return Math.round(value);
}

function parseCurrencyRange(value: string): [number, number] | null {
  const afterColon = value.split(":").slice(1).join(":") || value;
  const parts = afterColon.split(/\s*-\s*/);
  if (parts.length < 2) {
    const single = valueTokenToInr(afterColon);
    return single == null ? null : [single, single];
  }
  const low = valueTokenToInr(parts[0]);
  const high = valueTokenToInr(parts.slice(1).join("-"));
  if (low == null || high == null) return null;
  return low <= high ? [low, high] : [high, low];
}

function parseRateRange(value: string): [number, number] | null {
  const afterColon = value.split(":").slice(1).join(":") || value;
  const match = afterColon.match(/(\d[\d,]*)\s*-\s*(\d[\d,]*)/);
  if (!match) return null;
  const low = Number(match[1].replace(/,/g, ""));
  const high = Number(match[2].replace(/,/g, ""));
  if (!Number.isFinite(low) || !Number.isFinite(high)) return null;
  return low <= high ? [low, high] : [high, low];
}

function formatTotalPrice(value: number): string {
  if (value >= 10_000_000) {
    return `₹${(value / 10_000_000).toFixed(1).replace(/\.0$/, "")} Cr`;
  }
  if (value >= 100_000) {
    return `₹${(value / 100_000).toFixed(0)} L`;
  }
  return `₹${value.toLocaleString("en-IN")}`;
}

function formatRent(value: number): string {
  if (value >= 100_000) {
    return `₹${(value / 100_000).toFixed(1).replace(/\.0$/, "")} L/mo`;
  }
  return `₹${value.toLocaleString("en-IN")}/mo`;
}

function formatSqftRate(value: number): string {
  return `₹${Math.round(value).toLocaleString("en-IN")}`;
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
  const wave = Math.sin(index * 2.3) * 0.22;
  return 50 + (t - 0.5) * 16 + wave * 100;
}

function sampleRange(low: number, high: number): number[] {
  if (low === high) return [low];
  const samples: number[] = [];
  const steps = Math.min(MAX_DOTS, 9);
  for (let i = 0; i < steps; i += 1) {
    const pct = steps === 1 ? 0.5 : i / (steps - 1);
    samples.push(Math.round(low + (high - low) * pct));
  }
  return samples;
}

function marketSection(sections: EvidenceSection[]): EvidenceSection | undefined {
  return sections.find((section) => section.kind === "market");
}

export function hasMarketTrend(sections: EvidenceSection[]): boolean {
  const section = marketSection(sections);
  return section ? deriveMarketTrailRows(section).length > 0 : false;
}

function deriveMarketTrailRows(section: EvidenceSection): MarketTrailRow[] {
  const rows = new Map<number, MarketTrailRow>();

  for (const item of section.items) {
    const label = item.label || item.value;
    const bhkMatch = label.match(/(\d+(?:\.\d+)?)\s*BHK/i);
    if (!bhkMatch) continue;
    const bhk = Number(bhkMatch[1]);
    if (!Number.isFinite(bhk)) continue;
    const row = rows.get(bhk) ?? {
      bhk,
      label: `${bhk.toLocaleString("en-IN")} BHK`,
      saleLow: null,
      saleHigh: null,
      rentLow: null,
      rentHigh: null,
      rateLow: null,
      rateHigh: null,
      sourceUrl: item.source_url,
    };

    if (/listing price range/i.test(label)) {
      const range = parseCurrencyRange(item.value);
      if (range) [row.saleLow, row.saleHigh] = range;
    } else if (/monthly rent/i.test(label)) {
      const range = parseCurrencyRange(item.value);
      if (range) [row.rentLow, row.rentHigh] = range;
    } else if (/listing rate range/i.test(label)) {
      const range = parseRateRange(item.value);
      if (range) [row.rateLow, row.rateHigh] = range;
    }

    row.sourceUrl = row.sourceUrl ?? item.source_url;
    rows.set(bhk, row);
  }

  return [...rows.values()]
    .filter((row) => row.saleLow != null && row.saleHigh != null)
    .sort((a, b) => a.bhk - b.bhk);
}

function TrailRow({
  row,
  index,
  scaleMin,
  scaleMax,
}: {
  row: MarketTrailRow;
  index: number;
  scaleMin: number;
  scaleMax: number;
}) {
  const saleLow = row.saleLow ?? 0;
  const saleHigh = row.saleHigh ?? saleLow;
  const median = Math.round((saleLow + saleHigh) / 2);
  const span = Math.max(scaleMax - scaleMin, 1);
  const pct = (value: number) => `${((value - scaleMin) / span) * 100}%`;
  const palette = TRAIL_PALETTE[index % TRAIL_PALETTE.length];
  const samples = sampleRange(saleLow, saleHigh);
  const lowLeft = pct(saleLow);
  const highLeft = pct(saleHigh);
  const medianLeft = pct(median);
  const rentLabel =
    row.rentLow != null && row.rentHigh != null
      ? `${formatRent(row.rentLow)}-${formatRent(row.rentHigh).replace("₹", "")}`
      : null;
  const rateLabel =
    row.rateLow != null && row.rateHigh != null
      ? `${formatSqftRate(row.rateLow)}-${formatSqftRate(row.rateHigh).replace("₹", "")}/sqft`
      : null;

  return (
    <div className="price-bands__row market-trend__row">
      <div className="price-bands__label">
        <span className="price-bands__area">{row.label}</span>
        <span className="price-bands__meta">
          {[rentLabel, rateLabel].filter(Boolean).join(" · ")}
        </span>
      </div>
      <div className="price-bands__plot" aria-hidden="true">
        <span className="price-bands__axis-line" />
        {samples.map((price, sampleIndex) => (
          <span
            key={`${row.label}-${price}-${sampleIndex}`}
            className="price-bands__dot"
            style={{
              left: pct(price),
              top: `${jitterY(sampleIndex, samples.length)}%`,
              background: palette.dot,
            }}
          />
        ))}
        <span
          className="price-bands__whisker"
          style={{
            left: lowLeft,
            width: `calc(${highLeft} - ${lowLeft})`,
            background: palette.border,
          }}
        />
        <span className="price-bands__whisker-cap" style={{ left: lowLeft, background: palette.border }} />
        <span className="price-bands__whisker-cap" style={{ left: highLeft, background: palette.border }} />
        <span
          className="price-bands__box"
          style={{
            left: lowLeft,
            width: `calc(${highLeft} - ${lowLeft})`,
            background: palette.fill,
            borderColor: palette.border,
          }}
        />
        <span className="price-bands__median" style={{ left: medianLeft }} />
        <span className="price-bands__median-label" style={{ left: medianLeft }}>
          {formatTotalPrice(median)}
        </span>
        <span className="price-bands__p-label price-bands__p-label--low" style={{ left: lowLeft }}>
          {formatTotalPrice(saleLow)}
        </span>
        <span className="price-bands__p-label price-bands__p-label--high" style={{ left: highLeft }}>
          {formatTotalPrice(saleHigh)}
        </span>
      </div>
      <div className="price-bands__value">
        <strong>{formatTotalPrice(median)}</strong>
        <span>middle</span>
      </div>
    </div>
  );
}

function MarketTrendChart({ rows }: { rows: MarketTrailRow[] }) {
  const lows = rows.map((row) => row.saleLow).filter((value): value is number => value != null);
  const highs = rows.map((row) => row.saleHigh).filter((value): value is number => value != null);
  const scaleMin = Math.min(...lows);
  const scaleMax = Math.max(...highs);
  const pad = Math.max((scaleMax - scaleMin) * 0.08, 500_000);
  const axisMin = Math.max(0, scaleMin - pad);
  const axisMax = scaleMax + pad;
  const ticks = niceAxisTicks(axisMin, axisMax);
  const sourceCount = new Set(rows.map((row) => row.sourceUrl).filter(Boolean)).size;

  return (
    <>
      <div className="price-bands__chart">
        <div className="price-bands__axis-row" aria-hidden="true">
          <span className="price-bands__axis-spacer" />
          <div className="price-bands__axis-ticks">
            {ticks.map((tick) => (
              <span key={tick}>{formatTotalPrice(tick)}</span>
            ))}
            <span className="price-bands__axis-unit">ask</span>
          </div>
          <span className="price-bands__axis-spacer price-bands__axis-spacer--value" />
        </div>

        <div className="price-bands__rows">
          {rows.map((row, index) => (
            <TrailRow
              key={row.bhk}
              row={row}
              index={index}
              scaleMin={axisMin}
              scaleMax={axisMax}
            />
          ))}
        </div>

        <div className="price-bands__legend" aria-hidden="true">
          <span className="price-bands__legend-item">
            <span className="price-bands__legend-box" />
            Ask range
          </span>
          <span className="price-bands__legend-item">
            <span className="price-bands__legend-median" />
            Middle ask
          </span>
          <span className="price-bands__legend-item">
            <span className="price-bands__legend-dot" />
            Cluster
          </span>
        </div>
      </div>

      <p className="price-bands__caption">
        {rows.length} configuration{rows.length === 1 ? "" : "s"}
        {sourceCount > 0 ? ` · ${sourceCount} market source${sourceCount === 1 ? "" : "s"}` : ""}
      </p>
    </>
  );
}

function trendSummary(rows: MarketTrailRow[]): string {
  const lows = rows.map((row) => row.saleLow).filter((value): value is number => value != null);
  const highs = rows.map((row) => row.saleHigh).filter((value): value is number => value != null);
  if (lows.length === 0 || highs.length === 0) {
    return `${rows.length} configuration${rows.length === 1 ? "" : "s"}`;
  }
  return `${rows.length} configuration${rows.length === 1 ? "" : "s"} · ${formatTotalPrice(Math.min(...lows))}-${formatTotalPrice(Math.max(...highs)).replace("₹", "")}`;
}

export function MarketTrendTile({ sections }: { sections: EvidenceSection[] }) {
  const section = marketSection(sections);
  const rows = useMemo(
    () => (section ? deriveMarketTrailRows(section) : []),
    [section],
  );
  const [open, setOpen] = useState(false);

  if (!section || rows.length === 0) return null;

  const summary = trendSummary(rows);
  const previewRows = rows.slice(0, 4);

  return (
    <section className="market-trend" aria-labelledby="market-trend-title">
      <button
        type="button"
        className={`detail-action-tile market-trend__tile${open ? " is-open" : ""}`}
        aria-expanded={open}
        aria-controls="market-trend-panel"
        onClick={() => setOpen((current) => !current)}
      >
        <span className="market-trend__preview" aria-hidden="true">
          {previewRows.map((row) => (
            <span key={row.bhk}>
              <i />
            </span>
          ))}
        </span>
        <span className="market-trend__copy">
          <span className="market-trend__kicker">Market trend</span>
          <strong id="market-trend-title">Configuration price bands</strong>
          <span>{summary}</span>
        </span>
        <span className="market-trend__open" aria-hidden="true">
          {open ? "Hide trend" : "View trend"}
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <path d="m9 18 6-6-6-6" />
          </svg>
        </span>
      </button>

      {open && (
        <div id="market-trend-panel" className="market-trend__panel">
          <div className="market-trend__panel-head">
            <span>Market trend</span>
            <strong>Configuration price bands</strong>
            <p>Asking ranges by configuration.</p>
          </div>
          <div className="market-trend__chart price-bands">
            <MarketTrendChart rows={rows} />
          </div>
        </div>
      )}
    </section>
  );
}
