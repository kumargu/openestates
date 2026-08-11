import type {
  FiledPlanPreview,
  ReraBuyerDocument,
  ReraEvidenceClaim,
  ReraEvidenceClaimValue,
  ReraEvidenceEvent,
  ReraEvidenceProjection,
  ReraEvidenceSource,
  ReraRegulatoryCoverage,
  ReraReportSurfaceSection,
} from "./types.ts";

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
    const number = value.data.toLocaleString("en-IN", { maximumFractionDigits: 2 });
    return format === "square_metres" ? `${number} m²` : number;
  }
  if (value.type === "money") return `${value.data.currency} ${value.data.amount}`;
  if (value.type === "entity_ref") return value.data.entity_id;
  return value.data;
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
