import {
  labelDef,
  type NotebookNote,
} from "./notebook.ts";
import type {
  ConstructionProfile,
  PlanAssumptions,
} from "../features/home-plan/model.ts";
import type {
  DecisionLabel,
  MapPlacePin,
  PropertyCard,
  PropertyMapContext,
} from "./types.ts";

export type DecisionScope =
  | "property"
  | "society"
  | "project"
  | "builder"
  | "area"
  | "buyer"
  | (string & {});

export type DecisionOrigin =
  | "canonical_fact"
  | "map_fact"
  | "user_note"
  | "smart_block"
  | "financial_plan";

export type DecisionState =
  | "known"
  | "unknown"
  | "conflicting"
  | "not_evaluated";

export type DecisionSourceRef = Readonly<{
  surface: string;
  recordId?: string;
  url?: string;
}>;

export type DecisionCompareRef = Readonly<{
  group: string;
  rank?: number;
}>;

export type DecisionFacet = Readonly<{
  id: string;
  propertyId?: string;
  societyId?: string;
  scope: DecisionScope;
  topic: string;
  origin: DecisionOrigin;
  label: string;
  value?: string | number;
  detail?: string;
  state: DecisionState;
  sourceRef?: DecisionSourceRef;
  compare?: DecisionCompareRef;
  confidence?: number;
}>;

export type SavedFinancialPlan = Readonly<{
  id: string;
  propertyId: string;
  modelVersion: string;
  shared: Readonly<{
    propertyPrice: number;
  }>;
  monthlyPath: Readonly<{
    monthlyEmi: number;
    currentRent: number;
    monthlySip: number;
    loanRate: number;
    sipReturn: number;
    extraEmisPerYear: number;
    holdingPeriodYears: number;
    inspectedYear: number;
    purchaseYear: number;
    constructionProfile: ConstructionProfile;
    planAssumptions: PlanAssumptions;
  }>;
  outputs: Readonly<{
    loanFreeYear: number | null;
    breakEvenYear: number | null;
    buyNetWorthAtInspectedYear: number;
    rentNetWorthAtInspectedYear: number;
    totalInterest: number | null;
    loanAmount: number;
  }>;
  updatedAt: number;
}>;

type FacetDraft = Omit<DecisionFacet, "state"> & {
  state?: DecisionState;
};

function compactIdPart(value: string): string {
  return value
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    || "unknown";
}

function stableHash(value: string): string {
  let hash = 0x811c9dc5;
  for (const char of value) {
    hash ^= char.charCodeAt(0);
    hash = Math.imul(hash, 0x01000193);
  }
  return (hash >>> 0).toString(36);
}

function stableIdPart(value: string): string {
  return `${compactIdPart(value)}-${stableHash(value)}`;
}

function knownValue(value: string | number | undefined | null): string | number | undefined {
  if (typeof value === "number") return Number.isFinite(value) ? value : undefined;
  const normalized = value?.trim();
  return normalized ? normalized : undefined;
}

function finiteNumber(value: number, field: string): number {
  if (!Number.isFinite(value)) {
    throw new RangeError(`${field} must be a finite number`);
  }
  return value;
}

function nonNegativeNumber(value: number, field: string): number {
  const checked = finiteNumber(value, field);
  if (checked < 0) {
    throw new RangeError(`${field} must be a finite number >= 0`);
  }
  return checked;
}

function assertApproxEqual(actual: number, expected: number, field: string): void {
  if (Math.abs(actual - expected) > 0.5) {
    throw new RangeError(`${field} must equal ${expected}`);
  }
}

function firstKnown(values: readonly (string | undefined | null)[]): string | undefined {
  for (const value of values) {
    const known = knownValue(value);
    if (typeof known === "string") return known;
  }
  return undefined;
}

function handleSource(values: readonly (string | number | undefined | null)[]): string {
  const raw = values
    .map((value) => value == null ? null : String(value).trim())
    .filter((value): value is string => Boolean(value))
    .join("|");
  return stableIdPart(raw || "unknown");
}

function facet(input: FacetDraft): DecisionFacet {
  const value = knownValue(input.value);
  return {
    ...input,
    value,
    state: input.state ?? (value == null && !input.detail ? "unknown" : "known"),
  };
}

function moneyFacet(
  propertyId: string,
  planId: string,
  topic: string,
  label: string,
  value: number,
  rank: number,
  detail?: string,
): DecisionFacet {
  const checkedValue = finiteNumber(value, topic);
  return facet({
    id: financialFacetId(planId, topic),
    propertyId,
    scope: "property",
    topic,
    origin: "financial_plan",
    label,
    value: checkedValue,
    detail,
    sourceRef: { surface: "plan", recordId: planId },
    compare: { group: "financial_plan", rank },
  });
}

