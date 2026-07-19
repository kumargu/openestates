import type {
  EvidenceSection,
  PropertyCard,
  PropertyDetailResponse,
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
  const sourceTypes = [
    ...new Set(sections.flatMap((section) => {
      if (section.community_pulse) {
        return [section.community_pulse.source_label.replace(/ review$/i, "")];
      }
      return section.source_types;
    })),
  ].slice(0, 4);

  return {
    factCount,
    gapCount: 0,
    sectionCount: sections.length,
    sourceTypes,
  };
}

export function evidenceReceiptLabel(summary: EvidenceSummary): string {
  if (summary.sourceTypes.length === 0) {
    return `${summary.factCount} facts`;
  }
  return `${summary.factCount} facts · ${summary.sourceTypes.slice(0, 2).join(", ")}`;
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
    { id: "strong_fits", label: "Strong fits", hint: "Best alignment with your search" },
    { id: "worth_comparing", label: "Worth comparing", hint: "Solid options to shortlist" },
    { id: "verify_proof", label: "Verify proof first", hint: "Promising but evidence gaps remain" },
    { id: "value_angle", label: "Value angle", hint: "Price-led opportunities" },
    { id: "explore", label: "Explore further", hint: "Broader matches in this world" },
  ];

  return defs
    .map((def) => ({ ...def, results: buckets[def.id] }))
    .filter((cluster) => cluster.results.length > 0);
}

export function topEvidenceGlance(
  evidence: PropertyEvidenceResponse | undefined,
  limit = 2,
): string[] {
  const sections = visibleEvidenceSections(evidence?.sections);
  return sections
    .slice(0, limit)
    .map((section) => section.summary || section.title)
    .filter(Boolean);
}

export function briefHookLine(
  brief: PropertyDetailResponse["livability_brief"],
): string | null {
  if (!brief) return null;
  const riskBlock = brief.blocks.find((block) => block.lens === "risk");
  if (riskBlock?.themes[0]) return riskBlock.themes[0];
  const operatingBlock = brief.blocks.find((block) => block.lens === "operating");
  if (operatingBlock?.themes[0]) return operatingBlock.themes[0];
  return riskBlock?.paragraph ?? operatingBlock?.paragraph ?? null;
}

export function tileDecisionRead(
  result: SearchResultItem,
  summary: EvidenceSummary | null,
): string {
  if (summary && summary.factCount < 3) return "Verify before token";
  if (result.match_label.toLowerCase().includes("strong")) return "Worth comparing";
  if (result.match_label.toLowerCase().includes("value")) return "Value-led option";
  return "Explore proof";
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
