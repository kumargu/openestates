import type {
  FiledPlanPreview,
  ReraBuyerComplaintSummary,
  ReraBuyerFact,
  ReraBuyerFactSection,
  ReraBuyerDocument,
  ReraEvidenceClaim,
  ReraEvidenceClaimValue,
  ReraEvidenceEvent,
  ReraEvidenceProjection,
  ReraEvidenceSource,
  ReraEvidenceReportResponse,
  ReraRegulatoryCoverage,
  ReraReportSurfaceSection,
} from "./types.ts";

const SQUARE_FEET_PER_SQUARE_METRE = 10.763910416709722;

export type ReraModuleState =
  | "available"
  | "partial"
  | "stale"
  | "conflicting"
  | "missing"
  | "not_applicable";

export type ReraSummaryFact = {
  id: "registrations" | "completion" | "quarterly_progress" | "complaints_orders";
  label: string;
  value: string;
  detail?: string;
  state: ReraModuleState;
  sourceUrl?: string;
};

export type ReraRegistrationView = {
  id: string;
  scope: string;
  number?: string;
  status?: string;
  units?: string;
  completion?: string;
  sourceUrl?: string;
  state: ReraModuleState;
};

export type ReraDeliveryItem = {
  id: string;
  registrationId?: string;
  label: string;
  value: string;
  sourceUrl?: string;
};

export type ReraQuarterlyFilingView = {
  id: string;
  registrationId: string;
  period: string;
  filedAt: string;
  totalUnits?: number;
  bookedUnits?: number;
  unsoldUnits?: number;
  sourceUrl?: string;
};

export type ReraCoverageItem = {
  id: "registrations" | "qprs" | "plans_approvals" | "complaints_orders" | "completion_certificate";
  label: string;
  state: ReraModuleState;
  detail: string;
};

export type ReraReportViewModel = {
  state: ReraModuleState;
  hasData: boolean;
  checkedAt?: string;
  registryUrl?: string;
  registrations: ReraRegistrationView[];
  summary: ReraSummaryFact[];
  delivery: ReraDeliveryItem[];
  quarterlyFilings: ReraQuarterlyFilingView[];
  projectComplaints?: ReraBuyerComplaintSummary;
  promoterComplaints?: ReraBuyerComplaintSummary;
  coverage: ReraCoverageItem[];
};

/** Keep registry page sequences readable without project-specific filename rules. */
export function orderReraDocuments(documents: ReraBuyerDocument[]): ReraBuyerDocument[] {
  const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });
  return [...documents].sort((left, right) => (
    collator.compare(left.group_label, right.group_label)
    || collator.compare(left.label, right.label)
    || collator.compare(left.id, right.id)
  ));
}

export function selectReraPlanPreviews(
  previews: FiledPlanPreview[],
  allowedKinds: string[],
): FiledPlanPreview[] {
  const allowed = new Set(allowedKinds);
  return previews.filter((preview) => allowed.has(preview.kind));
}

export function orderReraRegulatoryEvents(
  events: ReraEvidenceEvent[],
  eventClassOrder: string[],
): ReraEvidenceEvent[] {
  const priority = new Map(eventClassOrder.map((eventClass, index) => [eventClass, index]));
  return [...events].sort((left, right) => (
    (priority.get(left.event_class) ?? Number.MAX_SAFE_INTEGER)
    - (priority.get(right.event_class) ?? Number.MAX_SAFE_INTEGER)
    || right.occurred_at.localeCompare(left.occurred_at)
    || left.event_id.localeCompare(right.event_id)
  ));
}

export function previewReraRegulatoryEvents(
  events: ReraEvidenceEvent[],
  eventClassOrder: string[],
  limit = 3,
): ReraEvidenceEvent[] {
  return orderReraRegulatoryEvents(events, eventClassOrder).slice(0, Math.max(0, limit));
}