function scopeFromDecisionLabel(label: DecisionLabel): DecisionScope {
  return label.scope || "society";
}

function decisionLabelHandle(label: DecisionLabel): string {
  if (label.sourceFactKeys?.length) {
    return handleSource(label.sourceFactKeys);
  }
  return handleSource([
    label.visualId,
    label.groupId,
    label.placement,
    label.priority,
    label.scope,
    label.severity,
    label.compareGroup,
    label.surfaces?.join("+"),
    label.value,
    label.valueText,
  ]);
}

export function decisionLabelFacets(input: {
  propertyId: string;
  societyId?: string;
  labels: readonly DecisionLabel[];
}): DecisionFacet[] {
  return input.labels.map((label) => facet({
    id: [
      "canonical",
      input.propertyId,
      compactIdPart(label.key),
      decisionLabelHandle(label),
    ].join(":"),
    propertyId: input.propertyId,
    societyId: input.societyId,
    scope: scopeFromDecisionLabel(label),
    topic: label.key,
    origin: "canonical_fact",
    label: label.label,
    value: label.value ?? label.valueText,
    detail: label.valueText && label.value != null ? label.valueText : undefined,
    confidence: label.confidence,
    sourceRef: {
      surface: "property",
      recordId: label.sourceFactKeys?.[0] ?? label.key,
    },
    compare: label.compareGroup
      ? { group: label.compareGroup, rank: label.priority }
      : undefined,
  }));
}

export function propertyBaselineFacets(property: PropertyCard): DecisionFacet[] {
  const societyId = property.kg_entity_refs.society_entity_id;
  const rows: FacetDraft[] = [
    {
      id: `canonical:${property.id}:price`,
      propertyId: property.id,
      societyId,
      scope: "property",
      topic: "price",
      origin: "canonical_fact",
      label: "Property price",
      value: property.price,
      sourceRef: { surface: "property", recordId: property.id },
      compare: { group: "baseline", rank: 10 },
    },
    {
      id: `canonical:${property.id}:price-per-sqft`,
      propertyId: property.id,
      societyId,
      scope: "property",
      topic: "price_per_sqft",
      origin: "canonical_fact",
      label: "Price per sqft",
      value: property.price_per_sqft,
      sourceRef: { surface: "property", recordId: property.id },
      compare: { group: "baseline", rank: 20 },
    },
    {
      id: `canonical:${property.id}:home-state`,
      propertyId: property.id,
      societyId,
      scope: "society",
      topic: "home_state",
      origin: "canonical_fact",
      label: "Home state",
      value: firstKnown([
        property.home_state_display,
        property.project_status_display,
        property.possession_status,
      ]),
      sourceRef: { surface: "property", recordId: property.id },
      compare: { group: "legal_project", rank: 30 },
    },
  ];
  return [
    ...rows.map(facet),
    ...decisionLabelFacets({
      propertyId: property.id,
      societyId,
      labels: property.decision_labels ?? [],
    }),
  ];
}

function mapPlaceId(propertyId: string, place: MapPlacePin, index: number): string {
  const handle = place.feature_id
    ?? place.place_entity_id
    ?? ([
      place.layer,
      place.name,
      place.source_url,
      place.source_type,
      place.latitude,
      place.longitude,
      place.distance_km,
      place.rating,
      place.review_count,
      place.note,
      place.lines?.join("+"),
    ].filter((value) => value != null).join(":")
    || `${place.layer}:${place.name}:${index}`);
  return `map:${propertyId}:place:${stableIdPart(handle)}`;
}

function mapDetail(place: MapPlacePin): string | undefined {
  const parts = [
    typeof place.distance_km === "number" && Number.isFinite(place.distance_km)
      ? `${place.distance_km.toFixed(1).replace(/\.0$/, "")} km`
      : null,
    typeof place.rating === "number" && Number.isFinite(place.rating)
      ? `Rating ${place.rating.toFixed(1)}`
      : null,
    typeof place.review_count === "number" && Number.isFinite(place.review_count)
      ? `${place.review_count} reviews`
      : null,
  ].filter((part): part is string => part != null);
  return parts.length > 0 ? parts.join(" · ") : place.note;
}

export function mapContextFacets(propertyId: string, context: PropertyMapContext | null | undefined): DecisionFacet[] {
  if (!context) return [];
  const societyId = context.home.entity_id;
  const placeFacets = context.places.map((place, index) => facet({
    id: mapPlaceId(propertyId, place, index),
    propertyId,
    societyId,
    scope: "society",
    topic: String(place.layer),
    origin: "map_fact",
    label: place.name,
    value: place.distance_km,
    detail: mapDetail(place),
    confidence: 1,
    sourceRef: {
      surface: "map",
      recordId: place.feature_id ?? place.place_entity_id ?? mapPlaceId(propertyId, place, index),
      url: place.source_url,
    },
    compare: { group: `map_${compactIdPart(String(place.layer))}`, rank: index },
  }));

  const waterFacet = context.water ? [facet({
    id: `map:${propertyId}:water`,
    propertyId,
    societyId,
    scope: "society",
    topic: "water",
    origin: "map_fact",
    label: "Water context",
    value: context.water.groundwater_class,
    detail: context.water.summary,
    sourceRef: {
      surface: "map",
      recordId: "water",
      url: context.water.source_url,
    },
    compare: { group: "map_water", rank: 0 },
  })] : [];

  return [...placeFacets, ...waterFacet];
}

