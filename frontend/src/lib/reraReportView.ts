import type {
  ReraEvidenceClaim,
  ReraEvidenceClaimValue,
  ReraEvidenceProjection,
  ReraReportSurfaceSection,
} from "./types.ts";

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

export function displayName(value: string): string {
  const keepUpper = new Set(["BHK", "ITPL", "JP", "KR", "NOC", "RERA", "BBMP", "BDA"]);
  return value
    .replace(/^(\d+(?:\.\d+)?)\s+BHK\s+(?:in|at)\s+/i, "$1 BHK ")
    .replace(/\b[A-Z][A-Z0-9&.'-]*\b/g, (word) => {
      if (keepUpper.has(word) || /\d/.test(word)) return word;
      return word.charAt(0) + word.slice(1).toLowerCase();
    });
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
