import { useMemo } from "react";
import { useSearchParams } from "react-router-dom";
import type { PropertyCard } from "../../lib/types.ts";

type ComparableId =
  | "space"
  | "land"
  | "openSpace"
  | "homeState"
  | "builder";

type SocietyGroup = {
  key: string;
  name: string;
  area: string;
  listings: PropertyCard[];
};

type PriceBand = {
  listings: PropertyCard[];
  low: number;
  p25: number;
  median: number;
  p75: number;
  high: number;
  samples: number[];
};

type ComparableValue = {
  primary: string;
};

type ComparableDefinition = {
  id: ComparableId;
  label: string;
  value: (listings: PropertyCard[]) => ComparableValue | null;
};

const MAX_VISIBLE_COMPARABLES = 2;

function formatPrice(price: number): string {
  if (price >= 10_000_000) {
    return `₹${(price / 10_000_000).toFixed(2).replace(/0+$/, "").replace(/\.$/, "")} Cr`;
  }
  if (price >= 100_000) {
    return `₹${(price / 100_000).toFixed(1).replace(/\.0$/, "")} L`;
  }
  return `₹${Math.round(price).toLocaleString("en-IN")}`;
}

function usableSqft(property: PropertyCard): number {
  const candidates = [
    property.carpet_area_sqft,
    property.super_builtup_sqft,
    property.sqft,
  ];
  return candidates.find((value) => typeof value === "number" && value > 0) ?? 0;
}

function numericRange(
  listings: PropertyCard[],
  read: (listing: PropertyCard) => number | null,
  format: (value: number) => string,
): ComparableValue | null {
  const values = listings
    .map(read)
    .filter((value): value is number => value !== null && value > 0);
  if (values.length === 0) return null;
  const min = Math.min(...values);
  const max = Math.max(...values);
  return {
    primary: min === max ? format(min) : `${format(min)}–${format(max)}`,
  };
}

function mostCommonLabel(labels: string[]): string | null {
  if (labels.length === 0) return null;
  const counts = new Map<string, number>();
  for (const label of labels) counts.set(label, (counts.get(label) ?? 0) + 1);
  return [...counts.entries()].sort((left, right) => right[1] - left[1])[0]?.[0] ?? null;
}

function explicitHomeStateLabel(label: string): string {
  const normalized = label.toLowerCase().replace(/[_-]+/g, " ");
  if (normalized.includes("delay")) return "Timeline delayed";
  if (normalized.includes("under construction") || normalized.includes("construction")) {
    return "Under construction";
  }
  return label;
}

function homeStateTone(label: string): "construction" | "delayed" | null {
  const normalized = label.toLowerCase();
  if (normalized.includes("delay")) return "delayed";
  if (normalized.includes("construction")) return "construction";
  return null;
}

const COMPARABLES: ComparableDefinition[] = [
  {
    id: "space",
    label: "Usable space",
    value: (listings) => numericRange(
      listings,
      (listing) => usableSqft(listing),
      (value) => `${Math.round(value).toLocaleString("en-IN")} sqft`,
    ),
  },
  {
    id: "land",
    label: "Acres",
    value: (listings) => numericRange(
      listings,
      (listing) => listing.society_land_acres ?? null,
      (value) => `${value.toFixed(1)} acres`,
    ),
  },
  {
    id: "openSpace",
    label: "Open space",
    value: (listings) => numericRange(
      listings,
      (listing) => listing.open_space_pct ?? null,
      (value) => `${Math.round(value)}%`,
    ),
  },
  {
    id: "homeState",
    label: "Home state",
    value: (listings) => {
      const labels = listings
        .map((listing) =>
          listing.home_state_display
          || listing.project_status_display
          || listing.possession_status
        )
        .filter(Boolean);
      const primary = mostCommonLabel(labels);
      return primary ? { primary: explicitHomeStateLabel(primary) } : null;
    },
  },
  {
    id: "builder",
    label: "Builder",
    value: (listings) => {
      const category = listings.find((listing) => listing.builder_category)?.builder_category;
      return category ? { primary: `Cat ${category}` } : null;
    },
  },
];