function noteOrigin(note: NotebookNote): DecisionOrigin {
  if (note.block) return "smart_block";
  return "user_note";
}

function notebookTopic(note: NotebookNote): string {
  if (note.kind === "plan") return "plan_snapshot";
  if (note.block?.type === "checklist") return "checklist";
  if (note.block?.type === "fields") return "field_block";
  const prefix = note.catalogKey.split(":")[0];
  if (prefix === "nearby") return "nearby_fact";
  if (prefix === "rera") return "rera_fact";
  if (prefix === "sel") return "selection";
  if (prefix === "hand") return "note";
  return note.kind;
}

function financialFacetId(planId: string, topic: string): string {
  return `financial-plan:${stableIdPart(planId)}:${topic}`;
}

function firstCompareLabel(note: NotebookNote): string | null {
  for (const label of note.labels) {
    if (labelDef(label).compareGroup) return label;
  }
  return null;
}

function notebookCompareGroup(note: NotebookNote, topic: string): string | undefined {
  if (note.kind === "plan") return undefined;
  if (topic === "nearby_fact") return "access_notes";
  if (topic === "rera_fact") return "legal_project";
  if (topic === "selection") return "buyer_selection";
  const compareLabel = firstCompareLabel(note);
  return compareLabel ? labelDef(compareLabel).compareGroup : undefined;
}

const CONSTRUCTION_STATES: readonly ConstructionProfile["state"][] = ["ready", "under_construction"];
const CONSTRUCTION_DATE_SOURCES: readonly ConstructionProfile["dateSource"][] = [
  "rera",
  "estimated",
  "not_applicable",
];

function validateIsoDate(value: string | undefined, field: string, required: boolean): void {
  if (!value) {
    if (required) throw new RangeError(`${field} must be an ISO date`);
    return;
  }
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value);
  if (!match) throw new RangeError(`${field} must be an ISO date`);
  const [, year, month, day] = match;
  const date = new Date(`${value}T00:00:00.000Z`);
  if (
    date.getUTCFullYear() !== Number(year)
    || date.getUTCMonth() + 1 !== Number(month)
    || date.getUTCDate() !== Number(day)
  ) {
    throw new RangeError(`${field} must be an ISO date`);
  }
}

function validateConstructionProfile(profile: ConstructionProfile): void {
  if (!CONSTRUCTION_STATES.includes(profile.state)) {
    throw new RangeError("constructionProfile.state must be ready or under_construction");
  }
  if (!CONSTRUCTION_DATE_SOURCES.includes(profile.dateSource)) {
    throw new RangeError("constructionProfile.dateSource is invalid");
  }
  validateIsoDate(profile.asOfDate, "constructionProfile.asOfDate", true);
  validateIsoDate(profile.startDate, "constructionProfile.startDate", false);
  validateIsoDate(profile.completionDate, "constructionProfile.completionDate", false);
  if (
    profile.startDate
    && profile.completionDate
    && profile.completionDate < profile.startDate
  ) {
    throw new RangeError("constructionProfile.completionDate cannot be before startDate");
  }
}

export function notebookNoteFacets(notes: readonly NotebookNote[]): DecisionFacet[] {
  return notes.flatMap((note) => {
    const topic = notebookTopic(note);
    const compareGroup = notebookCompareGroup(note, topic);
    const base = facet({
      id: `notebook:${note.id}:${compactIdPart(topic)}`,
      propertyId: note.propertyId,
      scope: "property",
      topic,
      origin: noteOrigin(note),
      label: note.title,
      value: note.title,
      detail: note.detail,
      sourceRef: { surface: "notebook", recordId: note.id },
      compare: compareGroup
        ? { group: compareGroup }
        : undefined,
    });

    if (note.kind === "plan") return [base];

    const labelFacets = note.labels.map((label) => {
      const definition = labelDef(label);
      return facet({
        id: `notebook:${note.id}:label:${compactIdPart(label)}`,
        propertyId: note.propertyId,
        scope: "property",
        topic: `label:${label}`,
        origin: noteOrigin(note),
        label: definition.title,
        value: note.title,
        detail: note.detail,
        sourceRef: { surface: "notebook", recordId: note.id },
        compare: definition.compareGroup
          ? { group: definition.compareGroup }
          : undefined,
      });
    });
    return [base, ...labelFacets];
  });
}

