import { useEffect, useMemo, useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { ImageWithFallback } from "../components/ImageWithFallback.tsx";
import { getProperties, getProperty } from "../lib/api.ts";
import {
  getShortlistItems,
  removeFromShortlist,
  updateShortlistItem,
  type DecisionTag,
  type ShortlistItem,
} from "../lib/shortlist-store.ts";
import type { PropertyCard as PropertyCardType, PropertyDetailResponse } from "../lib/types.ts";

type FilterMode = "all" | "finalist" | "verify" | "under_median" | "high_trust";

type SortKey =
  | "property"
  | "decisionScore"
  | "price"
  | "pricePerSqft"
  | "priceDelta"
  | "carpetEfficiency"
  | "trustScore"
  | "riskScore"
  | "possession";

type SortState = {
  key: SortKey;
  direction: "asc" | "desc";
};

type SheetRow = {
  id: string;
  title: string;
  area: string;
  societyName: string;
  heroImage: string | null;
  price: number;
  pricePerSqft: number;
  carpetArea: number | null;
  totalArea: number | null;
  carpetEfficiency: number | null;
  bhk: number;
  floor: number | null;
  facing: string;
  possession: string;
  possessionYear: number | null;
  tag: DecisionTag;
  note: string;
  trustScore: number | null;
  trustLabel: string;
  riskScore: number | null;
  riskLabel: string;
  priceDelta: number | null;
  decisionScore: number;
  nextStep: string;
};

const DEFAULT_SORT: SortState = { key: "decisionScore", direction: "desc" };

const SORT_LABELS: Record<SortKey, string> = {
  property: "property",
  decisionScore: "decision score",
  price: "price",
  pricePerSqft: "price / sqft",
  priceDelta: "vs sheet",
  carpetEfficiency: "efficiency",
  trustScore: "trust",
  riskScore: "risk",
  possession: "possession",
};

const TAG_META: Record<DecisionTag, { label: string; className: string }> = {
  watching: { label: "Watching", className: "decision-tag--watching" },
  finalist: { label: "Finalist", className: "decision-tag--finalist" },
  verify: { label: "Verify", className: "decision-tag--verify" },
  stretch: { label: "Stretch", className: "decision-tag--stretch" },
};

const FILTERS: { key: FilterMode; label: string }[] = [
  { key: "all", label: "All" },
  { key: "finalist", label: "Finalists" },
  { key: "verify", label: "Verify" },
  { key: "under_median", label: "Under median" },
  { key: "high_trust", label: "High trust" },
];

function formatPrice(price: number): string {
  if (price >= 10_000_000) return `\u20B9${(price / 10_000_000).toFixed(1)} Cr`;
  if (price >= 100_000) return `\u20B9${(price / 100_000).toFixed(1)} L`;
  return `\u20B9${price.toLocaleString("en-IN")}`;
}

function formatNumber(value: number): string {
  return value.toLocaleString("en-IN");
}

function formatPercent(value: number): string {
  return `${Math.round(value * 100)}%`;
}

function formatDelta(value: number | null): string {
  if (value === null) return "-";
  const pct = Math.abs(Math.round(value * 100));
  if (pct === 0) return "At median";
  return value < 0 ? `${pct}% under` : `${pct}% over`;
}

function formatPossession(status: string): string {
  const normalized = status.replace(/_/g, " ").trim().toLowerCase();
  if (normalized === "ready" || normalized === "ready to move") return "RTM";
  if (normalized === "under construction") return "UC";
  if (normalized === "new launch") return "NL";
  return status;
}

function possessionTone(status: string): "ready" | "construction" | "launch" | "unknown" {
  const normalized = status.toLowerCase();
  if (normalized.includes("rtm")) return "ready";
  if (normalized.includes("uc")) return "construction";
  if (normalized.includes("nl")) return "launch";
  return "unknown";
}

function extractPossessionYear(detail: PropertyDetailResponse): number | null {
  const texts = [
    detail.property.description_summary,
    detail.society?.summary ?? "",
    detail.society?.review_summary ?? "",
  ];

  for (const text of texts) {
    const match = text.match(/\b(20\d{2})\b/);
    if (match) return Number(match[1]);
  }

  if (detail.property.possession_status.toLowerCase().includes("ready")) {
    return detail.society?.year_built ?? null;
  }

  return null;
}

function median(values: number[]): number | null {
  const clean = values.filter((v) => Number.isFinite(v)).sort((a, b) => a - b);
  if (clean.length === 0) return null;
  const mid = Math.floor(clean.length / 2);
  return clean.length % 2 === 0 ? (clean[mid - 1] + clean[mid]) / 2 : clean[mid];
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

function compareNullableNumber(a: number | null, b: number | null): number {
  if (a === null && b === null) return 0;
  if (a === null) return 1;
  if (b === null) return -1;
  return a - b;
}

function trustFrom(card: PropertyCardType, detail: PropertyDetailResponse | undefined): { score: number | null; label: string } {
  if (detail?.confidence_score?.overall != null) {
    return {
      score: detail.confidence_score.overall,
      label: detail.confidence_score.label,
    };
  }

  const root = (detail?.root_source ?? card.root_source ?? "").toLowerCase();
  if (root === "rera") return { score: 0.9, label: "RERA" };
  if (root === "seller") return { score: 0.55, label: "Seller" };
  if (root === "discovered") return { score: 0.42, label: "Pending" };
  return { score: null, label: "Unknown" };
}

function riskFrom(detail: PropertyDetailResponse | undefined): { score: number | null; label: string } {
  if (!detail) return { score: null, label: "Unknown" };
  const legal = detail.property.litigation_risk ?? 0;
  const water = detail.property.waterlogging_risk_score ?? 0;
  const traffic = detail.property.traffic_score ? 1 - detail.property.traffic_score : 0;
  const score = clamp(Math.max(legal, water * 0.75, traffic * 0.45), 0, 1);
  const label = score <= 0.18 ? "Low" : score <= 0.38 ? "Moderate" : "High";
  return { score, label };
}

function decisionScore(params: {
  priceDelta: number | null;
  trustScore: number | null;
  riskScore: number | null;
  carpetEfficiency: number | null;
  medianEfficiency: number | null;
  docScore: number | null;
}): number {
  const valueScore = params.priceDelta === null
    ? 0.56
    : params.priceDelta <= 0
      ? 1
      : clamp(1 - params.priceDelta / 0.35, 0.18, 1);
  const trust = params.trustScore ?? 0.5;
  const risk = params.riskScore === null ? 0.58 : 1 - params.riskScore;
  const efficiency = params.carpetEfficiency !== null && params.medianEfficiency
    ? clamp(params.carpetEfficiency / params.medianEfficiency, 0.65, 1.15) / 1.15
    : 0.56;
  const docs = params.docScore ?? 0.52;

  return Math.round(100 * (
    valueScore * 0.32 +
    trust * 0.26 +
    risk * 0.22 +
    efficiency * 0.12 +
    docs * 0.08
  ));
}

function nextStepFor(row: Omit<SheetRow, "nextStep">): string {
  if (row.riskScore !== null && row.riskScore > 0.38) return "Resolve risk before visit";
  if (row.trustScore !== null && row.trustScore < 0.58) return "Verify source chain";
  if (row.priceDelta !== null && row.priceDelta > 0.08) return "Negotiate against sheet";
  if (row.decisionScore >= 82) return "Move to final visit";
  if (row.carpetEfficiency !== null && row.carpetEfficiency < 0.68) return "Check usable layout";
  return "Keep in active watch";
}

function sortRows(rows: SheetRow[], sortState: SortState): SheetRow[] {
  const direction = sortState.direction === "asc" ? 1 : -1;

  return [...rows].sort((left, right) => {
    let result = 0;

    switch (sortState.key) {
      case "property":
        result = left.title.localeCompare(right.title);
        break;
      case "decisionScore":
        result = left.decisionScore - right.decisionScore;
        break;
      case "price":
        result = left.price - right.price;
        break;
      case "pricePerSqft":
        result = left.pricePerSqft - right.pricePerSqft;
        break;
      case "priceDelta":
        result = compareNullableNumber(left.priceDelta, right.priceDelta);
        break;
      case "carpetEfficiency":
        result = compareNullableNumber(left.carpetEfficiency, right.carpetEfficiency);
        break;
      case "trustScore":
        result = compareNullableNumber(left.trustScore, right.trustScore);
        break;
      case "riskScore":
        result = compareNullableNumber(left.riskScore, right.riskScore);
        break;
      case "possession":
        result = left.possession.localeCompare(right.possession);
        break;
    }

    if (result === 0) result = left.title.localeCompare(right.title);
    return result * direction;
  });
}

type HeaderCellProps = {
  label: string;
  sortKey: SortKey;
  sortState: SortState;
  onSort: (key: SortKey) => void;
  align?: "left" | "right" | "center";
  sticky?: boolean;
};

function HeaderCell({
  label,
  sortKey,
  sortState,
  onSort,
  align = "left",
  sticky = false,
}: HeaderCellProps) {
  const active = sortState.key === sortKey;
  const direction = active ? sortState.direction : null;

  return (
    <th className={`decision-sheet-th decision-sheet-th--${align}${sticky ? " decision-sheet-th--sticky" : ""}`}>
      <button
        type="button"
        className={`decision-sheet-sort${active ? " decision-sheet-sort--active" : ""}`}
        onClick={() => onSort(sortKey)}
      >
        <span>{label}</span>
        <span className="decision-sheet-sort-icon" aria-hidden="true">
          {direction === "asc" ? "\u2191" : direction === "desc" ? "\u2193" : "\u2195"}
        </span>
      </button>
    </th>
  );
}

export function ShortlistPage() {
  const [allProperties, setAllProperties] = useState<PropertyCardType[]>([]);
  const [shortlistItems, setShortlistItems] = useState<ShortlistItem[]>(() => getShortlistItems());
  const [detailMap, setDetailMap] = useState<Record<string, PropertyDetailResponse>>({});
  const [loaded, setLoaded] = useState(false);
  const [detailsLoaded, setDetailsLoaded] = useState(false);
  const [loadError, setLoadError] = useState(false);
  const [sortState, setSortState] = useState<SortState>(DEFAULT_SORT);
  const [filterMode, setFilterMode] = useState<FilterMode>("all");
  const [queryFilter, setQueryFilter] = useState("");
  const navigate = useNavigate();

  useEffect(() => {
    getProperties()
      .then((data) => {
        setAllProperties(data);
        setLoaded(true);
      })
      .catch(() => {
        setLoadError(true);
        setLoaded(true);
      });
  }, []);

  const savedIds = useMemo(() => shortlistItems.map((item) => item.id), [shortlistItems]);

  useEffect(() => {
    if (!loaded || savedIds.length === 0) return;

    Promise.all(
      savedIds.map((id) =>
        getProperty(id)
          .then((detail) => [id, detail] as const)
          .catch(() => null),
      ),
    ).then((entries) => {
      const nextMap: Record<string, PropertyDetailResponse> = {};
      for (const entry of entries) {
        if (entry) nextMap[entry[0]] = entry[1];
      }
      setDetailMap(nextMap);
      setDetailsLoaded(true);
    });
  }, [loaded, savedIds]);

  const propertiesById = useMemo(
    () => new Map(allProperties.map((property) => [property.id, property])),
    [allProperties],
  );

  const savedProperties = useMemo(
    () => shortlistItems
      .map((item) => {
        const property = propertiesById.get(item.id);
        return property ? { property, item } : null;
      })
      .filter(Boolean) as { property: PropertyCardType; item: ShortlistItem }[],
    [propertiesById, shortlistItems],
  );

  const sheetMedianPrice = useMemo(
    () => median(savedProperties.map(({ property }) => property.price_per_sqft).filter((v) => v > 0)),
    [savedProperties],
  );

  const medianEfficiency = useMemo(() => {
    const efficiencies = savedProperties
      .map(({ property }) => {
        const detail = detailMap[property.id];
        const carpet = detail?.property.carpet_area_sqft ?? null;
        const total = detail?.property.super_builtup_sqft ?? property.sqft ?? null;
        return carpet !== null && total !== null && total > 0 ? carpet / total : null;
      })
      .filter((value): value is number => value !== null);
    return median(efficiencies);
  }, [detailMap, savedProperties]);

  const rows = useMemo(() => {
    return savedProperties.map(({ property: card, item }) => {
      const detail = detailMap[card.id];
      const carpetArea = detail?.property.carpet_area_sqft ?? null;
      const totalArea = detail?.property.super_builtup_sqft ?? card.sqft ?? null;
      const carpetEfficiency =
        carpetArea !== null && totalArea !== null && totalArea > 0 ? carpetArea / totalArea : null;
      const trust = trustFrom(card, detail);
      const risk = riskFrom(detail);
      const priceDelta = sheetMedianPrice && card.price_per_sqft > 0
        ? (card.price_per_sqft - sheetMedianPrice) / sheetMedianPrice
        : null;
      const score = decisionScore({
        priceDelta,
        trustScore: trust.score,
        riskScore: risk.score,
        carpetEfficiency,
        medianEfficiency,
        docScore: detail?.property.document_completeness_score ?? null,
      });

      const rowWithoutStep = {
        id: card.id,
        title: card.title,
        area: card.area,
        societyName: card.society_name,
        heroImage: card.hero_image,
        price: card.price,
        pricePerSqft: card.price_per_sqft,
        carpetArea,
        totalArea,
        carpetEfficiency,
        bhk: card.bhk,
        floor: Number.isFinite(card.floor) ? card.floor : null,
        facing: card.facing,
        possession: formatPossession(detail?.property.possession_status ?? card.possession_status),
        possessionYear: detail ? extractPossessionYear(detail) : null,
        tag: item.tag,
        note: item.note,
        trustScore: trust.score,
        trustLabel: trust.label,
        riskScore: risk.score,
        riskLabel: risk.label,
        priceDelta,
        decisionScore: score,
      };

      return {
        ...rowWithoutStep,
        nextStep: nextStepFor(rowWithoutStep),
      } satisfies SheetRow;
    });
  }, [detailMap, medianEfficiency, savedProperties, sheetMedianPrice]);

  const filteredRows = useMemo(() => {
    const q = queryFilter.trim().toLowerCase();
    return rows.filter((row) => {
      const textMatch = !q || [row.title, row.societyName, row.area, row.nextStep, row.note]
        .some((value) => value.toLowerCase().includes(q));
      if (!textMatch) return false;

      if (filterMode === "finalist") return row.tag === "finalist";
      if (filterMode === "verify") return row.tag === "verify" || row.nextStep.toLowerCase().includes("verify") || row.riskLabel === "High";
      if (filterMode === "under_median") return row.priceDelta !== null && row.priceDelta <= 0;
      if (filterMode === "high_trust") return row.trustScore !== null && row.trustScore >= 0.72;
      return true;
    });
  }, [filterMode, queryFilter, rows]);

  const sortedRows = useMemo(() => sortRows(filteredRows, sortState), [filteredRows, sortState]);

  const stats = useMemo(() => {
    const finalists = rows.filter((row) => row.tag === "finalist").length;
    const underMedian = rows.filter((row) => row.priceDelta !== null && row.priceDelta <= 0).length;
    const highTrust = rows.filter((row) => row.trustScore !== null && row.trustScore >= 0.72).length;
    const topScore = rows.length > 0 ? Math.max(...rows.map((row) => row.decisionScore)) : 0;
    return { finalists, underMedian, highTrust, topScore };
  }, [rows]);

  const handleSort = (key: SortKey) => {
    setSortState((current) =>
      current.key === key
        ? { key, direction: current.direction === "asc" ? "desc" : "asc" }
        : {
            key,
            direction: key === "property" || key === "riskScore" || key === "priceDelta" ? "asc" : "desc",
          },
    );
  };

  const handleRemove = (id: string) => {
    removeFromShortlist(id);
    setShortlistItems(getShortlistItems());
  };

  const handleTagChange = (id: string, tag: DecisionTag) => {
    setShortlistItems(updateShortlistItem(id, { tag }));
  };

  const handleNoteChange = (id: string, note: string) => {
    setShortlistItems(updateShortlistItem(id, { note }));
  };

  if (!loaded) {
    return <div className="page-container shortlist-page-state">Loading decision sheet...</div>;
  }

  if (loadError) {
    return (
      <div className="page-container shortlist-page-state shortlist-page-state--error">
        <h2>Could not load data</h2>
        <p>The backend is unavailable. Please try again later.</p>
        <div className="shortlist-page-actions">
          <button className="btn btn-primary" onClick={() => window.location.reload()}>Retry</button>
          <button className="btn btn-outline" onClick={() => navigate("/")}>Return home</button>
        </div>
      </div>
    );
  }

  if (savedProperties.length === 0) {
    return (
      <div className="page-container shortlist-empty-state">
        <div className="shortlist-empty-panel">
          <span className="shortlist-empty-kicker">Decision sheet</span>
          <h1>Your decision sheet is empty</h1>
          <p>
            Save candidates from search and this becomes your buyer workspace:
            benchmarks, trust signals, risk checks, notes, and next actions.
          </p>
          <button className="btn btn-primary" onClick={() => navigate("/results")}>Browse properties</button>
        </div>
      </div>
    );
  }

  return (
    <div className="page-container-wide shortlist-workspace">
      <div className="page-header shortlist-header">
        <div>
          <span className="shortlist-header-kicker">Decision workspace</span>
          <h1>Decision sheet</h1>
          <p>
            One canonical workspace for narrowing candidates. The score combines price discipline,
            trust, risk, usable area, and document strength from the data already loaded for each home.
          </p>
        </div>
        <div className="shortlist-summary">
          <div className="shortlist-summary-card">
            <span>Saved</span>
            <strong>{rows.length}</strong>
          </div>
          <div className="shortlist-summary-card">
            <span>Top score</span>
            <strong>{stats.topScore}</strong>
          </div>
        </div>
      </div>

      <div className="decision-benchmark-strip">
        <div className="decision-benchmark">
          <span>Sheet median</span>
          <strong>{sheetMedianPrice ? `\u20B9${formatNumber(Math.round(sheetMedianPrice))}` : "-"}</strong>
          <small>per sqft baseline</small>
        </div>
        <div className="decision-benchmark">
          <span>Below median</span>
          <strong>{stats.underMedian}</strong>
          <small>priced with leverage</small>
        </div>
        <div className="decision-benchmark">
          <span>High trust</span>
          <strong>{stats.highTrust}</strong>
          <small>strong source chain</small>
        </div>
        <div className="decision-benchmark">
          <span>Finalists</span>
          <strong>{stats.finalists}</strong>
          <small>marked by you</small>
        </div>
      </div>

      <div className="decision-sheet-shell">
        <div className="decision-sheet-toolbar">
          <div className="decision-sheet-toolbar-copy">
            <strong>{filteredRows.length} visible rows</strong>
            <span>Sorted by {SORT_LABELS[sortState.key]} ({sortState.direction})</span>
            {!detailsLoaded && (
              <span className="decision-sheet-loading">Loading trust and risk metrics...</span>
            )}
          </div>
          <div className="decision-sheet-controls">
            <input
              value={queryFilter}
              onChange={(event) => setQueryFilter(event.target.value)}
              className="decision-filter-input"
              placeholder="Filter area, society, note..."
            />
            <div className="decision-filter-tabs" role="group" aria-label="Decision sheet filters">
              {FILTERS.map((filter) => (
                <button
                  key={filter.key}
                  type="button"
                  className={`decision-filter-tab${filterMode === filter.key ? " decision-filter-tab--active" : ""}`}
                  onClick={() => setFilterMode(filter.key)}
                >
                  {filter.label}
                </button>
              ))}
            </div>
          </div>
        </div>

        <div className="decision-sheet-table-wrap">
          <table className="decision-sheet-table decision-sheet-table--workspace">
            <thead>
              <tr>
                <HeaderCell label="Property" sortKey="property" sortState={sortState} onSort={handleSort} sticky />
                <HeaderCell label="Score" sortKey="decisionScore" sortState={sortState} onSort={handleSort} align="center" />
                <th className="decision-sheet-th decision-sheet-th--center">Decision</th>
                <HeaderCell label="Vs sheet" sortKey="priceDelta" sortState={sortState} onSort={handleSort} align="center" />
                <HeaderCell label="Price / sqft" sortKey="pricePerSqft" sortState={sortState} onSort={handleSort} align="right" />
                <HeaderCell label="Trust" sortKey="trustScore" sortState={sortState} onSort={handleSort} align="center" />
                <HeaderCell label="Risk" sortKey="riskScore" sortState={sortState} onSort={handleSort} align="center" />
                <th className="decision-sheet-th">Next action</th>
                <th className="decision-sheet-th decision-sheet-th--center">-</th>
              </tr>
            </thead>
            <tbody>
              {sortedRows.map((row) => {
                const tagMeta = TAG_META[row.tag];
                return (
                  <tr key={row.id} className="decision-sheet-row">
                    <td className="decision-sheet-property-cell">
                      <div className="decision-sheet-property-stack">
                        <Link to={`/property/${row.id}`} className="decision-sheet-property-link">
                          <span className="decision-sheet-thumb">
                            <ImageWithFallback src={row.heroImage} alt={row.title} style={{ width: "100%", height: "100%" }} />
                          </span>
                          <span className="decision-sheet-property-copy">
                            <strong>{row.title}</strong>
                            <span>{row.societyName}</span>
                            <span>
                              {row.area} · {row.bhk} BHK · {formatPrice(row.price)} · {row.carpetEfficiency !== null ? formatPercent(row.carpetEfficiency) : "-"} usable
                            </span>
                          </span>
                        </Link>
                        <input
                          value={row.note}
                          onChange={(event) => handleNoteChange(row.id, event.target.value)}
                          className="decision-note-input decision-note-input--inline"
                          placeholder="Add constraint or reminder"
                          aria-label={`Note for ${row.title}`}
                        />
                      </div>
                    </td>
                    <td className="decision-sheet-td decision-sheet-td--center">
                      <span className={`decision-score-pill${row.decisionScore >= 82 ? " decision-score-pill--strong" : row.decisionScore < 64 ? " decision-score-pill--weak" : ""}`}>
                        {row.decisionScore}
                      </span>
                    </td>
                    <td className="decision-sheet-td decision-sheet-td--center">
                      <select
                        className={`decision-tag-select ${tagMeta.className}`}
                        value={row.tag}
                        onChange={(event) => handleTagChange(row.id, event.target.value as DecisionTag)}
                        aria-label={`Decision tag for ${row.title}`}
                      >
                        {Object.entries(TAG_META).map(([tag, meta]) => (
                          <option key={tag} value={tag}>{meta.label}</option>
                        ))}
                      </select>
                    </td>
                    <td className="decision-sheet-td decision-sheet-td--center">
                      <span className={`decision-delta ${row.priceDelta !== null && row.priceDelta <= 0 ? "decision-delta--good" : row.priceDelta !== null && row.priceDelta > 0.08 ? "decision-delta--watch" : ""}`}>
                        {formatDelta(row.priceDelta)}
                      </span>
                    </td>
                    <td className="decision-sheet-td decision-sheet-td--right">{`\u20B9${formatNumber(row.pricePerSqft)}`}</td>
                    <td className="decision-sheet-td decision-sheet-td--center">
                      <span className={`decision-trust-pill ${row.trustScore !== null && row.trustScore >= 0.72 ? "decision-trust-pill--high" : row.trustScore !== null && row.trustScore < 0.58 ? "decision-trust-pill--low" : ""}`}>
                        {row.trustLabel}
                      </span>
                    </td>
                    <td className="decision-sheet-td decision-sheet-td--center">
                      <span className={`decision-risk-pill decision-risk-pill--${row.riskLabel.toLowerCase()}`}>
                        {row.riskLabel}
                      </span>
                    </td>
                    <td className="decision-sheet-td decision-next-step">{row.nextStep}</td>
                    <td className="decision-sheet-td decision-sheet-td--center">
                      <button
                        type="button"
                        className="decision-sheet-remove"
                        onClick={() => handleRemove(row.id)}
                        aria-label={`Remove ${row.title} from decision sheet`}
                        title="Remove from sheet"
                      >
                        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" aria-hidden="true">
                          <line x1="5" y1="12" x2="19" y2="12" />
                        </svg>
                      </button>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
          {sortedRows.length === 0 && (
            <div className="decision-sheet-no-results">
              No saved candidates match the current sheet filter.
            </div>
          )}
        </div>

        <div className="decision-card-list">
          {sortedRows.length === 0 ? (
            <div className="decision-sheet-no-results">
              No saved candidates match the current sheet filter.
            </div>
          ) : sortedRows.map((row) => {
            const tagMeta = TAG_META[row.tag];
            return (
              <article key={row.id} className="decision-card">
                <div className="decision-card-main">
                  <Link to={`/property/${row.id}`} className="decision-card-property">
                    <span className="decision-sheet-thumb">
                      <ImageWithFallback src={row.heroImage} alt={row.title} style={{ width: "100%", height: "100%" }} />
                    </span>
                    <span className="decision-sheet-property-copy">
                      <strong>{row.title}</strong>
                      <span>{row.societyName}</span>
                      <span>{row.area} · {row.bhk} BHK · {formatPrice(row.price)}</span>
                    </span>
                  </Link>
                  <span className={`decision-score-pill${row.decisionScore >= 82 ? " decision-score-pill--strong" : row.decisionScore < 64 ? " decision-score-pill--weak" : ""}`}>
                    {row.decisionScore}
                  </span>
                </div>

                <div className="decision-card-controls">
                  <select
                    className={`decision-tag-select ${tagMeta.className}`}
                    value={row.tag}
                    onChange={(event) => handleTagChange(row.id, event.target.value as DecisionTag)}
                    aria-label={`Decision tag for ${row.title}`}
                  >
                    {Object.entries(TAG_META).map(([tag, meta]) => (
                      <option key={tag} value={tag}>{meta.label}</option>
                    ))}
                  </select>
                  <button
                    type="button"
                    className="decision-sheet-remove"
                    onClick={() => handleRemove(row.id)}
                    aria-label={`Remove ${row.title} from decision sheet`}
                    title="Remove from sheet"
                  >
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" aria-hidden="true">
                      <line x1="5" y1="12" x2="19" y2="12" />
                    </svg>
                  </button>
                </div>

                <div className="decision-card-signals">
                  <div>
                    <span>Vs sheet</span>
                    <strong className={`decision-delta ${row.priceDelta !== null && row.priceDelta <= 0 ? "decision-delta--good" : row.priceDelta !== null && row.priceDelta > 0.08 ? "decision-delta--watch" : ""}`}>
                      {formatDelta(row.priceDelta)}
                    </strong>
                  </div>
                  <div>
                    <span>Price / sqft</span>
                    <strong>{`\u20B9${formatNumber(row.pricePerSqft)}`}</strong>
                  </div>
                  <div>
                    <span>Trust</span>
                    <strong className={`decision-trust-pill ${row.trustScore !== null && row.trustScore >= 0.72 ? "decision-trust-pill--high" : row.trustScore !== null && row.trustScore < 0.58 ? "decision-trust-pill--low" : ""}`}>
                      {row.trustLabel}
                    </strong>
                  </div>
                  <div>
                    <span>Risk</span>
                    <strong className={`decision-risk-pill decision-risk-pill--${row.riskLabel.toLowerCase()}`}>
                      {row.riskLabel}
                    </strong>
                  </div>
                  <div>
                    <span>Usable</span>
                    <strong>{row.carpetEfficiency !== null ? formatPercent(row.carpetEfficiency) : "-"}</strong>
                  </div>
                  <div>
                    <span>Possession</span>
                    <strong className={`decision-sheet-pill decision-sheet-pill--${possessionTone(row.possession)}`}>
                      {row.possession}
                    </strong>
                  </div>
                </div>

                <div className="decision-card-action">
                  <span>Next action</span>
                  <strong>{row.nextStep}</strong>
                </div>
                <input
                  value={row.note}
                  onChange={(event) => handleNoteChange(row.id, event.target.value)}
                  className="decision-note-input"
                  placeholder="Add constraint or reminder"
                  aria-label={`Note for ${row.title}`}
                />
              </article>
            );
          })}
        </div>
      </div>
    </div>
  );
}
