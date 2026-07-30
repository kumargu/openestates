import { useMemo } from "react";
import { Link, useSearchParams } from "react-router-dom";
import { useNotebook } from "../../hooks/useNotebook.ts";
import { floorPlanForBhk, type FloorPlanComparePlan } from "../../lib/floor-plan-compare.ts";
import {
  labelDef,
  labelsForNearbyPlace,
  type NotebookLabelId,
} from "../../lib/notebook.ts";
import { LabelVisualIcon } from "../../lib/LabelVisualIcon.tsx";
import {
  labelBaseId,
  labelClassToken,
  labelDistanceLimitKm,
} from "../../lib/labelVisuals.ts";
import type { MapPlacePin, PropertyCard, PropertyDetailResponse, PropertyMapContext } from "../../lib/types.ts";

type SocietyColumn = {
  key: string;
  name: string;
  area: string;
  propertyId: string;
  selectedIds: Set<string>;
  listings: PropertyCard[];
};

type CanonicalRowId =
  | "projectScale"
  | "homeState";

type CanonicalRow = {
  id: CanonicalRowId;
  label: string;
  scope: "bhk" | "society";
  value: (listings: PropertyCard[]) => string | null;
};

type NoteRow = {
  id: NoteGroupId;
  label: string;
  icon: string;
  section: "Access" | "Risks" | "Money" | "Reference";
  rank: number;
};

type CompareItemOrigin = "backend";

type CompareItem = {
  id: string;
  title: string;
  detail?: string;
  source?: string;
  catalogKey?: string;
  labels: NotebookLabelId[];
  origin: CompareItemOrigin;
};

type CompareEvidence = {
  id: string;
  title: string;
  detail?: string;
  origin: CompareItemOrigin;
  labels: NotebookLabelId[];
  primaryLabel: NotebookLabelId;
  group: NoteGroupId;
};

type CompareEvidenceCluster = {
  label: NotebookLabelId;
  items: CompareEvidence[];
};

type NoteGroupId =
  | "nearby_access"
  | "access_notes"
  | "commute_anchors"
  | "open_spaces"
  | "red_flags"
  | "water"
  | "approach"
  | "money"
  | "layout"
  | "reference";

type NoteGroupDef = {
  id: NoteGroupId;
  label: string;
  icon: string;
  section: NoteRow["section"];
  rank: number;
};

const NOTE_GROUPS: NoteGroupDef[] = [
  { id: "nearby_access", label: "Nearby access", icon: "⌖", section: "Access", rank: 10 },
  { id: "access_notes", label: "Daily access", icon: "⌁", section: "Access", rank: 40 },
  { id: "commute_anchors", label: "Commute anchors", icon: "↔", section: "Access", rank: 50 },
  { id: "open_spaces", label: "Open spaces", icon: "⌑", section: "Access", rank: 60 },
  { id: "red_flags", label: "Red flags", icon: "!", section: "Risks", rank: 10 },
  { id: "water", label: "Water and flood", icon: "~", section: "Risks", rank: 20 },
  { id: "approach", label: "Approach", icon: "→", section: "Risks", rank: 30 },
  { id: "money", label: "Money", icon: "₹", section: "Money", rank: 10 },
  { id: "layout", label: "Plan and layout", icon: "□", section: "Reference", rank: 10 },
  { id: "reference", label: "Other", icon: "·", section: "Reference", rank: 99 },
];

const NOTE_GROUP_BY_ID = new Map(NOTE_GROUPS.map((group) => [group.id, group]));
const NOTE_SECTION_ORDER: Record<NoteRow["section"], number> = {
  Access: 1,
  Risks: 2,
  Money: 3,
  Reference: 4,
};

function isNoteGroupId(value: string | undefined): value is NoteGroupId {
  return value != null && NOTE_GROUP_BY_ID.has(value as NoteGroupId);
}

function compareNoteGroupPriority(left: NoteGroupId, right: NoteGroupId): number {
  const leftGroup = NOTE_GROUP_BY_ID.get(left);
  const rightGroup = NOTE_GROUP_BY_ID.get(right);
  if (!leftGroup || !rightGroup) return left.localeCompare(right);
  return NOTE_SECTION_ORDER[leftGroup.section] - NOTE_SECTION_ORDER[rightGroup.section]
    || leftGroup.rank - rightGroup.rank
    || leftGroup.label.localeCompare(rightGroup.label);
}

function societyKey(property: PropertyCard): string {
  return property.society_name?.trim().toLocaleLowerCase()
    || property.title.trim().toLocaleLowerCase();
}

