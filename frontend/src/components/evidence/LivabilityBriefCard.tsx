import type { LivabilityBrief } from "../../lib/types.ts";

type Props = {
  brief: LivabilityBrief;
};

const LENS_LABELS: Record<string, string> = {
  operating: "Operating quality",
  risk: "Risk signals",
  positive: "Positive signals",
  judgment: "How to judge",
};

export function LivabilityBriefCard({ brief }: Props) {
  if (brief.blocks.length === 0) return null;

  return (
    <section className="livability-brief" aria-label="Livability brief">
      <div className="livability-brief__header">
        <div className="property-section-heading">
          <span>Before you shortlist</span>
          <h2>Livability brief</h2>
        </div>
        <span className="livability-brief__confidence">{brief.confidence_label}</span>
      </div>

      {brief.lifecycle_flag && (
        <p className="livability-brief__flag">{formatLifecycleFlag(brief.lifecycle_flag)}</p>
      )}

      <div className="livability-brief__blocks">
        {brief.blocks.map((block) => (
          <article key={block.lens} className="livability-brief__block">
            <h3>{block.title || LENS_LABELS[block.lens] || block.lens}</h3>
            <p>{block.paragraph}</p>
            {block.themes.length > 0 && (
              <div className="livability-brief__themes">
                {block.themes.map((theme) => (
                  <span key={theme} className="livability-brief__chip">
                    {theme}
                  </span>
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
