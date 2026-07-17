import type {
  EvidenceSection,
  PropertyCard,
  PropertyDetailResponse,
  PropertyEvidenceResponse,
  SearchResultItem,
  SourcePanel,
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
  confidencePct: number;
  sourceTypes: string[];
  heat: "strong" | "moderate" | "sparse";
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

const SECTION_CONSTELLATION: Record<string, EvidenceConstellation> = {
  market: "value",
  rera: "trust",
  reviews: "lifestyle",
  community: "lifestyle",
  area: "commute",
  nearby: "commute",
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

export function constellationForSection(kind: string): EvidenceConstellation {
  return SECTION_CONSTELLATION[kind] ?? "trust";
}

export function constellationMeta(id: EvidenceConstellation) {
  return CONSTELLATION_META[id];
}

export function visibleEvidenceSections(
  sections: EvidenceSection[] | undefined,
): EvidenceSection[] {
  if (!sections?.length) return [];
  return sections
    .filter((section) => section.items.length > 0)
    .sort((a, b) => a.priority - b.priority);
}

export function summarizeEvidence(
  evidence: PropertyEvidenceResponse | undefined,
): EvidenceSummary | null {
  if (!evidence?.sections?.length) return null;

  const sections = visibleEvidenceSections(evidence.sections);
  if (sections.length === 0) return null;

  const factCount = sections.reduce((sum, s) => sum + s.items.length, 0);
  const gapCount = 0;
  const confidencePct = Math.round(
    sections.reduce((sum, s) => sum + s.confidence_pct, 0) / sections.length,
  );
  const sourceTypes = [
    ...new Set(sections.flatMap((s) => s.source_types)),
  ].slice(0, 4);

  let heat: EvidenceSummary["heat"] = "sparse";
  if (factCount >= 12 && confidencePct >= 70) heat = "strong";
  else if (factCount >= 5 && confidencePct >= 50) heat = "moderate";

  return {
    factCount,
    gapCount,
    sectionCount: sections.length,
    confidencePct,
    sourceTypes,
    heat,
  };
}

export function evidenceHeatClass(heat: EvidenceSummary["heat"]): string {
  if (heat === "strong") return "evidence-heat--strong";
  if (heat === "moderate") return "evidence-heat--moderate";
  return "evidence-heat--sparse";
}

export function groupSectionsByConstellation(
  sections: EvidenceSection[],
): Array<{ id: EvidenceConstellation; sections: EvidenceSection[] }> {
  const buckets = new Map<EvidenceConstellation, EvidenceSection[]>();
  for (const section of sections) {
    const id = constellationForSection(section.kind);
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

export function decisionReadLabel(
  detail: Pick<PropertyDetailResponse, "confidence_score" | "rera" | "market_activity" | "property" | "transparency_score">,
): string {
  const trust =
    detail.confidence_score?.overall != null
      ? Math.round(detail.confidence_score.overall * 100)
      : detail.transparency_score.overall;

  if (trust < 60) return "Verify before token";
  if ((detail.property.litigation_risk ?? 0) > 0.55) return "Risk needs clearing";
  const delta = detail.market_activity.price_vs_median?.pct_diff;
  if (delta != null && Math.abs(delta <= 1 ? delta * 100 : delta) > 8) {
    return "Price needs support";
  }
  return "Calm family bet";
}

export function clusterSearchResults(
  results: SearchResultItem[],
  evidenceById: Map<string, PropertyEvidenceResponse>,
): UniverseCluster[] {
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

    if (summary && (summary.gapCount >= 3 || summary.heat === "sparse")) {
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

export function tileDecisionRead(
  result: SearchResultItem,
  summary: EvidenceSummary | null,
): string {
  if (summary && summary.gapCount >= 3) return "Verify before token";
  if (summary && summary.heat === "sparse") return "Proof still building";
  if (result.confidence_score?.label) return result.confidence_score.label;
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

/** Legacy fallback when detail.evidence is absent. */
export function panelsToSections(panels: SourcePanel[]): EvidenceSection[] {
  return panels.map((panel, index) => ({
    kind: panel.kind ?? "source",
    title: panel.title,
    summary: panel.subtitle,
    subtitle: "",
    priority: index,
    confidence_pct: 50,
    source_types: [...new Set(panel.items.map((item) => item.source_type))],
    entity_ids: panel.items.map((item) => item.entity_id),
    items: panel.items,
    missing: [],
  }));
}