export function savedFinancialPlanFacets(plan: SavedFinancialPlan): DecisionFacet[] {
  const loanFreeYear = plan.outputs.loanFreeYear;
  validateConstructionProfile(plan.monthlyPath.constructionProfile);
  if (loanFreeYear != null) nonNegativeNumber(loanFreeYear, "loanFreeYear");
  if (plan.outputs.breakEvenYear != null) nonNegativeNumber(plan.outputs.breakEvenYear, "breakEvenYear");
  if (plan.outputs.totalInterest != null) nonNegativeNumber(plan.outputs.totalInterest, "totalInterest");
  if (loanFreeYear == null && plan.outputs.totalInterest != null) {
    throw new RangeError("totalInterest must be null when loanFreeYear is null");
  }
  if (loanFreeYear != null && plan.outputs.totalInterest == null) {
    throw new RangeError("totalInterest must be known when loanFreeYear is known");
  }
  const propertyPrice = nonNegativeNumber(plan.shared.propertyPrice, "propertyPrice");
  nonNegativeNumber(plan.monthlyPath.monthlyEmi, "monthlyEmi");
  nonNegativeNumber(plan.monthlyPath.currentRent, "currentRent");
  nonNegativeNumber(plan.monthlyPath.monthlySip, "monthlySip");
  nonNegativeNumber(plan.monthlyPath.loanRate, "loanRate");
  nonNegativeNumber(plan.monthlyPath.sipReturn, "sipReturn");
  nonNegativeNumber(plan.monthlyPath.extraEmisPerYear, "extraEmisPerYear");
  nonNegativeNumber(plan.monthlyPath.holdingPeriodYears, "holdingPeriodYears");
  nonNegativeNumber(plan.monthlyPath.inspectedYear, "inspectedYear");
  nonNegativeNumber(plan.monthlyPath.purchaseYear, "purchaseYear");
  nonNegativeNumber(plan.monthlyPath.planAssumptions.homeAppreciationRate, "homeAppreciationRate");
  nonNegativeNumber(plan.monthlyPath.planAssumptions.rentInflationRate, "rentInflationRate");
  nonNegativeNumber(plan.outputs.loanAmount, "loanAmount");
  finiteNumber(plan.outputs.buyNetWorthAtInspectedYear, "buyNetWorthAtInspectedYear");
  finiteNumber(plan.outputs.rentNetWorthAtInspectedYear, "rentNetWorthAtInspectedYear");
  const financed = plan.monthlyPath.monthlyEmi > 0;
  assertApproxEqual(plan.outputs.loanAmount, financed ? propertyPrice : 0, "loanAmount");

  return [
    moneyFacet(plan.propertyId, plan.id, "property_price", "Property price", plan.shared.propertyPrice, 10),
    moneyFacet(plan.propertyId, plan.id, "monthly_emi", "Monthly EMI", plan.monthlyPath.monthlyEmi, 20),
    facet({
      id: financialFacetId(plan.id, "rent-sip-path"),
      propertyId: plan.propertyId,
      scope: "property",
      topic: "rent_sip_path",
      origin: "financial_plan",
      label: "Rent + SIP path",
      value: plan.monthlyPath.currentRent + plan.monthlyPath.monthlySip,
      detail: `${plan.monthlyPath.currentRent} rent + ${plan.monthlyPath.monthlySip} SIP`,
      sourceRef: { surface: "plan", recordId: plan.id },
      compare: { group: "financial_plan", rank: 30 },
    }),
    moneyFacet(plan.propertyId, plan.id, "extra_emis_per_year", "Extra EMIs/year", plan.monthlyPath.extraEmisPerYear, 40),
    facet({
      id: financialFacetId(plan.id, "loan-free-year"),
      propertyId: plan.propertyId,
      scope: "property",
      topic: "loan_free_year",
      origin: "financial_plan",
      label: "Loan-free year",
      value: plan.outputs.loanFreeYear ?? "Does not close",
      sourceRef: { surface: "plan", recordId: plan.id },
      compare: { group: "financial_plan", rank: 100 },
    }),
    facet({
      id: financialFacetId(plan.id, "inspected-year-outcome"),
      propertyId: plan.propertyId,
      scope: "property",
      topic: "inspected_year_outcome",
      origin: "financial_plan",
      label: "Inspected-year outcome",
      value: plan.outputs.buyNetWorthAtInspectedYear - plan.outputs.rentNetWorthAtInspectedYear,
      detail: `Year ${plan.monthlyPath.inspectedYear}`,
      sourceRef: { surface: "plan", recordId: plan.id },
      compare: { group: "financial_plan", rank: 110 },
    }),
  ];
}