function buildSocietyColumns(
  selectedHomes: PropertyCard[],
  catalog: PropertyCard[],
): SocietyColumn[] {
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
      propertyId: representative.id,
      selectedIds: new Set(selected.map((home) => home.id)),
      listings,
    };
  });
}

function formatPrice(price: number): string {
  if (price >= 10_000_000) {
    return `₹${(price / 10_000_000).toFixed(2).replace(/0+$/, "").replace(/\.$/, "")} Cr`;
  }
  if (price >= 100_000) {
    return `₹${(price / 100_000).toFixed(1).replace(/\.0$/, "")} L`;
  }
  return `₹${Math.round(price).toLocaleString("en-IN")}`;
}

function numericRange(
  listings: PropertyCard[],
  read: (listing: PropertyCard) => number | null,
  format: (value: number) => string,
): string | null {
  const values = listings
    .map(read)
    .filter((value): value is number => value != null && value > 0);
  if (values.length === 0) return null;
  const low = Math.min(...values);
  const high = Math.max(...values);
  return low === high ? format(low) : `${format(low)}–${format(high)}`;
}

function usableSqft(property: PropertyCard): number | null {
  return property.plan_carpet_area_sqft
    ?? property.carpet_area_sqft
    ?? property.plan_sale_area_sqft
    ?? property.super_builtup_sqft
    ?? property.sqft
    ?? null;
}

function mostCommon(values: string[]): string | null {
  if (values.length === 0) return null;
  const counts = new Map<string, number>();
  for (const value of values) counts.set(value, (counts.get(value) ?? 0) + 1);
  return [...counts.entries()].sort((left, right) => right[1] - left[1])[0]?.[0] ?? null;
}

function homeState(listings: PropertyCard[]): string | null {
  const raw = mostCommon(
    listings
      .map((listing) =>
        listing.home_state_display
        || listing.project_status_display
        || listing.possession_status
      )
      .filter(Boolean),
  );
  if (!raw) return null;
  const normalized = raw.toLowerCase().replace(/[_-]+/g, " ");
  if (normalized.includes("delivered") || normalized.includes("ready")) return "Delivered";
  if (normalized.includes("delay")) return "Delayed";
  if (normalized.includes("construction")) return "Under construction";
  return raw;
}

function projectScale(listings: PropertyCard[]): string | null {
  const land = numericRange(
    listings,
    (listing) => listing.society_land_acres ?? null,
    (value) => `${value.toFixed(1).replace(/\.0$/, "")} acres`,
  );
  const density = numericRange(
    listings,
    (listing) => listing.units_per_acre ?? null,
    (value) => `${Math.round(value)} homes / acre`,
  );
  const openSpace = numericRange(
    listings,
    (listing) => listing.open_space_pct ?? null,
    (value) => `${value.toFixed(1).replace(/\.0$/, "")}% open`,
  );
  const parts = [land, density, openSpace].filter(Boolean);
  return parts.length > 0 ? parts.join(" · ") : null;
}

function homeHeaderSummary(listings: PropertyCard[]): string[] {
  const price = numericRange(listings, (listing) => listing.price, formatPrice);
  const sqft = numericRange(
    listings,
    usableSqft,
    (value) => `${Math.round(value).toLocaleString("en-IN")} sqft`,
  );
  const pricePerSqft = numericRange(
    listings,
    (listing) => listing.price_per_sqft,
    (value) => `₹${Math.round(value).toLocaleString("en-IN")}/sqft`,
  );
  return [price, sqft, pricePerSqft].filter((item): item is string => item != null);
}

function formatSqft(value: number | undefined): string | null {
  if (typeof value !== "number" || !Number.isFinite(value) || value <= 0) return null;
  return `${Math.round(value).toLocaleString("en-IN")} sqft`;
}

function formatUsableRatio(value: number | undefined): string | null {
  if (typeof value !== "number" || !Number.isFinite(value) || value <= 0) return null;
  return `${Math.round(value * 100)}%`;
}

const CANONICAL_ROWS: CanonicalRow[] = [
  {
    id: "projectScale",
    label: "Project scale",
    scope: "society",
    value: projectScale,
  },
  {
    id: "homeState",
    label: "Home state",
    scope: "society",
    value: homeState,
  },
];