export function regulatoryCoverageNote(
  coverage: ReraRegulatoryCoverage[],
  outsideReleaseNote: string,
): string | null {
  const checkedSources = [...new Set(
    coverage
      .filter((item) => item.status.trim().toLowerCase() === "checked")
      .map((item) => item.source.trim())
      .filter(Boolean),
  )].sort();
  if (checkedSources.length === 0) return null;
  const sources = checkedSources.length === 1
    ? checkedSources[0]
    : `${checkedSources.slice(0, -1).join(", ")} and ${checkedSources.at(-1)}`;
  const outside = outsideReleaseNote.trim();
  return outside ? `${sources} checked; ${outside}` : `${sources} checked.`;
}

export type ReraRegulatoryEventPresentation = {
  supportingEvidence?: ReraEvidenceClaim["evidence"][number];
  source?: ReraEvidenceSource;
  actionLabel: "Open filing" | "Open order";
};

export function regulatoryEventPresentation(
  event: ReraEvidenceEvent,
  evidence: ReraEvidenceProjection,
): ReraRegulatoryEventPresentation {
  const claims = event.claim_ids
    .map((claimId) => evidence.claims.find((claim) => claim.claim_id === claimId))
    .filter((claim): claim is ReraEvidenceClaim => Boolean(claim));
  const supportingEvidence = claims.flatMap((claim) => claim.evidence)
    .find((item) => item.supporting_quote);
  const source = event.source_ids
    .map((sourceId) => evidence.source_index.find((item) => item.receipt_id === sourceId))
    .find((item): item is ReraEvidenceSource => Boolean(item));
  const filing = claims.some((claim) => (
    claim.assertion_mode === "promoter_declaration"
    || claim.assertion_mode === "complainant_allegation"
    || claim.assertion_mode === "registry_record"
  ));
  return {
    supportingEvidence,
    source,
    actionLabel: filing ? "Open filing" : "Open order",
  };
}

export type ReraDisplayFact = {
  id: string;
  label: string;
  value: string;
  assertion: string;
  claims: ReraEvidenceClaim[];
};

export type ReraInventoryChartRow = {
  id: string;
  label: string;
  homes?: number;
  homesDisplay: string;
  homesPercent: number;
  carpetAreaPerHome?: number;
  carpetAreaPerHomeDisplay: string;
  carpetAreaPerHomePercent: number;
  carpetAreaLabel?: string;
};

export function knownText(value?: string | null): string | null {
  const normalized = value?.trim();
  if (!normalized) return null;
  if (["unknown", "not specified", "n/a", "na", "none", "null"].includes(normalized.toLowerCase())) {
    return null;
  }
  return normalized;
}

export function httpUrl(value?: string): string | null {
  const known = knownText(value);
  if (!known) return null;
  try {
    const url = new URL(known);
    return url.protocol === "http:" || url.protocol === "https:" ? url.toString() : null;
  } catch {
    return null;
  }
}

export function selectorMatches(selector: string, candidate: string): boolean {
  const normalizedSelector = selector.trim().toLowerCase();
  const normalizedCandidate = candidate.trim().toLowerCase();
  if (normalizedSelector.endsWith("*")) {
    return normalizedCandidate.startsWith(normalizedSelector.slice(0, -1));
  }
  return normalizedSelector === normalizedCandidate;
}

export function sectionHasEvidence(
  section: ReraReportSurfaceSection,
  evidence: ReraEvidenceProjection,
): boolean {
  return section.selectors.some(({ key }) => {
    if (key.startsWith("claim:")) {
      return evidence.claims.some((claim) => selectorMatches(key, `claim:${claim.predicate}`));
    }
    if (key.startsWith("event:")) {
      return evidence.events.some((event) => selectorMatches(key, `event:${event.event_type}`));
    }
    if (key.startsWith("series:")) {
      const seriesKey = key.split(".", 1)[0]!;
      return evidence.series.some((series) => selectorMatches(seriesKey, `series:${series.series_type}`));
    }
    if (key.startsWith("entity:")) {
      return evidence.entities.some((entity) => selectorMatches(key, `entity:${entity.entity_type}`));
    }
    return false;
  });
}