function societyKey(property: PropertyCard): string {
  return property.society_name?.trim().toLocaleLowerCase()
    || property.title.trim().toLocaleLowerCase();
}

function buildSocietyGroups(
  selectedHomes: PropertyCard[],
  catalog: PropertyCard[],
): SocietyGroup[] {
  const selectedKeys = [...new Set(selectedHomes.map(societyKey))];
  return selectedKeys.map((key) => {
    const selected = selectedHomes.filter((home) => societyKey(home) === key);
    const matching = catalog.filter((home) => societyKey(home) === key);
    const listings = matching.length > 0 ? matching : selected;
    const representative = selected[0] ?? listings[0];
    return {
      key,
      name: representative.society_name?.trim() || representative.title,
      area: representative.area,
      listings,
    };
  });
}

function percentile(sorted: number[], p: number): number {
  if (sorted.length === 1) return sorted[0];
  const index = (p / 100) * (sorted.length - 1);
  const lower = Math.floor(index);
  const upper = Math.ceil(index);
  if (lower === upper) return sorted[lower];
  const weight = index - lower;
  return sorted[lower] * (1 - weight) + sorted[upper] * weight;
}

function priceBand(listings: PropertyCard[]): PriceBand | null {
  const priced = listings
    .filter((listing) => listing.price > 0)
    .sort((left, right) => left.price - right.price);
  if (priced.length === 0) return null;
  const values = priced.map((listing) => listing.price);
  return {
    listings: priced,
    low: percentile(values, 10),
    p25: percentile(values, 25),
    median: percentile(values, 50),
    p75: percentile(values, 75),
    high: percentile(values, 90),
    samples: values,
  };
}

function chartRange(bands: PriceBand[]): { min: number; max: number } {
  const min = Math.min(...bands.map((band) => band.low));
  const max = Math.max(...bands.map((band) => band.high));
  const spread = Math.max(max - min, max * 0.08, 1);
  return {
    min: Math.max(0, min - spread * 0.12),
    max: max + spread * 0.12,
  };
}

function scalePosition(value: number, min: number, max: number): number {
  if (max <= min) return 50;
  return Math.max(0, Math.min(100, ((value - min) / (max - min)) * 100));
}

function PriceRange({
  band,
  min,
  max,
}: {
  band: PriceBand;
  min: number;
  max: number;
}) {
  const hasRange = band.samples.length > 1 && band.low < band.high;
  const pointPosition = scalePosition(band.median, min, max);
  const haloStart = Math.max(0, pointPosition - 2.2);
  const haloWidth = Math.min(4.4, 100 - haloStart);

  return (
    <div className="compare-price-range">
      <svg viewBox="0 0 100 44" height="44" preserveAspectRatio="none" aria-hidden="true">
        <line className="compare-price-range__axis" x1="0" x2="100" y1="22" y2="22" />
        {hasRange && (
          <>
            <line
              className="compare-price-range__whisker"
              x1={scalePosition(band.low, min, max)}
              x2={scalePosition(band.high, min, max)}
              y1="22"
              y2="22"
            />
            <rect
              className="compare-price-range__box"
              x={scalePosition(band.p25, min, max)}
              y="15"
              width={scalePosition(band.p75, min, max) - scalePosition(band.p25, min, max)}
              height="14"
              rx="2"
            />
            {band.samples.slice(0, 16).map((sample, index) => (
              <circle
                key={`${sample}:${index}`}
                className="compare-price-range__sample"
                cx={scalePosition(sample, min, max)}
                cy={index % 2 === 0 ? 11 : 33}
                r="1.8"
              />
            ))}
          </>
        )}
        {!hasRange && (
          <rect
            className="compare-price-range__single-halo"
            x={haloStart}
            y="15"
            width={haloWidth}
            height="14"
            rx="2"
          />
        )}
        <line
          className="compare-price-range__median"
          x1={pointPosition}
          x2={pointPosition}
          y1="12"
          y2="32"
        />
      </svg>
      <div className={`compare-price-range__labels${hasRange ? "" : " is-single"}`}>
        {hasRange ? (
          <>
            <span>{formatPrice(band.low)}</span>
            <strong>{formatPrice(band.median)}</strong>
            <span>{formatPrice(band.high)}</span>
          </>
        ) : (
          <strong>{formatPrice(band.median)}</strong>
        )}
      </div>
    </div>
  );
}