function noteCompareGroup(note: Pick<CompareItem, "labels">): NoteGroupId | null {
  const groups: NoteGroupId[] = [];
  for (const label of note.labels) {
    const group = labelDef(label).compareGroup;
    if (isNoteGroupId(group)) groups.push(group);
  }
  if (groups.length > 0) return [...new Set(groups)].sort(compareNoteGroupPriority)[0];
  return note.labels.length > 0 ? "reference" : null;
}

function detailForColumn(
  column: SocietyColumn,
  detailById: Map<string, PropertyDetailResponse>,
): PropertyDetailResponse | undefined {
  for (const selectedId of column.selectedIds) {
    const detail = detailById.get(selectedId);
    if (detail) return detail;
  }
  return column.listings
    .map((listing) => detailById.get(listing.id))
    .find((detail): detail is PropertyDetailResponse => Boolean(detail));
}

function compareContextForColumn(
  column: SocietyColumn,
  detailById: Map<string, PropertyDetailResponse>,
): PropertyMapContext | null {
  return detailForColumn(column, detailById)?.map_context ?? null;
}

function labelPillClass(label: NotebookLabelId, extra = ""): string {
  return `notion-pill notion-pill--${labelClassToken(label)} compare-label-pill${extra ? ` ${extra}` : ""}`;
}

function displayIconLabel(item: Pick<CompareItem, "labels">): NotebookLabelId | null {
  const bucket = item.labels.find((label) => labelDistanceLimitKm(label) != null);
  if (bucket) return labelBaseId(bucket);
  const primary = item.labels.find((label) => label !== "commute") ?? item.labels[0];
  return primary ?? null;
}

function primaryCompareLabel(item: Pick<CompareItem, "labels">): NotebookLabelId | null {
  const bucket = item.labels.find((label) => labelDistanceLimitKm(label) != null);
  return bucket ? labelBaseId(bucket) : displayIconLabel(item);
}

function itemDistanceKm(item: Pick<CompareItem, "detail">): number {
  const parsed = Number(item.detail?.replace(/[^0-9.]/g, ""));
  return Number.isFinite(parsed) ? parsed : Number.POSITIVE_INFINITY;
}

function compareEvidenceDistance(left: Pick<CompareEvidence | CompareItem, "labels" | "detail">, right: Pick<CompareEvidence | CompareItem, "labels" | "detail">): number {
  const leftDistanceLimit = Math.min(
    ...left.labels.map((label) => labelDistanceLimitKm(label) ?? Number.POSITIVE_INFINITY),
  );
  const rightDistanceLimit = Math.min(
    ...right.labels.map((label) => labelDistanceLimitKm(label) ?? Number.POSITIVE_INFINITY),
  );
  return leftDistanceLimit - rightDistanceLimit
    || itemDistanceKm(left) - itemDistanceKm(right)
}

function compareEvidence(left: CompareEvidence, right: CompareEvidence): number {
  return compareEvidenceDistance(left, right)
    || left.title.localeCompare(right.title);
}

function normalizeCompareEvidence(item: CompareItem): CompareEvidence | null {
  const group = noteCompareGroup(item);
  const primaryLabel = primaryCompareLabel(item);
  if (!group || !primaryLabel) return null;
  return {
    id: item.id,
    title: item.title,
    detail: item.detail || item.source,
    origin: item.origin,
    labels: item.labels,
    primaryLabel,
    group,
  };
}

function normalizeCompareEvidences(items: CompareItem[], hiddenLabels: Set<NotebookLabelId>): CompareEvidence[] {
  const byKey = new Map<string, CompareEvidence>();
  for (const item of items) {
    const evidence = normalizeCompareEvidence(item);
    if (!evidence || hiddenLabels.has(evidence.primaryLabel)) continue;
    const key = `${evidence.group}::${evidence.primaryLabel}::${evidence.title.toLocaleLowerCase("en-IN")}`;
    const existing = byKey.get(key);
    if (!existing) {
      byKey.set(key, evidence);
    }
  }
  return [...byKey.values()].sort(compareEvidence);
}

function compareEvidenceClusters(items: CompareEvidence[]): CompareEvidenceCluster[] {
  const grouped = new Map<NotebookLabelId, CompareEvidence[]>();
  for (const item of [...items].sort(compareEvidence)) {
    const label = item.primaryLabel;
    grouped.set(label, [...(grouped.get(label) ?? []), item]);
  }
  return [...grouped.entries()]
    .sort((left, right) =>
      compareEvidence(left[1][0], right[1][0])
      || labelDef(left[0]).title.localeCompare(labelDef(right[0]).title)
    )
    .map(([label, groupItems]) => ({ label, items: groupItems }));
}