export function claimsForSelector(
  claims: ReraEvidenceClaim[],
  selector: string,
  subjectId?: string,
): ReraEvidenceClaim[] {
  return claims.filter((claim) => (
    (!subjectId || claim.subject.entity_id === subjectId)
    && selectorMatches(selector, `claim:${claim.predicate}`)
  ));
}

export function claimValueText(value: ReraEvidenceClaimValue, format?: string): string {
  if (value.type === "boolean") return value.data ? "Yes" : "No";
  if (value.type === "number") {
    if (format === "square_feet_from_square_metres") {
      const squareFeet = Math.round(value.data * SQUARE_FEET_PER_SQUARE_METRE);
      return `${squareFeet.toLocaleString("en-IN")} sq ft`;
    }
    const number = value.data.toLocaleString("en-IN", { maximumFractionDigits: 2 });
    return format === "square_metres" ? `${number} m²` : number;
  }
  if (value.type === "money") return `${value.data.currency} ${value.data.amount}`;
  if (value.type === "entity_ref") return value.data.entity_id;
  return value.data;
}

function numericClaimValue(claim?: ReraEvidenceClaim): number | undefined {
  if (
    claim?.value.type !== "number"
    || !Number.isFinite(claim.value.data)
    || claim.value.data < 0
  ) return undefined;
  return claim.value.data;
}

/**
 * Projects configured inventory selectors into two independently scaled measures.
 * The first claim selector is the count; later selectors are ordered total-area
 * fallbacks. Per-home carpet area is derived only when both values are valid.
 */
export function projectReraInventoryChart(
  section: ReraReportSurfaceSection,
  evidence: ReraEvidenceProjection,
): ReraInventoryChartRow[] {
  const entities = evidence.entities.filter((entity) => entity.entity_type === "inventory_configuration");
  const valueSelectors = section.selectors.filter(({ key }) => key.startsWith("claim:"));
  const [homesSelector, ...carpetAreaSelectors] = valueSelectors;
  if (!homesSelector) return [];

  const uniqueRows = new Map<string, {
    id: string;
    label: string;
    claims: ReraEvidenceClaim[];
  }>();
  for (const entity of entities) {
    const claims = evidence.claims.filter((claim) => claim.subject.entity_id === entity.entity_id);
    const values = valueSelectors.map((selector) => {
      const claim = claimsForSelector(claims, selector.key)[0];
      return claim ? claimValueText(claim.value, selector.format) : "—";
    });
    const normalizedLabel = (entity.label ?? "")
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "");
    uniqueRows.set(`${normalizedLabel}:${values.join("|")}`, {
      id: entity.entity_id,
      label: entity.label ?? "Filed configuration",
      claims,
    });
  }

  const rows = [...uniqueRows.values()].map((row) => {
    const homesClaim = claimsForSelector(row.claims, homesSelector.key)[0];
    const carpetAreaSelection = carpetAreaSelectors
      .map((selector) => ({
        selector,
        claim: claimsForSelector(row.claims, selector.key)[0],
      }))
      .find(({ claim }) => numericClaimValue(claim) !== undefined);
    const homes = numericClaimValue(homesClaim);
    const carpetArea = numericClaimValue(carpetAreaSelection?.claim);
    const carpetAreaPerHome = homes !== undefined && homes > 0 && carpetArea !== undefined
      ? carpetArea / homes
      : undefined;
    return {
      ...row,
      homes,
      homesDisplay: homesClaim ? claimValueText(homesClaim.value, homesSelector.format) : "—",
      homesPercent: 0,
      carpetAreaPerHome,
      carpetAreaPerHomeDisplay: carpetAreaPerHome !== undefined && carpetAreaSelection
        ? claimValueText(
          { type: "number", data: carpetAreaPerHome },
          carpetAreaSelection.selector.format,
        )
        : "—",
      carpetAreaPerHomePercent: 0,
      carpetAreaLabel: carpetAreaSelection?.selector.label,
    };
  }).filter(({ homes }) => homes !== undefined);
  const maxHomes = Math.max(0, ...rows.map(({ homes }) => homes ?? 0));
  const maxCarpetAreaPerHome = Math.max(
    0,
    ...rows.map(({ carpetAreaPerHome }) => carpetAreaPerHome ?? 0),
  );
  return rows.map((row) => ({
    ...row,
    homesPercent: maxHomes > 0 && row.homes !== undefined ? (row.homes / maxHomes) * 100 : 0,
    carpetAreaPerHomePercent: maxCarpetAreaPerHome > 0 && row.carpetAreaPerHome !== undefined
      ? (row.carpetAreaPerHome / maxCarpetAreaPerHome) * 100
      : 0,
  }));
}

