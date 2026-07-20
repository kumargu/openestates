import type { LivabilityBrief, EvidenceSection } from "../../lib/types.ts";
import {
  evidenceSectionKindForBrief,
  scrollToEvidenceSection,
} from "../../lib/evidence-nav.ts";

type Props = {
  brief: LivabilityBrief;
  evidenceSections?: EvidenceSection[];
};

const LENS_LABELS: Record<string, string> = {
  operating: "Operating quality",
  risk: "Risk signals",
  positive: "Positive signals",
  judgment: "How to judge",
  lifecycle: "Lifecycle",
};

export function LivabilityBriefCard({ brief, evidenceSections = [] }: Props) {
  const summary = brief.summary_paragraph?.trim();
  const blocks = brief.blocks ?? [];
  if (summary) {
    return (
      <section className="livability-brief" aria-label="Livability brief">
        <div className="livability-brief__header">
          <div className="property-section-heading">
            <span>Before you shortlist</span>
            <h2>Livability brief</h2>
          </div>
        </div>

        {brief.lifecycle_flag && (
          <p className="livability-brief__flag">{formatLifecycleFlag(brief.lifecycle_flag)}</p>
        )}

        <p className="livability-brief__summary">{summary}</p>

        {brief.source_urls.length > 0 && (
          <div className="livability-brief__sources">
            {brief.source_urls.map((url) => (
              <a key={url} href={url} target="_blank" rel="noreferrer">
                Source
              </a>
            ))}
          </div>
        )}
      </section>
    );
  }

  if (blocks.length === 0) return null;

  const handleThemeClick = (lens: string, factKeys?: string[]) => {
    const kind = evidenceSectionKindForBrief(lens, factKeys, evidenceSections);
    if (kind) scrollToEvidenceSection(kind);
  };

  return (
    <section className="livability-brief" aria-label="Livability brief">
      <div className="livability-brief__header">
        <div className="property-section-heading">
          <span>Before you shortlist</span>
          <h2>Livability brief</h2>
        </div>
      </div>

      {brief.lifecycle_flag && (
        <p className="livability-brief__flag">{formatLifecycleFlag(brief.lifecycle_flag)}</p>
      )}

      <div className="livability-brief__blocks">
        {blocks.map((block) => (
          <article key={block.lens} className="livability-brief__block">
            <h3>{block.title || LENS_LABELS[block.lens] || block.lens}</h3>
            <p>{block.paragraph}</p>
            {block.themes.length > 0 && (
              <div className="livability-brief__themes">
                {block.themes.map((theme) => (
                  <button
                    key={theme}
                    type="button"
                    className="livability-brief__chip livability-brief__chip--link"
                    onClick={() => handleThemeClick(block.lens, block.fact_keys)}
                  >
                    {theme}
                  </button>
                ))}
              </div>
            )}
          </article>
        ))}
      </div>

      {brief.source_urls.length > 0 && (
        <div className="livability-brief__sources">
          {brief.source_urls.map((url) => (
            <a key={url} href={url} target="_blank" rel="noreferrer">
              Source
            </a>
          ))}
        </div>
      )}
    </section>
  );
}

function formatLifecycleFlag(flag: string): string {
  switch (flag) {
    case "livability-first":
      return "Reads as a livability-first society";
    case "ready-to-move":
      return "Positioned as ready to move";
    case "understand-before-you-buy":
      return "Understand before you buy";
    default:
      return flag.replaceAll("-", " ");
  }
}
