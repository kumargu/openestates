import type { EvidenceSection } from "./types.ts";

const LENS_SECTION_KINDS: Record<string, string[]> = {
  lifecycle: ["home_state", "rera"],
  risk: ["waterlogging", "environment", "approach_road"],
  operating: ["approach_road", "locality", "market"],
  positive: ["community_reviews", "community_pulse", "nearby"],
  judgment: ["market", "rera"],
};

/** Resolve the best evidence fold kind to scroll to for a brief block lens / fact key. */
export function evidenceSectionKindForBrief(
  lens: string,
  factKeys: string[] | undefined,
  sections: EvidenceSection[],
): string | null {
  const populated = new Set(
    sections
      .filter((section) =>
        section.items.some(
          (item) =>
            (item.values?.some(Boolean) ?? false)
            || (item.value?.trim().length ?? 0) > 0,
        )
        || section.community_pulse != null,
      )
      .map((section) => section.kind),
  );

  for (const factKey of factKeys ?? []) {
    const match = sections.find(
      (section) =>
        populated.has(section.kind)
        && section.items.some((item) => item.key === factKey),
    );
    if (match) return match.kind;
  }

  for (const kind of LENS_SECTION_KINDS[lens] ?? []) {
    if (populated.has(kind)) return kind;
  }

  return null;
}

export function scrollToEvidenceSection(kind: string): void {
  const target = document.getElementById(`evidence-${kind}`);
  if (!target) return;
  target.scrollIntoView({ behavior: "smooth", block: "start" });
  const toggle = target.querySelector<HTMLButtonElement>(".ev-fold__head");
  if (toggle && toggle.getAttribute("aria-expanded") === "false") {
    toggle.click();
  }
}
