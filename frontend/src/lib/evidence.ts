import type {
  EvidenceSection,
  PropertyCard,
  PropertyEvidenceResponse,
  SearchResultItem,
  SourceItem,
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

export type EvidenceBand = "home" | "area" | "records";

const BAND_META: Record<EvidenceBand, { label: string }> = {
  home: { label: "This home" },
  area: { label: "The area" },
  records: { label: "Trust & records" },
};

/** Maps evidence section kinds to buyer-facing bands on the property page. */
const SECTION_BAND: Record<string, EvidenceBand> = {
  home_state: "records",
  market: "home",
  rera: "records",
  approach_road: "area",
  nearby: "area",
  water_context: "area",
  waterlogging_context: "area",
  surroundings: "area",
  community: "area",
  area: "area",
};

const BAND_ORDER: EvidenceBand[] = ["records", "home", "area"];

export function sectionBand(
  section: Pick<EvidenceSection, "kind" | "constellation">,
): EvidenceBand {
  const mapped = SECTION_BAND[section.kind];
  if (mapped) return mapped;
  const constellation = sectionConstellation(section);
  if (constellation === "trust") return "records";
  if (constellation === "value" || constellation === "investment") return "home";
  return "area";
}

export function bandMeta(band: EvidenceBand) {
  return BAND_META[band];
}

function sortSectionsInBand(band: EvidenceBand, sections: EvidenceSection[]): EvidenceSection[] {
  const sorted = [...sections].sort((a, b) => a.priority - b.priority);
  if (band !== "records") return sorted;
  const reraIndex = sorted.findIndex((section) => section.kind === "rera");
  if (reraIndex <= 0) return sorted;
  const [rera] = sorted.splice(reraIndex, 1);
  return [rera, ...sorted];
}

export function groupSectionsByBand(
  sections: EvidenceSection[],
): Array<{ id: EvidenceBand; sections: EvidenceSection[] }> {
  const buckets = new Map<EvidenceBand, EvidenceSection[]>();
  for (const section of sections) {
    const band = sectionBand(section);
    const list = buckets.get(band) ?? [];
    list.push(section);
    buckets.set(band, list);
  }
  return BAND_ORDER
    .filter((id) => buckets.has(id))
    .map((id) => ({ id, sections: sortSectionsInBand(id, buckets.get(id)!) }));
}

function compactTileText(value: string, max = 72): string {
  const trimmed = value.trim();
  if (trimmed.length <= max) return trimmed;
  return `${trimmed.slice(0, max - 1).trimEnd()}…`;
}

/**
 * Convert leaked snake_case enum codes (e.g. `under_construction`) into readable
 * text (`Under construction`). Leaves URLs, RERA numbers, and slash/digit codes
 * untouched so we never mangle real identifiers.
 */
export function humanizeFactText(value: string): string {
  if (!value || value.includes("://")) return value;
  return value.replace(/\b[a-z]+(?:_[a-z]+)+\b/g, (match) => {
    const words = match.split("_");
    return words
      .map((word, index) => (index === 0 ? word.charAt(0).toUpperCase() + word.slice(1) : word))
      .join(" ");
  });
}

function factDisplayValue(item: SourceItem): string | null {
  const values = item.values?.filter(Boolean) ?? [];
  if (values.length > 0) return compactTileText(humanizeFactText(values[0]), 48);
  if (item.value?.trim()) {
    const raw = humanizeFactText(item.value.trim());
    const colon = raw.indexOf(":");
    if (colon > 0 && colon < 40) return compactTileText(raw.slice(colon + 1).trim(), 48);
    return compactTileText(raw, 48);
  }
  return null;
}

/** Count of discrete facts / list items surfaced in a section. */
export function sectionItemCount(section: EvidenceSection): number {
  let count = 0;
  for (const item of section.items) {
    const values = item.values?.filter(Boolean) ?? [];
    if (values.length > 1) count += values.length;
    else if (values.length === 1 || (item.value && item.value.trim().length > 0)) count += 1;
  }
  if (section.community_pulse) {
    const pulse = section.community_pulse;
    count += pulse.positives.length + pulse.concerns.length + (pulse.paragraph.trim() ? 1 : 0);
  }
  for (const strip of section.media ?? []) {
    count += strip.frames.filter((frame) => frame.image_url).length;
  }
  return count;
}

/** One-line headline for a closed topic tile. */
export function sectionTileSignal(section: EvidenceSection): string {
  if (section.summary?.trim()) return compactTileText(humanizeFactText(section.summary));
  if (section.community_pulse) {
    const pulse = section.community_pulse;
    if (pulse.sentiment_band?.trim()) return compactTileText(pulse.sentiment_band);
    if (pulse.positives[0]) return compactTileText(pulse.positives[0]);
    if (pulse.paragraph?.trim()) return compactTileText(pulse.paragraph);
  }

  const facts = section.items.filter(
    (it) => (it.values?.some(Boolean) ?? false) || (it.value && it.value.trim().length > 0),
  );
  const snippets = facts
    .map(factDisplayValue)
    .filter((value): value is string => value !== null)
    .slice(0, 2);
  if (snippets.length > 0) return snippets.join(" · ");

  return section.subtitle ? compactTileText(section.subtitle) : section.title;
}

/** Badge count for tiles with multiple facts or list items. */
export function sectionTileCount(section: EvidenceSection): number | null {
  const count = sectionItemCount(section);
  return count >= 2 ? count : null;
}

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

function isBuyerVisibleSource(sourceType: string | undefined): boolean {
  if (!sourceType) return false;
  const lowered = sourceType.trim().toLowerCase();
  return lowered.includes("rera") || lowered.includes("google");
}

/** Hide pipeline source types from buyer-facing UI. */
export function displaySourceType(sourceType: string | undefined): string | null {
  if (!sourceType) return null;
  const lowered = sourceType.trim().toLowerCase();
  if (INTERNAL_SOURCE_TYPES.has(lowered)) return null;
  if (!isBuyerVisibleSource(sourceType)) return null;
  if (lowered === "rera") return "RERA";
  if (lowered.includes("google")) return "Google";
  return sourceType;
}

export function canShowBuyerSource(sourceType: string | undefined): boolean {
  return isBuyerVisibleSource(sourceType);
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

  const defs: Array<{ id: UniverseClusterId; label: string }> = [
    { id: "strong_fits", label: "Best matches" },
    { id: "worth_comparing", label: "More homes" },
    { id: "verify_proof", label: "Less evidence" },
    { id: "value_angle", label: "Lower priced" },
    { id: "explore", label: "Broader matches" },
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
