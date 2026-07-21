import type {
  EvidenceSection,
  PropertyCard,
  PropertyEvidenceResponse,
  SearchResultItem,
} from "./types.ts";

export type EvidenceConstellation =
  | "value"
  | "trust"
  | "lifestyle"
  | "risk"
  | "commute"
  | "investment";

export type EvidenceSummary = {
  factCount: number;
  gapCount: number;
  sectionCount: number;
  sourceTypes: string[];
};

export type UniverseClusterId =
  | "strong_fits"
  | "worth_comparing"
  | "verify_proof"
  | "value_angle"
  | "explore";

export type UniverseCluster = {
  id: UniverseClusterId;
  label: string;
  hint: string;
  results: SearchResultItem[];
};

const CONSTELLATION_META: Record<
  EvidenceConstellation,
  { label: string; hint: string }
> = {
  value: { label: "Value", hint: "Price, market trail, benchmarks" },
  trust: { label: "Trust", hint: "RERA, builder, official records" },
  lifestyle: { label: "Lifestyle", hint: "Reviews, society pulse, living feel" },
  risk: { label: "Risk", hint: "Gaps, complaints, verify-first signals" },
  commute: { label: "Commute", hint: "Metro, traffic, nearby anchors" },
  investment: { label: "Investment", hint: "Demand, resale, upside" },
};

export function sectionConstellation(
  section: Pick<EvidenceSection, "kind" | "constellation">,
): EvidenceConstellation {
  return section.constellation ?? "trust";
}

export function constellationMeta(id: EvidenceConstellation) {
  return CONSTELLATION_META[id];
}

export function visibleEvidenceSections(
  sections: EvidenceSection[] | undefined,
): EvidenceSection[] {
  if (!sections?.length) return [];
  return sections
    .filter((section) =>
      section.community_pulse != null
      || section.items.length > 0
      || (section.media?.some((strip) => strip.frames.length > 0) ?? false),
    )
    .sort((a, b) => a.priority - b.priority);
}

export function summarizeEvidence(
  evidence: PropertyEvidenceResponse | undefined,
): EvidenceSummary | null {
  if (!evidence?.sections?.length) return null;

  const sections = visibleEvidenceSections(evidence.sections);
  if (sections.length === 0) return null;

  const factCount = sections.reduce((sum, section) => {
    if (section.community_pulse) {
      const pulse = section.community_pulse;
      return sum
        + pulse.quotes.length
        + pulse.positives.length
        + pulse.concerns.length
        + (pulse.paragraph.trim().length > 0 ? 1 : 0);
    }
    return sum + section.items.length;
  }, 0);
  const sourceTypes = humanizeSourceTypes([
    ...new Set(sections.flatMap((section) => {
      if (section.community_pulse) {
        return [section.community_pulse.source_label.replace(/ review$/i, "")];
      }
      return section.source_types;
    })),
  ]).slice(0, 4);

  return {
    factCount,
    gapCount: 0,
    sectionCount: sections.length,
    sourceTypes,
  };
}

const INTERNAL_SOURCE_TYPES = new Set(["computed", "manual", "system"]);

/** Hide pipeline source types from buyer-facing UI. */
export function displaySourceType(sourceType: string | undefined): string | null {
  if (!sourceType) return null;
  const lowered = sourceType.trim().toLowerCase();
  if (INTERNAL_SOURCE_TYPES.has(lowered)) return null;
  if (lowered === "rera") return "RERA";
  return sourceType;
}

export function humanizeSourceTypes(types: string[]): string[] {
  return types
    .map(displaySourceType)
    .filter((value): value is string => value !== null);
}

export function evidenceReceiptLabel(summary: EvidenceSummary): string | null {
  if (summary.sourceTypes.length === 0) return null;
  return summary.sourceTypes.slice(0, 2).join(" · ");
}

export function groupSectionsByConstellation(
  sections: EvidenceSection[],
): Array<{ id: EvidenceConstellation; sections: EvidenceSection[] }> {
  const buckets = new Map<EvidenceConstellation, EvidenceSection[]>();
  for (const section of sections) {
    const id = sectionConstellation(section);
    const list = buckets.get(id) ?? [];
    list.push(section);
    buckets.set(id, list);
  }

  const order: EvidenceConstellation[] = [
    "value",
    "trust",
    "lifestyle",
    "commute",
    "investment",
    "risk",
  ];

  return order
    .filter((id) => buckets.has(id))
    .map((id) => ({ id, sections: buckets.get(id)! }));
}

export const UNIVERSE_CLUSTER_MIN_RESULTS = 5;

export function clusterSearchResults(
  results: SearchResultItem[],
  evidenceById: Map<string, PropertyEvidenceResponse>,
): UniverseCluster[] {
  if (results.length < UNIVERSE_CLUSTER_MIN_RESULTS) return [];
  const buckets: Record<UniverseClusterId, SearchResultItem[]> = {
    strong_fits: [],
    worth_comparing: [],
    verify_proof: [],
    value_angle: [],
    explore: [],
  };

  for (const result of results) {
    const summary = summarizeEvidence(evidenceById.get(result.id));
    const label = result.match_label.toLowerCase();
    const score = result.match_score ?? 0;

    if (summary && summary.factCount < 3) {
      buckets.verify_proof.push(result);
      continue;
    }
    if (label.includes("strong") || score >= 0.55) {
      buckets.strong_fits.push(result);
      continue;
    }
    if (label.includes("value")) {
      buckets.value_angle.push(result);
      continue;
    }
    if (label.includes("good") || label.includes("partial") || score >= 0.35) {
      buckets.worth_comparing.push(result);
      continue;
    }
    buckets.explore.push(result);
  }

  const defs: Array<{ id: UniverseClusterId; label: string; hint: string }> = [
    { id: "strong_fits", label: "Strong fits", hint: "Closest to your search" },
    { id: "worth_comparing", label: "Worth comparing", hint: "More options in this search" },
    { id: "verify_proof", label: "Thinner context", hint: "Fewer linked sources so far" },
    { id: "value_angle", label: "Value angle", hint: "Price-led matches" },
    { id: "explore", label: "Explore further", hint: "Broader matches" },
  ];

  return defs
    .map((def) => ({ ...def, results: buckets[def.id] }))
    .filter((cluster) => cluster.results.length > 0);
}

export function topEvidenceGlance(
  evidence: PropertyEvidenceResponse | undefined,
  limit = 2,
  excludeKinds: string[] = [],
): string[] {
  const excluded = new Set(excludeKinds);
  const sections = visibleEvidenceSections(evidence?.sections).filter(
    (section) => !excluded.has(section.kind),
  );
  return sections
    .slice(0, limit)
    .map((section) => section.summary || section.title)
    .filter(Boolean);
}

export function entityRefCount(card: PropertyCard): number {
  const refs = card.kg_entity_refs;
  let count = 0;
  if (refs.property_entity_id) count += 1;
  if (refs.society_entity_id) count += 1;
  if (refs.area_entity_id) count += 1;
  if (refs.builder_entity_id) count += 1;
  count += refs.source_entity_ids?.length ?? 0;
  return count;
}