function LandAreaPendingMark() {
  return (
    <span
      className="compare-land-pending"
      title="Land area is not verified yet"
      role="img"
      aria-label="Land area not verified"
    >
      <svg viewBox="0 0 42 32" aria-hidden="true">
        <path className="compare-land-pending__backdrop" d="M8 3.5h24a6.5 6.5 0 0 1 6.5 6.5v12A6.5 6.5 0 0 1 32 28.5H8A6.5 6.5 0 0 1 1.5 22V10A6.5 6.5 0 0 1 8 3.5Z" />
        <circle className="compare-land-pending__sun" cx="31.5" cy="10" r="2.4" />
        <path className="compare-land-pending__plot" d="m8 22 8.5-6 17 3.5-8.5 6Z" />
        <path className="compare-land-pending__furrow" d="m13 20.4 12.4 2.5M18 17.6l11.8 2.5" />
        <path className="compare-land-pending__sprout" d="M18.5 17v-4.2m0 1.8c-2.8-.2-4.2-1.5-4.4-3.8 2.8-.1 4.2 1.2 4.4 3.8Zm.1-1.8c.4-2.5 1.9-3.7 4.5-3.6-.2 2.5-1.7 3.7-4.5 3.6Z" />
      </svg>
    </span>
  );
}

function ComparableCell({
  definition,
  listings,
}: {
  definition: ComparableDefinition;
  listings: PropertyCard[];
}) {
  const observed = definition.value(listings);
  if (!observed && definition.id === "land") {
    return (
      <div className="compare-fact-cell compare-fact-cell--pending">
        <LandAreaPendingMark />
      </div>
    );
  }
  const statusTone = definition.id === "homeState" && observed
    ? homeStateTone(observed.primary)
    : null;
  return (
    <div className={`compare-fact-cell${observed ? "" : " is-empty"}`}>
      <strong
        className={statusTone ? `compare-fact-status compare-fact-status--${statusTone}` : undefined}
        title={observed?.primary}
      >
        {observed?.primary ?? "—"}
      </strong>
    </div>
  );
}

type SocietyComparisonChartProps = {
  selectedHomes: PropertyCard[];
  catalog: PropertyCard[];
};