export function assertionLabel(mode: ReraEvidenceClaim["assertion_mode"]): string {
  switch (mode) {
    case "registry_record": return "Registry record";
    case "promoter_declaration": return "Promoter declaration";
    case "complainant_allegation": return "Allegation";
    case "authority_order": return "Authority order";
    case "system_derivation": return "Calculated from filed records";
  }
}

export function displayFactsForSection(
  section: ReraReportSurfaceSection,
  evidence: ReraEvidenceProjection,
): ReraDisplayFact[] {
  const grouped = new Map<string, ReraDisplayFact>();
  for (const selector of section.selectors.filter(({ key }) => key.startsWith("claim:"))) {
    for (const claim of claimsForSelector(evidence.claims, selector.key)) {
      const value = claimValueText(claim.value, selector.format);
      if (value.trim().endsWith(":")) continue;
      const key = `${claim.subject.entity_id}\u0000${claim.predicate}\u0000${value}`;
      const existing = grouped.get(key);
      if (existing) {
        existing.claims.push(claim);
      } else {
        grouped.set(key, {
          id: claim.claim_id,
          label: selector.label,
          value,
          assertion: assertionLabel(claim.assertion_mode),
          claims: [claim],
        });
      }
    }
  }
  return [...grouped.values()].sort((left, right) => left.label.localeCompare(right.label));
}

export function formatReraDate(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return new Intl.DateTimeFormat("en-IN", {
    day: "numeric",
    month: "short",
    year: "numeric",
  }).format(date);
}

function buyerSection(
  sections: ReraBuyerFactSection[] | undefined,
  id: string,
): ReraBuyerFactSection | undefined {
  return sections?.find((section) => section.id === id);
}

function buyerFact(section: ReraBuyerFactSection | undefined, key: string): ReraBuyerFact | undefined {
  return section?.facts.find((fact) => fact.key === key);
}

function claimText(
  evidence: ReraEvidenceProjection,
  subjectId: string,
  predicate: string,
): string | undefined {
  const claim = evidence.claims.find((candidate) => (
    candidate.subject.entity_id === subjectId && candidate.predicate === predicate
  ));
  return claim ? knownText(claimValueText(claim.value)) ?? undefined : undefined;
}

function claimSourceUrl(
  evidence: ReraEvidenceProjection,
  subjectId: string,
  predicates: string[],
): string | undefined {
  const receiptIds = evidence.claims
    .filter((claim) => claim.subject.entity_id === subjectId && predicates.includes(claim.predicate))
    .flatMap((claim) => claim.evidence.map((item) => item.receipt_id));
  return evidence.source_index
    .find((source) => receiptIds.includes(source.receipt_id) && httpUrl(source.source_url))
    ?.source_url;
}

function latestKnownDate(values: Array<string | undefined>): string | undefined {
  return values
    .filter((value): value is string => Boolean(value && !Number.isNaN(new Date(value).getTime())))
    .sort((left, right) => new Date(left).getTime() - new Date(right).getTime())
    .at(-1);
}

function displayStatus(value: string): string {
  return value.toLowerCase().replace(/(^|[_\s-])\S/g, (letter) => letter.toUpperCase());
}

