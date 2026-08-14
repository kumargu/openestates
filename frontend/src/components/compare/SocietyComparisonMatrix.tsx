import { useMemo } from "react";
import { Link } from "react-router-dom";
import { useNotebook } from "../../hooks/useNotebook.ts";
import { floorPlanForBhk, type FloorPlanComparePlan } from "../../lib/floor-plan-compare.ts";
import {
  decisionLabelFacets,
  mapContextFacets,
  notebookNoteFacets,
  propertyBaselineFacets,
} from "../../lib/decisionFacets.ts";
import {
  buildCompareProjection,
  formatCompareCell,
  type CompareProjectionRow,
} from "../../lib/compareProjection.ts";
import type { PropertyCard, PropertyDetailResponse } from "../../lib/types.ts";
import { workspaceBuyVsRentHref } from "../../lib/workspaceNav.ts";

type CompareHomeColumn = {
  key: string;
  name: string;
  area: string;
  configuration: string;
  propertyId: string;
  listing: PropertyCard;
};

function buildHomeColumns(selectedHomes: PropertyCard[]): CompareHomeColumn[] {
  return selectedHomes.map((home) => ({
    key: home.id,
    name: home.society_name?.trim() || home.title,
    area: home.area,
    configuration: `${home.bhk} BHK`,
    propertyId: home.id,
    listing: home,
  }));
}

function formatSqft(value: number | undefined): string | null {
  if (typeof value !== "number" || !Number.isFinite(value) || value <= 0) return null;
  return `${Math.round(value).toLocaleString("en-IN")} sqft`;
}

function formatUsableRatio(value: number | undefined): string | null {
  if (typeof value !== "number" || !Number.isFinite(value) || value <= 0) return null;
  return `${Math.round(value * 100)}%`;
}

function CompareHomeHeader({
  column,
  index,
  compareIds,
  onRemove,
}: {
  column: CompareHomeColumn;
  index: number;
  compareIds: string[];
  onRemove?: (propertyIds: string[]) => void;
}) {
  const identityMeta = [column.configuration, column.area.trim()]
    .filter(Boolean)
    .join(" · ");
  return (
    <article className="compare-editorial__home">
      {onRemove && (
        <button
          type="button"
          className="compare-editorial__remove"
          aria-label={`Remove ${column.name} from compare`}
          onClick={() => onRemove([column.propertyId])}
        >
          Remove
        </button>
      )}
      <div className="compare-editorial__home-identity">
        <i aria-hidden="true">{String(index + 1).padStart(2, "0")}</i>
        <Link to={`/property/${encodeURIComponent(column.propertyId)}`}>
          <strong>{column.name}</strong>
        </Link>
        <span>{identityMeta}</span>
        <div className="compare-editorial__home-actions">
          <Link to={`/property/${encodeURIComponent(column.propertyId)}`}>Open home</Link>
          <Link to={workspaceBuyVsRentHref(column.propertyId, compareIds)}>Rent vs buy</Link>
        </div>
      </div>
    </article>
  );
}

function FloorPlanMetrics({ plan }: { plan: FloorPlanComparePlan }) {
  const carpet = formatSqft(plan.carpetAreaSqft);
  const sale = formatSqft(plan.saleAreaSqft);
  const usable = formatUsableRatio(plan.usableAreaRatio);
  const metrics = [
    carpet ? ["Carpet", carpet] : null,
    sale ? ["Sale", sale] : null,
    usable ? ["Usable", usable] : null,
  ].filter((metric): metric is [string, string] => metric !== null);
  if (metrics.length === 0) return null;
  return (
    <dl>
      {metrics.map(([label, value]) => (
        <div key={label}>
          <dt>{label}</dt>
          <dd>{value}</dd>
        </div>
      ))}
    </dl>
  );
}