function CanonicalRowIcon({ id }: { id: CanonicalRowId }) {
  if (id === "projectScale") {
    return (
      <svg viewBox="0 0 24 24" width="15" height="15" aria-hidden="true">
        <path d="M4.5 18.5h15M6.5 16V8.5h3V16M10.5 16V5.5h3V16M14.5 16v-5.5h3V16" />
      </svg>
    );
  }
  return (
    <svg viewBox="0 0 24 24" width="15" height="15" aria-hidden="true">
      <path d="M12 7v5l3 2" />
      <path d="M20 12a8 8 0 1 1-2.35-5.65" />
      <path d="M18.5 4.5v3.8h-3.8" />
    </svg>
  );
}

function statusClassName(value: string): string {
  const normalized = value.toLocaleLowerCase("en-IN");
  if (normalized.includes("delivered") || normalized.includes("ready")) {
    return "compare-property-status compare-property-status--good";
  }
  if (normalized.includes("delay")) {
    return "compare-property-status compare-property-status--risk";
  }
  return "compare-property-status compare-property-status--neutral";
}

function CanonicalValue({ row, value }: { row: CanonicalRow; value: string }) {
  if (row.id === "projectScale") {
    const parts = value.split(" · ").filter(Boolean);
    return (
      <div className="compare-property-pills">
        {parts.map((part) => (
          <span key={part}>{part}</span>
        ))}
      </div>
    );
  }

  return <strong className={statusClassName(value)}>{value}</strong>;
}

function CompareHomeHeader({
  column,
  index,
  summary = [],
  onRemove,
}: {
  column: SocietyColumn;
  index: number;
  summary?: string[];
  onRemove?: (propertyIds: string[]) => void;
}) {
  return (
    <article className="compare-editorial__home">
      {onRemove && (
        <button
          type="button"
          className="compare-editorial__remove"
          aria-label={`Remove ${column.name} from compare`}
          onClick={() => onRemove([...column.selectedIds])}
        >
          Remove
        </button>
      )}
      <Link to={`/property/${encodeURIComponent(column.propertyId)}`}>
        <i aria-hidden="true">{String(index + 1).padStart(2, "0")}</i>
        <strong>{column.name}</strong>
        <span>{column.area}</span>
        {summary.length > 0 && (
          <small>{summary.join(" · ")}</small>
        )}
        <em>Open home ↗</em>
      </Link>
    </article>
  );
}

function distanceLabel(place: MapPlacePin): string | null {
  if (typeof place.distance_km !== "number" || !Number.isFinite(place.distance_km)) {
    return null;
  }
  return `${place.distance_km.toFixed(1).replace(/\.0$/, "")} km`;
}

function nearbyPlaceCompareItem(place: MapPlacePin, index: number): CompareItem | null {
  const labels = labelsForNearbyPlace(place.layer, place.distance_km);
  if (!noteCompareGroup({ labels })) return null;
  const distance = distanceLabel(place);
  return {
    id: `${place.feature_id ?? place.place_entity_id ?? place.layer}-${place.name}-${index}`,
    title: place.name,
    detail: distance ?? undefined,
    source: place.source_type,
    labels,
    origin: "backend",
  };
}

function backendCompareItems(context: PropertyMapContext | null): CompareItem[] {
  if (!context) return [];
  return context.places
    .map(nearbyPlaceCompareItem)
    .filter((item): item is CompareItem => item !== null)
    .sort((left, right) =>
      compareEvidenceDistance(left, right)
      || left.title.localeCompare(right.title)
    );
}

function CompactCompareItem({
  item,
}: {
  item: CompareEvidence;
}) {
  return (
    <span className="compare-compact-item">
      <strong>{item.title}</strong>
      {item.detail && <small>{item.detail}</small>}
    </span>
  );
}