function sourceUrlForClaimIds(
  evidence: ReraEvidenceProjection,
  claimIds: string[],
): string | undefined {
  const receiptIds = evidence.claims
    .filter((claim) => claimIds.includes(claim.claim_id))
    .flatMap((claim) => claim.evidence.map((item) => item.receipt_id));
  return evidence.source_index.find((source) => receiptIds.includes(source.receipt_id))?.source_url;
}

function reportState(report: ReraEvidenceReportResponse, hasData: boolean): ReraModuleState {
  if (!hasData) return "missing";
  if (report.evidence.regulatory_coverage.some((coverage) => coverage.status.toLowerCase() === "conflicting")) {
    return "conflicting";
  }
  if (report.evidence.regulatory_coverage.some((coverage) => coverage.status.toLowerCase() === "stale")) {
    return "stale";
  }
  return report.availability === "available" ? "available" : "partial";
}

function completionCertificateState(
  documents: ReraBuyerDocument[],
  currentCompletion: string | undefined,
  now: Date,
): Pick<ReraCoverageItem, "state" | "detail"> {
  const completionDocument = documents.find((document) => (
    /(?:completion|occupancy)\s+certificate|\boc\b|\bcc\b/i.test(document.label)
  ));
  if (completionDocument) return { state: "available", detail: "Found in official documents" };
  const completionDate = currentCompletion ? new Date(currentCompletion) : undefined;
  if (completionDate && !Number.isNaN(completionDate.getTime()) && completionDate > now) {
    return { state: "not_applicable", detail: "Current completion date is in the future" };
  }
  return { state: "missing", detail: "Not found in the available documents" };
}

/**
 * Resolve the dense canonical projection and the older buyer-report fallback into
 * one adaptive page model. Presentation components should not need to infer
 * registration scope, coverage, or missing states themselves.
 */