function FloorPlanCompareStrip({ columns }: { columns: CompareHomeColumn[] }) {
  const plans = columns.map((column) => ({
    column,
    plan: floorPlanForBhk([column.listing], column.listing.bhk),
  }));
  if (!plans.some((row) => row.plan !== null)) return null;
  return (
    <section className="compare-floor-plans" aria-label="Selected floor plans">
      <header><span>Floor plans</span></header>
      <div className={`compare-floor-plans__grid compare-floor-plans__grid--homes-${columns.length}`}>
        {plans.map(({ column, plan }) => (
          <figure key={column.key} className="compare-floor-plan">
            <div className="compare-floor-plan__image">
              {plan ? (
                <img
                  src={plan.previewUrl}
                  alt={`${column.name} ${plan.configurationType ?? column.configuration} floor plan`}
                />
              ) : <span aria-hidden="true">—</span>}
            </div>
            <figcaption>
              <strong title={column.name}>{column.name}</strong>
              {plan?.configurationType && <span>{plan.configurationType}</span>}
            </figcaption>
            {plan && <FloorPlanMetrics plan={plan} />}
          </figure>
        ))}
      </div>
    </section>
  );
}

function DifferenceRow({
  row,
  homes,
}: {
  row: CompareProjectionRow;
  homes: Map<string, PropertyCard>;
}) {
  return (
    <article className={`compare-difference compare-difference--${row.contrast}`}>
      <header><h3>{row.label}</h3></header>
      <div className="compare-difference__values">
        {row.cells.map((cell) => {
          const home = homes.get(cell.propertyId);
          const name = home?.society_name?.trim() || home?.title || "Home unavailable";
          return (
            <div key={cell.propertyId} className={`compare-difference__value compare-difference__value--${cell.state}`}>
              <span className="compare-difference__home">{name}</span>
              <strong>{formatCompareCell(cell)}</strong>
              {cell.detail && cell.detail !== String(cell.value ?? "") && <small>{cell.detail}</small>}
            </div>
          );
        })}
      </div>
    </article>
  );
}

function DifferencesFirst({
  rows,
  homes,
}: {
  rows: CompareProjectionRow[];
  homes: Map<string, PropertyCard>;
}) {
  if (rows.length === 0) {
    return (
      <section className="compare-differences compare-differences--aligned">
        <h2>No material differences yet</h2>
        <p>These homes align on the facts currently available.</p>
      </section>
    );
  }
  return (
    <section className="compare-differences" aria-labelledby="compare-differences-title">
      <header><h2 id="compare-differences-title">Material differences</h2></header>
      {rows.map((row) => <DifferenceRow key={row.id} row={row} homes={homes} />)}
    </section>
  );
}

export function SocietyComparisonMatrix({
  selectedHomes,
  details,
  onRemoveColumn,
}: {
  selectedHomes: PropertyCard[];
  details: PropertyDetailResponse[];
  onRemoveColumn?: (propertyIds: string[]) => void;
}) {
  const { notes } = useNotebook();
  const columns = useMemo(() => buildHomeColumns(selectedHomes), [selectedHomes]);
  const selectedIds = useMemo(() => selectedHomes.map((home) => home.id), [selectedHomes]);
  const selectedIdSet = useMemo(() => new Set(selectedIds), [selectedIds]);
  const projection = useMemo(() => buildCompareProjection(selectedIds, [
    ...selectedHomes.flatMap(propertyBaselineFacets),
    ...details.flatMap((detail) => decisionLabelFacets({
      propertyId: detail.property.id,
      societyId: detail.entity_refs.society_entity_id,
      labels: detail.decision_labels ?? [],
    })),
    ...details.flatMap((detail) => mapContextFacets(detail.property.id, detail.map_context)),
    ...notebookNoteFacets(notes.filter((note) => selectedIdSet.has(note.propertyId))),
  ]), [details, notes, selectedHomes, selectedIdSet, selectedIds]);
  const homesById = useMemo(
    () => new Map(selectedHomes.map((home) => [home.id, home])),
    [selectedHomes],
  );

  return (
    <section className="compare-editorial" aria-label="Side-by-side home comparison">
      <div className="compare-topics">
        <div className={`compare-topics__homes compare-topic-columns compare-topic-columns--homes-${columns.length}`}>
          {columns.map((column, index) => (
            <CompareHomeHeader
              key={column.key}
              column={column}
              index={index}
              compareIds={selectedIds}
              onRemove={onRemoveColumn}
            />
          ))}
        </div>

        <DifferencesFirst rows={projection.differences} homes={homesById} />

        <details className="compare-evidence-details">
          <summary>More evidence</summary>
          <FloorPlanCompareStrip columns={columns} />
          <section className="compare-full-evidence" aria-label="All compared facts">
            {projection.evidence.map((row) => (
              <DifferenceRow key={row.id} row={row} homes={homesById} />
            ))}
          </section>
        </details>
      </div>
    </section>
  );
}