export function SocietyComparisonChart({
  selectedHomes,
  catalog,
}: SocietyComparisonChartProps) {
  const [searchParams, setSearchParams] = useSearchParams();
  const groups = useMemo(
    () => buildSocietyGroups(selectedHomes, catalog),
    [catalog, selectedHomes],
  );
  const availableBhks = [...new Set(groups.flatMap((group) =>
    group.listings.map((listing) => listing.bhk)
  ))].sort((left, right) => left - right);
  const requestedBhk = Number(searchParams.get("bhk"));
  const selectedHomeBhk = selectedHomes[0]?.bhk;
  const activeBhk = availableBhks.includes(requestedBhk)
    ? requestedBhk
    : availableBhks.includes(selectedHomeBhk)
      ? selectedHomeBhk
      : availableBhks[0];

  const visibleRows = groups
    .map((group) => ({
      group,
      listings: group.listings.filter((listing) => listing.bhk === activeBhk),
    }))
    .filter((row) => row.listings.length > 0);
  const dataBackedComparables = COMPARABLES.filter((definition) =>
    definition.id === "land"
    || visibleRows.some((row) => definition.value(row.listings) !== null)
  );
  const availableComparables = dataBackedComparables;
  const requestedComparableIds = (searchParams.get("facts") ?? "")
    .split(",")
    .filter((id): id is ComparableId =>
      availableComparables.some((definition) => definition.id === id)
    );
  const defaultComparableIds: ComparableId[] = ["space", "land"];
  const activeComparableIds = [
    ...new Set([
      ...requestedComparableIds,
      ...defaultComparableIds.filter((id) =>
        availableComparables.some((definition) => definition.id === id)
      ),
      ...availableComparables.map((definition) => definition.id),
    ]),
  ].slice(0, MAX_VISIBLE_COMPARABLES);
  const activeComparables = activeComparableIds
    .map((id) => availableComparables.find((definition) => definition.id === id))
    .filter((definition): definition is ComparableDefinition => Boolean(definition));

  const rowsWithBands = visibleRows
    .map((row) => ({ ...row, band: priceBand(row.listings) }))
    .filter((row): row is typeof row & { band: PriceBand } => row.band !== null);
  if (rowsWithBands.length === 0) return null;

  const range = chartRange(rowsWithBands.map((row) => row.band));

  function updateParam(key: string, value: string) {
    const next = new URLSearchParams(searchParams);
    next.set(key, value);
    setSearchParams(next, { replace: true });
  }

  function toggleComparable(id: ComparableId) {
    if (activeComparableIds.includes(id)) {
      if (activeComparableIds.length === 1) return;
      updateParam("facts", activeComparableIds.filter((current) => current !== id).join(","));
      return;
    }
    const next = activeComparableIds.length < MAX_VISIBLE_COMPARABLES
      ? [...activeComparableIds, id]
      : [activeComparableIds[1], id];
    updateParam("facts", next.join(","));
  }

  return (
    <section className="compare-range-chart" aria-labelledby="compare-range-chart-title">
      <header className="compare-range-chart__controls">
        <div>
          <span>Configuration</span>
          <h2 id="compare-range-chart-title">{activeBhk} BHK</h2>
          <div className="compare-filter-group" role="group" aria-label="Filter by BHK">
            {availableBhks.map((bhk) => (
              <button
                key={bhk}
                type="button"
                className={bhk === activeBhk ? "is-active" : ""}
                aria-pressed={bhk === activeBhk}
                onClick={() => updateParam("bhk", String(bhk))}
              >
                {bhk} BHK
              </button>
            ))}
          </div>
        </div>

        <div className="compare-visible-facts">
          <span>Beside price</span>
          <div role="group" aria-label="Visible comparison facts">
            {availableComparables.map((definition) => (
              <button
                key={definition.id}
                type="button"
                className={activeComparableIds.includes(definition.id) ? "is-active" : ""}
                aria-pressed={activeComparableIds.includes(definition.id)}
                onClick={() => toggleComparable(definition.id)}
              >
                {definition.label}
              </button>
            ))}
          </div>
        </div>
      </header>

      <div className={`compare-society-table compare-society-table--facts-${activeComparables.length}`}>
        <div className="compare-society-table__head" aria-hidden="true">
          <span>Society</span>
          <span>Price</span>
          {activeComparables.map((definition) => (
            <span key={definition.id}>{definition.label}</span>
          ))}
        </div>

        <div className="compare-society-table__rows">
          {rowsWithBands.map((row, index) => (
            <div
              key={row.group.key}
              className={`compare-society-table__row compare-society-table__row--tone-${index % 6}`}
            >
              <div className="compare-society-table__identity" title={row.group.name}>
                <span aria-hidden="true">{row.group.name.charAt(0)}</span>
                <div>
                  <strong title={row.group.name}>{row.group.name}</strong>
                  <small>{row.group.area}</small>
                </div>
              </div>
              <PriceRange band={row.band} min={range.min} max={range.max} />
              {activeComparables.map((definition) => (
                <ComparableCell
                  key={definition.id}
                  definition={definition}
                  listings={row.listings}
                />
              ))}
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}