export function buildReraReportViewModel(
  report: ReraEvidenceReportResponse,
  now = new Date(),
): ReraReportViewModel {
  const evidence = report.evidence;
  const buyer = report.buyer_report;
  const registrationSection = buyerSection(buyer?.fact_sections, "registration");
  const scheduleSection = buyerSection(buyer?.fact_sections, "schedule");
  const fallbackNumber = buyerFact(registrationSection, "rera_number")
    ?? registrationSection?.facts.find((fact) => /registration number/i.test(fact.label));
  const fallbackStatus = buyerFact(registrationSection, "rera_status");
  const fallbackCompletion = buyerFact(scheduleSection, "rera_completion_date");
  const originalCompletion = buyerFact(scheduleSection, "rera_original_completion_date");
  const movement = buyerFact(scheduleSection, "rera_delay_months");
  const fallbackUnits = buyerFact(buyerSection(buyer?.fact_sections, "overview"), "project_unit_count")
    ?? buyerSection(buyer?.fact_sections, "overview")?.facts.find((fact) => fact.label === "Homes");
  const registryUrl = httpUrl(buyer?.registry_url) ?? undefined;

  const registrationEntities = evidence.entities.filter((entity) => entity.entity_type === "registration");
  const registrationIds = [...new Set([
    ...evidence.registration_ids,
    ...registrationEntities.map((entity) => entity.registration_id ?? entity.entity_id),
  ])];
  if (registrationIds.length === 0 && knownText(fallbackNumber?.value)) {
    registrationIds.push(`buyer-report:${fallbackNumber!.value}`);
  }

  const registrations = registrationIds.map((id, index): ReraRegistrationView => {
    const entity = registrationEntities.find((candidate) => (
      candidate.entity_id === id || candidate.registration_id === id
    ));
    const claimSubject = entity?.entity_id ?? id;
    const canonicalNumber = claimText(evidence, claimSubject, "official_registration_number");
    const canonicalCompletion = claimText(evidence, claimSubject, "proposed_completion_date");
    const canonicalUnits = claimText(evidence, claimSubject, "declared_unit_count");
    const canonicalName = claimText(evidence, claimSubject, "registry_project_name");
    const mayUseFallback = registrationIds.length === 1;
    const number = canonicalNumber ?? (mayUseFallback ? knownText(fallbackNumber?.value) ?? undefined : undefined);
    const completion = (mayUseFallback ? knownText(fallbackCompletion?.value) ?? undefined : undefined)
      ?? canonicalCompletion;
    const status = mayUseFallback ? knownText(fallbackStatus?.value) ?? undefined : undefined;
    const sourceUrl = claimSourceUrl(evidence, claimSubject, [
      "official_registration_number",
      "proposed_completion_date",
      "declared_unit_count",
    ]) ?? httpUrl(fallbackNumber?.source_url) ?? registryUrl;
    return {
      id,
      scope: knownText(entity?.label) ?? canonicalName ?? (registrationIds.length > 1
        ? `Registration ${index + 1}`
        : "Project registration"),
      number,
      status,
      units: canonicalUnits ?? (mayUseFallback ? knownText(fallbackUnits?.value) ?? undefined : undefined),
      completion,
      sourceUrl,
      state: canonicalNumber && canonicalCompletion ? "available" : "partial",
    };
  });

  const quarterlyFilings = evidence.series
    .filter((series) => series.series_type === "quarterly_inventory")
    .flatMap((series) => series.points.map((point): ReraQuarterlyFilingView => ({
      id: point.point_id,
      registrationId: series.registration_id,
      period: [point.quarter, point.financial_year].filter(Boolean).join(" · ") || "Filed quarter",
      filedAt: point.effective_at,
      totalUnits: point.total_units,
      bookedUnits: point.booked_units,
      unsoldUnits: point.unsold_units,
      sourceUrl: sourceUrlForClaimIds(evidence, point.claim_ids),
    })))
    .sort((left, right) => right.filedAt.localeCompare(left.filedAt));

  const currentCompletion = registrations.length === 1
    ? registrations[0]?.completion
    : undefined;
  const delivery: ReraDeliveryItem[] = [];
  if (
    registrations.length === 1
    && originalCompletion
    && knownText(originalCompletion.value)
    && originalCompletion.value !== currentCompletion
  ) {
    delivery.push({
      id: "original-completion",
      registrationId: registrations[0]?.id,
      label: "Original completion",
      value: originalCompletion.value,
      sourceUrl: httpUrl(originalCompletion.source_url) ?? registryUrl,
    });
  }
  for (const event of evidence.events
    .filter((item) => item.event_type === "registration_extended")
    .sort((left, right) => left.occurred_at.localeCompare(right.occurred_at))) {
    const source = event.source_ids
      .map((sourceId) => evidence.source_index.find((item) => item.receipt_id === sourceId))
      .find((item): item is ReraEvidenceSource => Boolean(item));
    delivery.push({
      id: event.event_id,
      registrationId: event.registration_id,
      label: "Extension recorded",
      value: event.occurred_at,
      sourceUrl: source?.source_url,
    });
  }
  if (currentCompletion) {
    delivery.push({
      id: "current-completion",
      registrationId: registrations[0]?.id,
      label: "Current completion",
      value: currentCompletion,
      sourceUrl: httpUrl(fallbackCompletion?.source_url) ?? registrations[0]?.sourceUrl ?? registryUrl,
    });
  }

  const projectComplaints = buyer?.complaints?.find((item) => item.scope === "project");
  const promoterComplaints = buyer?.complaints?.find((item) => item.scope === "promoter");
  const latestFiling = quarterlyFilings[0];
  const projectRecordCount = (projectComplaints?.total ?? 0) + evidence.events.length;
  const hasBuyerFacts = buyer?.fact_sections.some((section) => section.facts.length > 0) ?? false;
  const hasData = evidence.claims.length > 0 || hasBuyerFacts;
  const state = reportState(report, hasData);
  const completionState: ReraModuleState = currentCompletion ? "available" : "missing";
  const complaintState: ReraModuleState = projectComplaints || evidence.events.length > 0
    ? "available"
    : "missing";

  const summary: ReraSummaryFact[] = [
    {
      id: "registrations",
      label: "Registration",
      value: registrations.length > 0
        ? `${registrations.length} ${registrations.length === 1 ? "registration" : "registrations"}`
        : "Match unresolved",
      detail: registrations.length > 0
        ? registrations.map((item) => item.status ? displayStatus(item.status) : undefined).filter(Boolean).join(" · ") || "Official record found"
        : "No exact registration match",
      state: registrations.length > 0 ? (report.availability === "available" ? "available" : "partial") : "missing",
      sourceUrl: registrations[0]?.sourceUrl ?? registryUrl,
    },
    {
      id: "completion",
      label: "Declared completion",
      value: currentCompletion ? formatReraDate(currentCompletion) : "Not in record",
      detail: movement ? `${movement.value} movement from original date` : undefined,
      state: completionState,
      sourceUrl: registrations[0]?.sourceUrl ?? registryUrl,
    },
    {
      id: "quarterly_progress",
      label: "Latest quarterly filing",
      value: latestFiling?.period ?? "Not available",
      detail: latestFiling ? `Filed ${formatReraDate(latestFiling.filedAt)} · Promoter reported` : "No QPR series in this record",
      state: latestFiling ? "available" : "missing",
      sourceUrl: latestFiling?.sourceUrl ?? registryUrl,
    },
    {
      id: "complaints_orders",
      label: "Project complaints and orders",
      value: projectComplaints || evidence.events.length > 0
        ? `${projectRecordCount.toLocaleString("en-IN")} ${projectRecordCount === 1 ? "record" : "records"}`
        : "Not matched",
      detail: promoterComplaints ? `${promoterComplaints.total.toLocaleString("en-IN")} promoter-wide shown as context` : undefined,
      state: complaintState,
      sourceUrl: registryUrl,
    },
  ];

  const documents = buyer?.documents ?? [];
  const planDocuments = documents.filter((document) => document.group === "plans");
  const approvalDocuments = documents.filter((document) => document.group === "approvals_nocs");
  const completionCertificate = completionCertificateState(documents, currentCompletion, now);
  const checkedAt = latestKnownDate(evidence.regulatory_coverage.map((item) => item.checked_at))
    ?? latestKnownDate(buyer?.fact_sections.flatMap((section) => section.facts.map((fact) => fact.learned_at)) ?? [])
    ?? evidence.generated_at;

  return {
    state,
    hasData,
    checkedAt,
    registryUrl,
    registrations,
    summary,
    delivery,
    quarterlyFilings,
    projectComplaints,
    promoterComplaints,
    coverage: [
      {
        id: "registrations",
        label: "Registrations",
        state: registrations.length > 0 ? (report.availability === "available" ? "available" : "partial") : "missing",
        detail: registrations.length > 0 ? `${registrations.length} found` : "Exact match unresolved",
      },
      {
        id: "qprs",
        label: "Quarterly filings",
        state: quarterlyFilings.length > 0 ? "available" : "missing",
        detail: quarterlyFilings.length > 0
          ? `${quarterlyFilings.length} ${quarterlyFilings.length === 1 ? "filing period" : "filing periods"} found`
          : "No QPR series found",
      },
      {
        id: "plans_approvals",
        label: "Plans and approvals",
        state: planDocuments.length + approvalDocuments.length > 0 ? "available" : "missing",
        detail: `${planDocuments.length + approvalDocuments.length} documents found`,
      },
      {
        id: "complaints_orders",
        label: "Complaints and orders",
        state: complaintState,
        detail: projectComplaints || evidence.events.length > 0
          ? `${projectRecordCount} ${projectRecordCount === 1 ? "project record" : "project records"} found`
          : "No project-specific match found",
      },
      {
        id: "completion_certificate",
        label: "Completion or occupancy certificate",
        ...completionCertificate,
      },
    ],
  };
}
