import { useEffect, useMemo, useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { ImageWithFallback } from "../components/ImageWithFallback.tsx";
import { getProperties, getProperty } from "../lib/api.ts";
import { getShortlistedIds, toggleShortlist } from "../lib/shortlist-store.ts";
import type { PropertyCard as PropertyCardType, PropertyDetailResponse } from "../lib/types.ts";

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

function formatPossession(status: string): string {
  const normalized = status.replace(/_/g, " ").trim().toLowerCase();
  if (normalized === "ready" || normalized === "ready to move") return "RTM";
  if (normalized === "under construction") return "UC";
  if (normalized === "new launch") return "NL";
  return status;
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

function possessionTone(status: string): "ready" | "construction" | "launch" | "unknown" {
  const normalized = status.toLowerCase();
  if (normalized.includes("rtm")) return "ready";
  if (normalized.includes("uc")) return "construction";
  if (normalized.includes("nl")) return "launch";
  return "unknown";
}

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
};

type SortKey =
  | "property"
  | "price"
  | "pricePerSqft"
  | "carpetArea"
  | "totalArea"
  | "carpetEfficiency"
  | "bhk"
  | "floor"
  | "facing"
  | "possession"
  | "possessionYear";

type SortState = {
  key: SortKey;
  direction: "asc" | "desc";
};

const DEFAULT_SORT: SortState = { key: "price", direction: "asc" };

const SORT_LABELS: Record<SortKey, string> = {
  property: "property",
  price: "price",
  pricePerSqft: "price / sqft",
  carpetArea: "carpet area",
  totalArea: "total area",
  carpetEfficiency: "carpet efficiency",
  bhk: "BHK",
  floor: "floor",
  facing: "facing",
  possession: "possession",
  possessionYear: "possession year",
};

function compareNullableNumber(a: number | null, b: number | null): number {
  if (a === null && b === null) return 0;
  if (a === null) return 1;
  if (b === null) return -1;
  return a - b;
}

function sortRows(rows: SheetRow[], sortState: SortState): SheetRow[] {
  const direction = sortState.direction === "asc" ? 1 : -1;

  return [...rows].sort((left, right) => {
    let result = 0;

    switch (sortState.key) {
      case "property":
        result = left.title.localeCompare(right.title);
        break;
      case "price":
        result = left.price - right.price;
        break;
      case "pricePerSqft":
        result = left.pricePerSqft - right.pricePerSqft;
        break;
      case "carpetArea":
        result = compareNullableNumber(left.carpetArea, right.carpetArea);
        break;
      case "totalArea":
        result = compareNullableNumber(left.totalArea, right.totalArea);
        break;
      case "carpetEfficiency":
        result = compareNullableNumber(left.carpetEfficiency, right.carpetEfficiency);
        break;
      case "bhk":
        result = left.bhk - right.bhk;
        break;
      case "floor":
        result = compareNullableNumber(left.floor, right.floor);
        break;
      case "facing":
        result = left.facing.localeCompare(right.facing);
        break;
      case "possession":
        result = left.possession.localeCompare(right.possession);
        break;
      case "possessionYear":
        result = compareNullableNumber(left.possessionYear, right.possessionYear);
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
  const [savedIds, setSavedIds] = useState<string[]>(() => getShortlistedIds());
  const [detailMap, setDetailMap] = useState<Record<string, PropertyDetailResponse>>({});
  const [loaded, setLoaded] = useState(false);
  const [detailsLoaded, setDetailsLoaded] = useState(false);
  const [loadError, setLoadError] = useState(false);
  const [sortState, setSortState] = useState<SortState>(DEFAULT_SORT);
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

  useEffect(() => {
    if (!loaded || savedIds.length === 0) {
      setDetailMap({});
      setDetailsLoaded(true);
      return;
    }

    setDetailsLoaded(false);

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
    () => savedIds.map((id) => propertiesById.get(id)).filter(Boolean) as PropertyCardType[],
    [propertiesById, savedIds],
  );

  const rows = useMemo(() => {
    return savedProperties.map((card) => {
      const detail = detailMap[card.id];
      const carpetArea = detail?.property.carpet_area_sqft ?? null;
      const totalArea = detail?.property.super_builtup_sqft ?? card.sqft ?? null;
      const carpetEfficiency =
        carpetArea !== null && totalArea !== null && totalArea > 0 ? carpetArea / totalArea : null;

      return {
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
      } satisfies SheetRow;
    });
  }, [detailMap, savedProperties]);

  const sortedRows = useMemo(() => sortRows(rows, sortState), [rows, sortState]);

  const handleSort = (key: SortKey) => {
    setSortState((current) =>
      current.key === key
        ? { key, direction: current.direction === "asc" ? "desc" : "asc" }
        : {
            key,
            direction:
              key === "property" || key === "facing" || key === "possession" ? "asc" : "desc",
          },
    );
  };

  const handleRemove = (id: string) => {
    toggleShortlist(id);
    setSavedIds(getShortlistedIds());
  };

  if (!loaded) {
    return <div className="page-container shortlist-page-state">Loading shortlist...</div>;
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
          <h1>Your shortlist is empty</h1>
          <p>
            Save a few candidates from results and this page turns into your decision table:
            price, carpet area, efficiency, and possession timing in one place.
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
          <span className="shortlist-header-kicker">Shortlist workspace</span>
          <h1>Decision sheet</h1>
          <p>
            This is where the shortlist becomes a buy-side worksheet. Compare hard numbers, sort fast,
            and remove weak candidates without leaving the table.
          </p>
        </div>
        <div className="shortlist-summary">
          <div className="shortlist-summary-card">
            <span>Saved</span>
            <strong>{savedProperties.length}</strong>
          </div>
        </div>
      </div>

      <div className="decision-sheet-shell">
        <div className="decision-sheet-toolbar">
          <div className="decision-sheet-toolbar-copy">
            <strong>{savedProperties.length} live rows</strong>
            <span>Sorted by {SORT_LABELS[sortState.key]} ({sortState.direction})</span>
          </div>
          {!detailsLoaded && (
            <span className="decision-sheet-loading">Loading detailed sheet metrics...</span>
          )}
        </div>

        <div className="decision-sheet-table-wrap">
          <table className="decision-sheet-table">
            <thead>
              <tr>
                <HeaderCell label="Property" sortKey="property" sortState={sortState} onSort={handleSort} sticky />
                <HeaderCell label="Price" sortKey="price" sortState={sortState} onSort={handleSort} align="right" />
                <HeaderCell label="Price / sqft" sortKey="pricePerSqft" sortState={sortState} onSort={handleSort} align="right" />
                <HeaderCell label="Carpet area" sortKey="carpetArea" sortState={sortState} onSort={handleSort} align="right" />
                <HeaderCell label="Total area" sortKey="totalArea" sortState={sortState} onSort={handleSort} align="right" />
                <HeaderCell label="Carpet efficiency" sortKey="carpetEfficiency" sortState={sortState} onSort={handleSort} align="right" />
                <HeaderCell label="BHK" sortKey="bhk" sortState={sortState} onSort={handleSort} align="center" />
                <HeaderCell label="Floor" sortKey="floor" sortState={sortState} onSort={handleSort} align="center" />
                <HeaderCell label="Facing" sortKey="facing" sortState={sortState} onSort={handleSort} align="center" />
                <HeaderCell label="Possession" sortKey="possession" sortState={sortState} onSort={handleSort} align="center" />
                <HeaderCell label="Possession year" sortKey="possessionYear" sortState={sortState} onSort={handleSort} align="center" />
                <th className="decision-sheet-th decision-sheet-th--center">-</th>
              </tr>
            </thead>
            <tbody>
              {sortedRows.map((row) => (
                <tr key={row.id} className="decision-sheet-row">
                  <td className="decision-sheet-property-cell">
                    <Link to={`/property/${row.id}`} className="decision-sheet-property-link">
                      <span className="decision-sheet-thumb">
                        <ImageWithFallback src={row.heroImage} alt={row.title} style={{ width: "100%", height: "100%" }} />
                      </span>
                      <span className="decision-sheet-property-copy">
                        <strong>{row.title}</strong>
                        <span>{row.societyName}</span>
                        <span>{row.area}</span>
                      </span>
                    </Link>
                  </td>
                  <td className="decision-sheet-td decision-sheet-td--right">{formatPrice(row.price)}</td>
                  <td className="decision-sheet-td decision-sheet-td--right">{`\u20B9${formatNumber(row.pricePerSqft)}`}</td>
                  <td className="decision-sheet-td decision-sheet-td--right">
                    {row.carpetArea !== null ? `${formatNumber(row.carpetArea)} sqft` : "\u2014"}
                  </td>
                  <td className="decision-sheet-td decision-sheet-td--right">
                    {row.totalArea !== null ? `${formatNumber(row.totalArea)} sqft` : "\u2014"}
                  </td>
                  <td className="decision-sheet-td decision-sheet-td--right">
                    {row.carpetEfficiency !== null ? <span className="decision-sheet-efficiency">{formatPercent(row.carpetEfficiency)}</span> : "\u2014"}
                  </td>
                  <td className="decision-sheet-td decision-sheet-td--center">
                    <span className="decision-sheet-pill decision-sheet-pill--bhk">{row.bhk}</span>
                  </td>
                  <td className="decision-sheet-td decision-sheet-td--center">{row.floor ?? "\u2014"}</td>
                  <td className="decision-sheet-td decision-sheet-td--center">{row.facing}</td>
                  <td className="decision-sheet-td decision-sheet-td--center">
                    <span className={`decision-sheet-pill decision-sheet-pill--${possessionTone(row.possession)}`}>
                      {row.possession}
                    </span>
                  </td>
                  <td className="decision-sheet-td decision-sheet-td--center">{row.possessionYear ?? "\u2014"}</td>
                  <td className="decision-sheet-td decision-sheet-td--center">
                    <button
                      type="button"
                      className="decision-sheet-remove"
                      onClick={() => handleRemove(row.id)}
                      aria-label={`Remove ${row.title} from shortlist`}
                      title="Remove from sheet"
                    >
                      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" aria-hidden="true">
                        <line x1="5" y1="12" x2="19" y2="12" />
                      </svg>
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
}