function GroupedCompareCell({
  evidences,
  onHideLabel,
}: {
  evidences: CompareEvidence[];
  onHideLabel: (label: NotebookLabelId) => void;
}) {
  if (evidences.length === 0) {
    return <div className="compare-theme__cell is-empty" />;
  }

  const clusters = compareEvidenceClusters(evidences);
  return (
    <div className="compare-theme__cell compare-theme__cell--grouped">
      {clusters.map((cluster) => (
        <div key={cluster.label} className="compare-item-cluster">
          <div className="compare-item-cluster__head">
            <span className={labelPillClass(cluster.label, "compare-item-cluster__label compare-label-pill--readonly")}>
              <LabelVisualIcon id={cluster.label} size={18} />
              {labelDef(cluster.label).title}
            </span>
            <button
              type="button"
              className="compare-label-hide"
              aria-label={`Hide ${labelDef(cluster.label).title} from compare`}
              onClick={() => onHideLabel(cluster.label)}
            >
              Hide
            </button>
          </div>
          <div className="compare-compact-items">
            {cluster.items.map((item) => (
              <CompactCompareItem key={item.id} item={item} />
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}

function HiddenCompareLabels({
  labels,
  onShow,
}: {
  labels: NotebookLabelId[];
  onShow: (label: NotebookLabelId) => void;
}) {
  if (labels.length === 0) return null;
  return (
    <div className="compare-hidden-labels" aria-label="Hidden compare labels">
      <span>Hidden</span>
      {labels.map((label) => (
        <button
          key={label}
          type="button"
          className={labelPillClass(label, "compare-label-pill--button")}
          onClick={() => onShow(label)}
        >
          <LabelVisualIcon id={label} size={18} />
          {labelDef(label).title}
          <small>Restore</small>
        </button>
      ))}
    </div>
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

function SocietyFactCard({
  rows,
  listings,
}: {
  rows: CanonicalRow[];
  listings: PropertyCard[];
}) {
  const visible = rows
    .map((row) => ({
      row,
      value: row.value(listings),
    }))
    .filter((item) => item.value);

  if (visible.length === 0) {
    return <article className="compare-fact-card is-empty" aria-hidden="true" />;
  }

  return (
    <article className="compare-fact-card">
      {visible.map((item) => (
        <div key={item.row.id} className="compare-fact-card__row">
          <span className="compare-fact-card__icon">
            <CanonicalRowIcon id={item.row.id} />
          </span>
          <span className="compare-fact-card__label">{item.row.label}</span>
          {item.value && <CanonicalValue row={item.row} value={item.value} />}
        </div>
      ))}
    </article>
  );
}

function FloorPlanCompareStrip({
  columns,
  activeBhk,
}: {
  columns: SocietyColumn[];
  activeBhk: number;
}) {
  const planRows = columns.map((column) => ({
    column,
    plan: floorPlanForBhk(column.listings, activeBhk),
  }));
  if (!planRows.some((row) => row.plan !== null)) return null;

  return (
    <section className="compare-floor-plans" aria-label={`${activeBhk} BHK floor plans`}>
      <header>
        <span>{activeBhk} BHK plans</span>
      </header>
      <div className={`compare-floor-plans__grid compare-floor-plans__grid--homes-${columns.length}`}>
        {planRows.map((row) => (
          <figure key={row.column.key} className="compare-floor-plan">
            <div className="compare-floor-plan__image">
              {row.plan ? (
                <img
                  src={row.plan.previewUrl}
                  alt={`${row.column.name} ${row.plan.configurationType ?? `${activeBhk} BHK`} floor plan`}
                />
              ) : (
                <span aria-hidden="true">—</span>
              )}
            </div>
            <figcaption>
              <strong title={row.column.name}>{row.column.name}</strong>
              {row.plan?.configurationType && <span>{row.plan.configurationType}</span>}
            </figcaption>
            {row.plan && <FloorPlanMetrics plan={row.plan} />}
          </figure>
        ))}
      </div>
    </section>
  );
}

export function SocietyComparisonMatrix({
  selectedHomes,
  catalog,
  details,
  onRemoveColumn,
}: {
  selectedHomes: PropertyCard[];
  catalog: PropertyCard[];
  details: PropertyDetailResponse[];
  onRemoveColumn?: (propertyIds: string[]) => void;
}) {
  const [searchParams, setSearchParams] = useSearchParams();
  const {
    hiddenCompareLabels,
    hideCompareLabel,
    showCompareLabel,
  } = useNotebook();
  const columns = useMemo(
    () => buildSocietyColumns(selectedHomes, catalog),
    [catalog, selectedHomes],
  );
  const detailById = useMemo(
    () => new Map(details.map((detail) => [detail.property.id, detail])),
    [details],
  );
  const availableBhks = [...new Set(columns.flatMap((column) =>
    column.listings.map((listing) => listing.bhk)
  ))].sort((left, right) => left - right);
  const requestedBhk = Number(searchParams.get("bhk"));
  const preferredBhk = selectedHomes[0]?.bhk;
  const activeBhk = availableBhks.includes(requestedBhk)
    ? requestedBhk
    : preferredBhk != null && availableBhks.includes(preferredBhk)
      ? preferredBhk
      : availableBhks[0] ?? 0;

  const columnItems = useMemo(
    () => new Map(columns.map((column) => {
      const context = compareContextForColumn(column, detailById);
      return [column.key, backendCompareItems(context)];
    })),
    [columns, detailById],
  );
  const hiddenLabelSet = useMemo(
    () => new Set(hiddenCompareLabels),
    [hiddenCompareLabels],
  );
  const columnEvidences = useMemo(
    () => new Map(columns.map((column) => [
      column.key,
      normalizeCompareEvidences(columnItems.get(column.key) ?? [], hiddenLabelSet),
    ])),
    [columnItems, columns, hiddenLabelSet],
  );

  const noteRows = useMemo(() => {
    const groups = new Set<NoteGroupId>();
    for (const column of columns) {
      for (const evidence of columnEvidences.get(column.key) ?? []) {
        groups.add(evidence.group);
      }
    }
    return [...groups]
      .map((id): NoteRow => {
        const group = NOTE_GROUP_BY_ID.get(id) ?? NOTE_GROUP_BY_ID.get("reference");
        return {
          id,
          label: group?.label ?? id,
          icon: group?.icon ?? "·",
          section: group?.section ?? "Reference",
          rank: group?.rank ?? 99,
        };
      })
      .sort((left, right) =>
        left.section.localeCompare(right.section)
        || left.rank - right.rank
        || left.label.localeCompare(right.label)
      );
  }, [columnEvidences, columns]);

  function setBhk(bhk: number) {
    const next = new URLSearchParams(searchParams);
    next.set("bhk", String(bhk));
    setSearchParams(next, { replace: true });
  }

  const canonicalSections = [
    { title: "Society", rows: CANONICAL_ROWS.filter((row) => row.scope === "society") },
  ];
  const noteSections = ["Access", "Risks", "Money", "Reference"] as const;

  return (
    <section className="compare-editorial" aria-label="Side-by-side home comparison">
      <header className="compare-editorial__controls">
        <div className="compare-editorial__bhk" role="group" aria-label="Filter by BHK">
          {availableBhks.map((bhk) => (
            <button
              key={bhk}
              type="button"
              className={bhk === activeBhk ? "is-active" : ""}
              aria-pressed={bhk === activeBhk}
              onClick={() => setBhk(bhk)}
            >
              {bhk} BHK
            </button>
          ))}
        </div>
      </header>
      <HiddenCompareLabels labels={hiddenCompareLabels} onShow={showCompareLabel} />

      <FloorPlanCompareStrip columns={columns} activeBhk={activeBhk} />

      <div className="compare-topics">
        <div className={`compare-topics__homes compare-topic-columns compare-topic-columns--homes-${columns.length}`}>
          {columns.map((column, index) => (
            <CompareHomeHeader
              key={column.key}
              column={column}
              index={index}
              summary={homeHeaderSummary(column.listings.filter((listing) => listing.bhk === activeBhk))}
              onRemove={onRemoveColumn}
            />
          ))}
        </div>

        {canonicalSections.map((section) => (
          <section key={section.title} className="compare-topics__group compare-topics__group--facts">
            <h2>{section.title}</h2>
            <div className={`compare-topic-columns compare-topic-columns--homes-${columns.length}`}>
              {columns.map((column) => (
                <SocietyFactCard
                  key={column.key}
                  rows={section.rows}
                  listings={column.listings}
                />
              ))}
            </div>
          </section>
        ))}

        {noteSections.map((section) => {
          const rows = noteRows.filter((row) => row.section === section);
          if (rows.length === 0) return null;
          return (
            <section key={section} className="compare-topics__group">
              <h2>{section}</h2>
              {rows.map((row) => (
                <article key={row.id} className="compare-theme">
                  <header className="compare-theme__head">
                    <span className="compare-topics__label-icon" aria-hidden="true">
                      {row.icon}
                    </span>
                    <strong>{row.label}</strong>
                  </header>
                  <div className={`compare-topic-columns compare-topic-columns--homes-${columns.length}`}>
                    {columns.map((column) => {
                      const matching = (columnEvidences.get(column.key) ?? []).filter(
                        (evidence) => evidence.group === row.id,
                      );
                      return (
                        <GroupedCompareCell
                          key={column.key}
                          evidences={matching}
                          onHideLabel={hideCompareLabel}
                        />
                      );
                    })}
                  </div>
                </article>
              ))}
            </section>
          );
        })}
      </div>
    </section>
  );
}
