import type {
  EvidenceSection,
  SourceItem,
} from "./types.ts";

export type EvidenceConstellation =
  | "value"
  | "trust"
  | "lifestyle"
  | "risk"
  | "commute"
  | "investment";

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
