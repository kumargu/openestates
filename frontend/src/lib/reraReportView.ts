import type {
  ReraCompareItem,
  ReraDossier,
  ReraDocumentSection,
  ReraReportFact,
  ReraReportSection,
} from "./types.ts";
import type { NotebookLabelId } from "./notebook.ts";

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

function titleLabel(value: string): string {
  const keepUpper = new Set(["bhk", "itpl", "jp", "kr", "noc", "rera", "bbmp", "bda"]);
  return value.split(/\s+/).map((word) => {
    const lower = word.toLowerCase();
    if (keepUpper.has(lower)) return lower.toUpperCase();
    return lower.charAt(0).toUpperCase() + lower.slice(1);
  }).join(" ");
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

export function toneClass(tone?: string): string {
  if (!tone || tone === "neutral" || tone === "default") return "";
  return `is-${tone}`;
}

export function kindLabel(value: string): string {
  const normalized = value.replace(/[_-]+/g, " ").trim();
  return normalized ? titleLabel(displayName(normalized)) : "Document";
}

export function safeLabels(labels: string[] | undefined, key: string): NotebookLabelId[] {
  const next = labels?.filter(Boolean) ?? [];
  if (next.length > 0) return [...new Set(next)].slice(0, 4);
  const keyText = key.toLowerCase();
  if (keyText.includes("complaint")) return ["complaints", "risk", "legal"];
  if (keyText.includes("delay") || keyText.includes("litigation")) return ["risk", "legal"];
  return ["legal"];
}

function compareItemToFact(item: ReraCompareItem, dossier: ReraDossier): ReraReportFact {
  return {
    key: item.key,
    label: item.label,
    value: item.value,
    tone: item.tone,
    labels: safeLabels(item.labels, item.key),
    confidence: 1,
    learned_at: dossier.source.last_verified ?? "",
  };
}

export function reportSections(dossier: ReraDossier): ReraReportSection[] {
  if (dossier.fact_sections?.length) return dossier.fact_sections;

  const facts = dossier.compare_items
    .filter((item) => knownText(item.value))
    .map((item) => compareItemToFact(item, dossier));

  return facts.length > 0 ? [{ id: "facts", title: "Facts", facts }] : [];
}

export function visibleDocumentSections(sections: ReraDocumentSection[]): ReraDocumentSection[] {
  return sections
    .map((section) => {
      const items = section.items?.filter((item) => httpUrl(item.source_url)) ?? [];
      return {
        ...section,
        count: items.length,
        items,
      };
    })
    .filter((section) => section.items.length > 0);
}
