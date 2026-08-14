import type {
  DecisionFacet,
  DecisionSourceRef,
  DecisionState,
} from "./decisionFacets.ts";

export type CompareProjectionCell = Readonly<{
  propertyId: string;
  state: DecisionState;
  value?: string | number;
  unit?: string;
  detail?: string;
  receipts: DecisionSourceRef[];
}>;

export type CompareProjectionRow = Readonly<{
  id: string;
  group: string;
  topic: string;
  label: string;
  rank: number;
  contrast: "different" | "coverage_gap" | "conflicting" | "same";
  numericDelta?: number;
  cells: CompareProjectionCell[];
}>;

export type CompareProjection = Readonly<{
  differences: CompareProjectionRow[];
  evidence: CompareProjectionRow[];
}>;

const DEFAULT_ROW_RANK = 1_000;
const DEFAULT_DIFFERENCE_COUNT = 5;

function comparableValue(value: string | number | undefined): string | number | undefined {
  if (typeof value === "number") return Number.isFinite(value) ? value : undefined;
  const normalized = value?.trim().toLocaleLowerCase("en-IN");
  return normalized || undefined;
}

function baseFacetKey(facet: DecisionFacet): string | null {
  if (!facet.compare) return null;
  return `${facet.compare.group}::${facet.topic}`;
}

function recordFacetKey(facet: DecisionFacet): string {
  const recordId = facet.sourceRef?.recordId?.trim() || facet.id;
  return `${facet.origin}::${recordId}`;
}

function facetKey(facet: DecisionFacet): string | null {
  const baseKey = baseFacetKey(facet);
  if (!baseKey) return null;
  if (
    facet.origin === "map_fact"
    || facet.origin === "user_note"
    || facet.origin === "smart_block"
  ) {
    return recordFacetKey(facet);
  }
  return baseKey;
}

function resolvedFacet(facets: DecisionFacet[]): DecisionFacet | undefined {
  if (facets.length <= 1) return facets[0];
  const numeric = facets.filter((facet) => typeof facet.value === "number" && Number.isFinite(facet.value));
  if (numeric.length === facets.length) {
    return [...numeric].sort((left, right) => Number(left.value) - Number(right.value))[0];
  }
  return [...facets].sort((left, right) =>
    (left.compare?.rank ?? DEFAULT_ROW_RANK) - (right.compare?.rank ?? DEFAULT_ROW_RANK)
    || left.id.localeCompare(right.id)
  )[0];
}

function cellFor(propertyId: string, facets: DecisionFacet[]): CompareProjectionCell {
  const facet = resolvedFacet(facets);
  if (!facet) {
    return { propertyId, state: "not_evaluated", receipts: [] };
  }
  return {
    propertyId,
    state: facet.state,
    value: facet.value,
    unit: facet.unit,
    detail: facet.detail,
    receipts: facet.sourceRef ? [facet.sourceRef] : [],
  };
}

function rowContrast(cells: CompareProjectionCell[]): Pick<CompareProjectionRow, "contrast" | "numericDelta"> {
  const signatures = cells.map((cell) => `${cell.state}:${String(comparableValue(cell.value) ?? "")}`);
  if (signatures.every((signature) => signature === signatures[0])) {
    return { contrast: "same" };
  }
  if (cells.some((cell) => cell.state === "conflicting")) {
    return { contrast: "conflicting" };
  }
  if (cells.some((cell) => cell.state !== "known")) {
    return { contrast: "coverage_gap" };
  }
  const values = cells.map((cell) => comparableValue(cell.value));
  if (values.some((value) => value == null)) return { contrast: "coverage_gap" };
  if (values.every((value) => typeof value === "number")) {
    const numeric = values as number[];
    return {
      contrast: "different",
      numericDelta: Math.max(...numeric) - Math.min(...numeric),
    };
  }
  return { contrast: "different" };
}

function rowPriority(row: CompareProjectionRow): number {
  const statePriority = row.contrast === "conflicting"
    ? -200
    : row.contrast === "coverage_gap"
      ? -100
      : 0;
  return statePriority + row.rank;
}

export function buildCompareProjection(
  propertyIds: readonly string[],
  facets: readonly DecisionFacet[],
  differenceCount = DEFAULT_DIFFERENCE_COUNT,
): CompareProjection {
  const selectedIds = [...new Set(propertyIds.filter(Boolean))];
  if (selectedIds.length === 0) return { differences: [], evidence: [] };

  const byRow = new Map<string, DecisionFacet[]>();
  for (const facet of facets) {
    if (!facet.propertyId || !selectedIds.includes(facet.propertyId)) continue;
    const key = facetKey(facet);
    if (!key) continue;
    const current = byRow.get(key) ?? [];
    current.push(facet);
    byRow.set(key, current);
  }

  const evidence = [...byRow.entries()].map(([id, rowFacets]): CompareProjectionRow => {
    const representative = [...rowFacets].sort((left, right) =>
      (left.compare?.rank ?? DEFAULT_ROW_RANK) - (right.compare?.rank ?? DEFAULT_ROW_RANK)
      || left.id.localeCompare(right.id)
    )[0];
    const cells = selectedIds.map((propertyId) => cellFor(
      propertyId,
      rowFacets.filter((facet) => facet.propertyId === propertyId),
    ));
    return {
      id,
      group: representative.compare?.group ?? "reference",
      topic: representative.topic,
      label: representative.label,
      rank: representative.compare?.rank ?? DEFAULT_ROW_RANK,
      ...rowContrast(cells),
      cells,
    };
  }).sort((left, right) =>
    left.rank - right.rank
    || left.label.localeCompare(right.label, "en-IN")
    || left.id.localeCompare(right.id)
  );

  const differences = evidence
    .filter((row) => row.contrast !== "same")
    .sort((left, right) =>
      rowPriority(left) - rowPriority(right)
      || left.label.localeCompare(right.label, "en-IN")
    )
    .slice(0, Math.max(0, differenceCount));

  return { differences, evidence };
}

export function formatCompareCell(cell: CompareProjectionCell): string {
  if (cell.state === "conflicting") return "Conflicting";
  if (cell.state === "unknown") return "Unknown";
  if (cell.state === "not_evaluated") return "Not evaluated";
  if (cell.value == null || cell.value === "") return "Unknown";
  if (typeof cell.value === "string") return cell.value;
  if (cell.unit === "INR") {
    if (cell.value >= 10_000_000) return `₹${Number((cell.value / 10_000_000).toFixed(2))} Cr`;
    if (cell.value >= 100_000) return `₹${Number((cell.value / 100_000).toFixed(1))} L`;
    return `₹${Math.round(cell.value).toLocaleString("en-IN")}`;
  }
  if (cell.unit === "INR_PER_SQFT") return `₹${Math.round(cell.value).toLocaleString("en-IN")}/sqft`;
  if (cell.unit === "SQFT") return `${Math.round(cell.value).toLocaleString("en-IN")} sqft`;
  if (cell.unit === "BHK") return `${cell.value} BHK`;
  if (cell.unit === "KM") return `${Number(cell.value.toFixed(1))} km`;
  return cell.value.toLocaleString("en-IN");
}
